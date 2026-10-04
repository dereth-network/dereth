//! Combat: readiness, the swing, the stances, the mode toggle, the power bar and the combat window.
//!
//! Every scenario here opens the retail dats under `$DERETH_TEST_DAT_DIR`, and **this binary must run
//! serially**: two headless clients in one process share the UI request globals. `ALL` is this
//! file's own list, concatenated with the other subjects' in `census.rs`.
//!
//! **Two shapes**, the same two `world.rs` uses. A claim about the combat *model* -- the readiness
//! predicates, the charge, the acknowledgement -- drives the client's own frame slot
//! (`dereth_client::interaction::use_time`, which is what `App::frame` calls) at clocks the
//! scenario chooses, opens the dats itself for the body and the region, and books the claim through
//! `assert_behaviour` on a model client. A claim about a gesture the player makes with the shipped
//! keymap builds a whole client with [`dereth_testkit::ClientSpec::gameplay`] and fires real input
//! events through its own input manager.
//!
//! **What a request counts as here.** A scenario reads the client's own per-frame outbox
//! (`Interaction::last_sent`, which the frame replaces every pass), one layer before the bytes.
//! **The cancel that breaks an automatic attack is the exception**: the abort puts its message
//! straight on the flow queue, so it never reaches that outbox, and `dereth_testkit::wire` reads it
//! off the datagrams. Nothing here binds a socket or sends a datagram: the endpoint only queues.

use dereth_testkit::{ClientSpec, HeadlessClient};

use support::{a_melee_bench, cancels, key, melee_attacks, settled, store, world_in, Bench};

/// Every scenario in this file, for the census: the name, **the behaviour ids the
/// scenario asserts**, and the function.
pub static ALL: &[dereth_testkit::behaviours::Scenario] = &[
    (
        "ready_is_the_bodys_own_motion_queue",
        &["combat.readiness.is-the-bodys-own-motion-queue-and-not-whether-it-is-on-the-ground"],
        ready_is_the_bodys_own_motion_queue,
    ),
    (
        "a_stance_change_the_body_refuses_is_queued",
        &["combat.mode.a-change-the-body-is-not-ready-for-is-queued-and-nothing-is-sent"],
        a_stance_change_the_body_refuses_is_queued,
    ),
    (
        "the_two_flavours_disagree_on_one_body",
        &["combat.readiness.a-swing-and-a-stance-change-ask-different-questions-of-one-body"],
        the_two_flavours_disagree_on_one_body,
    ),
    (
        "a_melee_swing_needs_the_weapons_combat_table",
        &["combat.readiness.a-melee-swing-needs-the-weapons-own-combat-table"],
        a_melee_swing_needs_the_weapons_combat_table,
    ),
    (
        "a_missile_swing_needs_the_stance_and_the_ready_command",
        &["combat.readiness.a-missile-swing-needs-the-stance-up-and-the-body-standing-ready"],
        a_missile_swing_needs_the_stance_and_the_ready_command,
    ),
    (
        "all_three_attack_arms_consume_the_produced_flavour",
        &["combat.readiness.the-window-the-keyboard-and-the-shards-reply-all-ask-the-body"],
        all_three_attack_arms_consume_the_produced_flavour,
    ),
    (
        "one_click_charges_the_bar_and_swings_when_it_fills",
        &["combat.attack.a-click-charges-the-bar-and-the-swing-comes-when-it-fills"],
        one_click_charges_the_bar_and_swings_when_it_fills,
    ),
    (
        "a_held_control_charges_to_full_and_swings_on_release",
        &["combat.attack.a-held-control-charges-to-full-and-swings-only-on-release"],
        a_held_control_charges_to_full_and_swings_on_release,
    ),
    (
        "the_button_and_the_key_start_the_same_charge",
        &["combat.attack.the-window-button-and-the-key-start-the-same-charge"],
        the_button_and_the_key_start_the_same_charge,
    ),
    (
        "a_clean_acknowledgement_rearms_the_loop",
        &["combat.attack-done.a-clean-acknowledgement-rearms-the-loop-and-sends-nothing"],
        a_clean_acknowledgement_rearms_the_loop,
    ),
    (
        "any_other_acknowledgement_cancels_the_repeat",
        &["combat.attack-done.any-other-value-cancels-the-repeat-and-empties-the-bar"],
        any_other_acknowledgement_cancels_the_repeat,
    ),
    (
        "a_moved_slider_refires_at_its_new_power",
        &["combat.attack-done.a-moved-slider-refires-the-swing-at-its-new-power"],
        a_moved_slider_refires_at_its_new_power,
    ),
    (
        "the_acknowledgement_writes_no_line",
        &["combat.attack-done.the-acknowledgement-writes-no-line-whatever-it-carries"],
        the_acknowledgement_writes_no_line,
    ),
    (
        "walking_backwards_breaks_a_repeating_swing",
        &["combat.auto-attack.walking-backwards-breaks-a-repeating-swing"],
        walking_backwards_breaks_a_repeating_swing,
    ),
    (
        "a_release_a_turn_and_an_idle_walk_cancel_nothing",
        &["combat.auto-attack.a-release-a-turn-and-an-idle-walk-cancel-nothing"],
        a_release_a_turn_and_an_idle_walk_cancel_nothing,
    ),
    (
        "a_damage_line_says_what_was_hit_how_hard_and_with_what",
        &["combat.damage-line.says-what-was-hit-how-hard-and-with-what"],
        a_damage_line_says_what_was_hit_how_hard_and_with_what,
    ),
    (
        "the_verb_steps_at_the_thresholds",
        &["combat.damage-line.the-verb-steps-at-the-thresholds-and-an-unranked-hit-just-hits"],
        the_verb_steps_at_the_thresholds,
    ),
    (
        "a_critical_a_sneak_and_a_reckless_swing_each_say_so",
        &["combat.damage-line.a-critical-a-sneak-and-a-reckless-swing-each-say-so"],
        a_critical_a_sneak_and_a_reckless_swing_each_say_so,
    ),
    (
        "both_lines_reach_the_window_and_a_squelch_stops_them",
        &["combat.damage-line.both-lines-reach-the-window-and-a-squelch-stops-them"],
        both_lines_reach_the_window_and_a_squelch_stops_them,
    ),
    (
        "exactly_one_combat_map_is_registered_and_it_follows_the_mode",
        &["combat.input-map.exactly-one-is-registered-and-it-follows-the-mode"],
        exactly_one_combat_map_is_registered_and_it_follows_the_mode,
    ),
    (
        "the_mode_decides_which_map_takes_the_contested_keys",
        &["combat.input-map.the-mode-decides-which-map-takes-the-contested-keys"],
        the_mode_decides_which_map_takes_the_contested_keys,
    ),
    (
        "every_shipped_quickslot_key_reaches_the_toolbar",
        &["combat.input-map.every-shipped-quickslot-key-reaches-the-toolbar-except-the-nine-magic-takes"],
        every_shipped_quickslot_key_reaches_the_toolbar,
    ),
    (
        "the_combat_map_itself_is_always_up_and_never_moves",
        &["combat.input-map.the-combat-map-itself-is-always-up-and-never-moves"],
        the_combat_map_itself_is_always_up_and_never_moves,
    ),
    (
        "a_running_client_swaps_the_keys_the_frame_after_the_mode_changes",
        &["combat.input-map.a-running-client-swaps-the-keys-the-frame-after-the-mode-changes"],
        a_running_client_swaps_the_keys_the_frame_after_the_mode_changes,
    ),
    (
        "a_combat_mode_change_in_the_model_opens_the_window",
        &["combat.mode.a-change-in-the-model-opens-the-window-and-closing-combat-shuts-it"],
        a_combat_mode_change_in_the_model_opens_the_window,
    ),
    (
        "the_shards_own_word_opens_the_window_and_the_client_sends_nothing",
        &["combat.mode.the-shards-own-word-opens-the-window-and-the-client-sends-nothing"],
        the_shards_own_word_opens_the_window_and_the_client_sends_nothing,
    ),
    (
        "an_update_that_is_not_this_players_or_is_older_is_refused",
        &["combat.mode.an-update-that-is-not-this-players-or-is-older-than-the-last-is-refused"],
        an_update_that_is_not_this_players_or_is_older_is_refused,
    ),
    (
        "the_recorded_combat_mode_changes_reach_the_window_the_buttons_and_the_keys",
        &["combat.mode.the-recorded-changes-reach-the-window-the-buttons-and-the-keys"],
        the_recorded_combat_mode_changes_reach_the_window_the_buttons_and_the_keys,
    ),
    (
        "the_combat_option_boxes_draw_their_shipped_captions",
        &["combat.window.the-option-boxes-draw-their-shipped-captions-and-carry-their-shipped-help"],
        the_combat_option_boxes_draw_their_shipped_captions,
    ),
    (
        "the_attack_height_buttons_follow_the_notice",
        &["combat.window.the-attack-height-buttons-follow-the-notice-and-draw-the-chosen-one"],
        the_attack_height_buttons_follow_the_notice,
    ),
    (
        "pressing_the_chosen_height_again_keeps_it_chosen",
        &["combat.window.pressing-the-chosen-height-again-keeps-it-chosen"],
        pressing_the_chosen_height_again_keeps_it_chosen,
    ),
    (
        "choosing_one_height_leaves_every_other_button_alone",
        &["combat.window.choosing-one-height-leaves-every-other-button-alone"],
        choosing_one_height_leaves_every_other_button_alone,
    ),
    (
        "the_vital_rows_are_drawn_into_the_shipped_template",
        &["vitals.row.the-numbers-are-drawn-into-the-shipped-template-in-both-layouts"],
        the_vital_rows_are_drawn_into_the_shipped_template,
    ),
    (
        "a_terminal_answer_empties_the_classic_meter_and_the_next_charge_works",
        &["combat.power-bar.a-terminal-answer-empties-the-classic-meter-and-the-next-charge-works"],
        a_terminal_answer_empties_the_classic_meter_and_the_next_charge_works,
    ),
    (
        "the_standalone_bar_and_the_classic_meter_are_separate_routes",
        &["combat.power-bar.the-standalone-bar-and-the-classic-meter-are-separate-routes"],
        the_standalone_bar_and_the_classic_meter_are_separate_routes,
    ),
    (
        "a_jump_raises_the_standalone_bar_and_not_the_classic_one",
        &["combat.power-bar.a-jump-raises-the-standalone-bar-and-not-the-classic-one"],
        a_jump_raises_the_standalone_bar_and_not_the_classic_one,
    ),
    (
        "a_notice_with_no_panel_to_hear_it_is_dropped",
        &["combat.power-bar.a-notice-with-no-panel-to-hear-it-is-dropped-and-never-replayed"],
        a_notice_with_no_panel_to_hear_it_is_dropped,
    ),
    (
        "the_power_bar_follows_the_clock_at_one_full_charge_a_second",
        &["combat.power-bar.it-follows-the-clock-at-one-full-charge-a-second"],
        the_power_bar_follows_the_clock_at_one_full_charge_a_second,
    ),
    (
        "a_two_handed_style_charges_a_quarter_faster",
        &["combat.power-bar.a-two-handed-style-charges-a-quarter-faster-over-the-same-frames"],
        a_two_handed_style_charges_a_quarter_faster,
    ),
    (
        "exactly_one_of_the_two_power_displays_is_live",
        &["combat.power-bar.exactly-one-of-the-two-displays-is-live-and-they-never-differ"],
        exactly_one_of_the_two_power_displays_is_live,
    ),
    (
        "the_power_bar_is_shown_when_a_charge_begins_and_emptied_when_it_ends",
        &["combat.power-bar.it-is-shown-when-the-charge-begins-and-hidden-and-emptied-when-it-ends"],
        the_power_bar_is_shown_when_a_charge_begins_and_emptied_when_it_ends,
    ),
    (
        "the_keyboard_gauge_has_seven_notches_and_starts_half_way",
        &["combat.power-bar.the-keyboard-gauge-has-seven-notches-and-starts-half-way"],
        the_keyboard_gauge_has_seven_notches_and_starts_half_way,
    ),
    (
        "the_power_drag_is_continuous_and_lands_between_the_notches",
        &["combat.power-bar.the-drag-is-continuous-and-lands-between-the-notches"],
        the_power_drag_is_continuous_and_lands_between_the_notches,
    ),
    (
        "the_notch_and_the_fill_are_different_things",
        &["combat.power-bar.the-notch-and-the-fill-are-different-things-on-different-elements"],
        the_notch_and_the_fill_are_different_things,
    ),
    (
        "the_display_holds_the_level_the_swing_went_out_at",
        &["combat.power-bar.the-display-holds-the-level-the-swing-went-out-at"],
        the_display_holds_the_level_the_swing_went_out_at,
    ),
    (
        "the_advanced_combat_option_suppresses_the_classic_combat_window",
        &["combat.advanced.the-option-suppresses-the-classic-combat-window"],
        the_advanced_combat_option_suppresses_the_classic_combat_window,
    ),
    (
        "turning_the_advanced_option_on_while_the_window_is_up_takes_it_down",
        &["combat.advanced.turning-the-option-on-while-the-window-is-up-takes-it-down"],
        turning_the_advanced_option_on_while_the_window_is_up_takes_it_down,
    ),
    (
        "the_strip_comes_up_while_an_attack_key_is_held_and_goes_on_release",
        &["combat.advanced.the-strip-comes-up-while-an-attack-key-is-held-and-goes-on-release"],
        the_strip_comes_up_while_an_attack_key_is_held_and_goes_on_release,
    ),
    (
        "a_release_that_reaches_the_shard_holds_the_strip_until_the_answer",
        &["combat.advanced.a-release-that-reaches-the-shard-holds-the-strip-until-the-answer"],
        a_release_that_reaches_the_shard_holds_the_strip_until_the_answer,
    ),
    (
        "the_advanced_option_chooses_which_display_is_live",
        &["combat.advanced.the-option-chooses-which-display-is-live-and-only-one-ever-is"],
        the_advanced_option_chooses_which_display_is_live,
    ),
    (
        "the_advanced_option_is_off_at_login_and_the_toggle_is_one_bit",
        &["combat.advanced.the-option-is-off-at-login-and-the-toggle-is-one-bit-on-the-wire"],
        the_advanced_option_is_off_at_login_and_the_toggle_is_one_bit,
    ),
    (
        "the_recorded_attacks_are_charges_after_the_toggle_and_notches_before",
        &["combat.advanced.the-recorded-attacks-are-charges-after-the-toggle-and-notches-before"],
        the_recorded_attacks_are_charges_after_the_toggle_and_notches_before,
    ),
    (
        "one_set_of_keys_means_three_different_things",
        &["combat.input-map.one-set-of-keys-means-three-different-things-and-the-mode-picks-which"],
        one_set_of_keys_means_three_different_things,
    ),
    (
        "every_height_key_moves_the_height_in_both_modes",
        &["combat.attack.every-height-key-moves-the-height-in-both-modes-and-the-swing-carries-it"],
        every_height_key_moves_the_height_in_both_modes,
    ),
    (
        "the_gauge_keys_step_by_a_sixth_in_both_modes",
        &["combat.power-bar.the-gauge-keys-step-by-a-sixth-in-both-modes-and-saturate"],
        the_gauge_keys_step_by_a_sixth_in_both_modes,
    ),
    (
        "a_value_between_notches_rounds_to_the_nearest_before_stepping",
        &["combat.power-bar.a-value-between-notches-rounds-to-the-nearest-before-stepping"],
        a_value_between_notches_rounds_to_the_nearest_before_stepping,
    ),
    (
        "holding_a_height_key_refuses_once_and_not_once_per_repeat",
        &["combat.attack.holding-a-height-key-refuses-once-and-not-once-per-repeat"],
        holding_a_height_key_refuses_once_and_not_once_per_repeat,
    ),
    (
        "at_peace_no_combat_key_reaches_an_arm",
        &["combat.mode.at-peace-no-combat-key-reaches-an-arm"],
        at_peace_no_combat_key_reaches_an_arm,
    ),
    (
        "every_control_in_the_combat_window_does_what_the_client_does",
        &["combat.window.every-control-does-what-the-client-does"],
        every_control_in_the_combat_window_does_what_the_client_does,
    ),
    (
        "the_combat_window_reflects_the_height_the_power_and_the_notch",
        &["combat.window.it-reflects-the-height-the-power-and-the-notch"],
        the_combat_window_reflects_the_height_the_power_and_the_notch,
    ),
    (
        "the_frames_own_pass_drives_the_combat_windows_read_backs",
        &["combat.window.the-frames-own-pass-drives-the-read-backs"],
        the_frames_own_pass_drives_the_combat_windows_read_backs,
    ),
    (
        "the_recklessness_meter_appears_at_exactly_the_trained_class",
        &["combat.window.the-recklessness-meter-appears-at-exactly-the-trained-class"],
        the_recklessness_meter_appears_at_exactly_the_trained_class,
    ),
];

/// Run one of this file's scenarios under a recorder, and check that the behaviour ids it
/// asserted are exactly the ones its [`ALL`] entry declares.
fn scenario(name: &str) {
    dereth_testkit::behaviours::run_scenario(ALL, name);
}

// ---------------------------------------------------------------------------------------------
// combat.readiness.* and combat.mode.a-change-the-body-is-not-ready-for-…
//
// The body is one real `Character` on the retail terrain, settled and then jumped, asked the same
// question twice.
// ---------------------------------------------------------------------------------------------

/// Readiness is the body's own motion queue, in both directions, and it is not ground contact.
pub fn ready_is_the_bodys_own_motion_queue() {
    use dereth_client::character::CharacterInput;
    use dereth_client::interaction::Interaction;
    use dereth_primitives::LocalTime;

    let store = store();
    let (mut body, mut f) = settled(&store);
    let mut inter = Interaction::new();

    // Settled: nothing outstanding, and the body is on the ground.
    inter.note_player_physics(Some(&body));
    let settled_is_ready = inter.player_ready() && inter.player_on_ground() == Some(true);
    let changed = inter.stats.ready_answers_changed;

    // Jumped: the body's own input queues a motion, and the same predicate turns over.
    body.input = CharacterInput {
        jump: true,
        ..CharacterInput::default()
    };
    f += 1;
    body.update(LocalTime(f64::from(f) / 30.0));
    body.input = CharacterInput::default();
    for _ in 0..2 {
        f += 1;
        body.update(LocalTime(f64::from(f) / 30.0));
    }
    inter.note_player_physics(Some(&body));
    let jumped_is_not_ready = !inter.player_ready();
    // A counter, so that "always false" and "never produced" are not the same reading.
    let the_answer_moved = inter.stats.ready_answers_changed == changed + 1;

    // …and it is a reading rather than a latch: the motion drains and the answer comes back,
    // while the body is still in the air -- which is the pair of readings ground contact cannot
    // produce.
    let mut ready_again_in_the_air = false;
    for _ in 0..20 {
        for _ in 0..5 {
            f += 1;
            body.update(LocalTime(f64::from(f) / 30.0));
        }
        inter.note_player_physics(Some(&body));
        if inter.player_ready() {
            ready_again_in_the_air = inter.player_on_ground() == Some(false);
            break;
        }
    }

    // A body walking on flat ground is the other separation: in contact, and never idle.
    let (mut walking, mut g) = settled(&store);
    let mut inter2 = Interaction::new();
    walking.input = CharacterInput {
        forward: true,
        ..CharacterInput::default()
    };
    for _ in 0..10 {
        g += 1;
        walking.update(LocalTime(f64::from(g) / 30.0));
    }
    walking.input = CharacterInput::default();
    inter2.note_player_physics(Some(&walking));
    let walking_is_on_the_ground_and_not_ready =
        inter2.player_on_ground() == Some(true) && !inter2.player_ready();

    // And no body at all is the client's own outright `false`, not a third state.
    inter2.note_player_physics(None);
    let no_body_is_false = !inter2.player_ready() && inter2.player_on_ground().is_none();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.readiness.is-the-bodys-own-motion-queue-and-not-whether-it-is-on-the-ground",
        move |_| {
            settled_is_ready
                && jumped_is_not_ready
                && the_answer_moved
                && ready_again_in_the_air
                && walking_is_on_the_ground_and_not_ready
                && no_body_is_false
        },
    );
}

/// A stance change the body is not ready for is parked, and nothing is built or sent.
pub fn a_stance_change_the_body_refuses_is_queued() {
    use dereth_client::interaction::{self, Interaction};
    use dereth_client_model::combat::CombatMode;
    use dereth_primitives::LocalTime;

    let store = store();
    let (body, _) = settled(&store);
    let mut inter = Interaction::new();
    let mut objects = dereth_client::objects::ObjectStream::default();
    objects.world = world_in(CombatMode::NonCombat, true);

    // Note a ready body first, so the frame is shown to overwrite it rather than to have found
    // `false` sitting there by default.
    inter.note_player_physics(Some(&body));
    let ready_before_the_frame = inter.player_ready();

    let _ = interaction::use_time(
        &mut inter,
        &store,
        None,
        &mut objects,
        None,
        vec![key(interaction::action::COMBAT_TOGGLE_COMBAT, true)],
        false,
        (1024, 768),
        LocalTime(2.0),
    );

    let re_read_from_the_frame = !inter.player_ready();
    let parked = objects.world.combat.pending_combat_mode == CombatMode::Melee;
    let did_not_move = objects.world.combat.combat_mode == CombatMode::NonCombat;
    let nothing_sent = inter.last_sent.is_empty();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.mode.a-change-the-body-is-not-ready-for-is-queued-and-nothing-is-sent",
        move |_| {
            ready_before_the_frame
                && re_read_from_the_frame
                && parked
                && did_not_move
                && nothing_sent
        },
    );
}

/// One body, one world, one instant: the swing is allowed and the stance change is not.
pub fn the_two_flavours_disagree_on_one_body() {
    use dereth_client::character::CharacterInput;
    use dereth_client::interaction::Interaction;
    use dereth_client_model::combat::CombatMode;
    use dereth_primitives::LocalTime;

    let store = store();
    let (mut body, mut f) = settled(&store);
    let world = world_in(CombatMode::Melee, true);
    let mut inter = Interaction::new();

    // A: settled. Both flavours agree -- the control that stops an "always true" attack flavour
    // and an "always false" strict one from passing a single station.
    inter.note_player_physics(Some(&body));
    let a = inter.player_motions_pending() == Some(false)
        && inter.ready_for_attack(&world)
        && inter.ready_for_mode_change(&world);

    // B: the same body mid-motion. This is the split.
    body.input = CharacterInput {
        jump: true,
        ..CharacterInput::default()
    };
    f += 1;
    body.update(LocalTime(f64::from(f) / 30.0));
    body.input = CharacterInput::default();
    for _ in 0..2 {
        f += 1;
        body.update(LocalTime(f64::from(f) / 30.0));
    }
    inter.note_player_physics(Some(&body));
    let b = inter.player_motions_pending() == Some(true)
        && inter.ready_for_attack(&world)
        && !inter.ready_for_mode_change(&world);

    // C: the motion drains and they agree again, so B is a reading and not a latch.
    let mut c_agrees = false;
    for _ in 0..20 {
        for _ in 0..5 {
            f += 1;
            body.update(LocalTime(f64::from(f) / 30.0));
        }
        inter.note_player_physics(Some(&body));
        if inter.ready_for_mode_change(&world) {
            c_agrees = inter.ready_for_attack(&world);
            break;
        }
    }

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.readiness.a-swing-and-a-stance-change-ask-different-questions-of-one-body",
        move |_| a && b && c_agrees,
    );
}

/// No combat table on the wielded weapon and the lenient flavour refuses too.
pub fn a_melee_swing_needs_the_weapons_combat_table() {
    use dereth_client::interaction::Interaction;
    use dereth_client_model::combat::CombatMode;

    let store = store();
    let (body, _) = settled(&store);
    let mut inter = Interaction::new();
    inter.note_player_physics(Some(&body));

    let no_table = world_in(CombatMode::Melee, false);
    let fixture_really_has_none = no_table.combat_table_did().is_none();
    let refused_either_way =
        !inter.ready_for_attack(&no_table) && !inter.ready_for_mode_change(&no_table);

    // The known positive, from the identical body: the only thing that changed is the quality.
    let with_table = world_in(CombatMode::Melee, true);
    let allowed_with_it = inter.ready_for_attack(&with_table);

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.readiness.a-melee-swing-needs-the-weapons-own-combat-table",
        move |_| fixture_really_has_none && refused_either_way && allowed_with_it,
    );
}

/// The missile arm wants the stance up **and** the body standing in its ready command.
pub fn a_missile_swing_needs_the_stance_and_the_ready_command() {
    use dereth_client::interaction::Interaction;
    use dereth_client_model::combat::{CombatMode, MISSILE_READY_STYLES};

    let store = store();
    let (body, _) = settled(&store);
    let mut inter = Interaction::new();
    inter.note_player_physics(Some(&body));

    let mut w = world_in(CombatMode::Missile, true);
    // A settled body's stance is the peace one -- what a player is in before drawing a bow.
    let stance_down_refuses = !inter.ready_for_attack(&w);

    w.combat.current_style = MISSILE_READY_STYLES[0];
    let stance_up_allows = inter.ready_for_attack(&w);

    // The stance is up but the body is walking forward rather than standing ready.
    w.combat.forward_command = 0x4000_0004;
    let a_moving_body_refuses = !inter.ready_for_attack(&w);

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.readiness.a-missile-swing-needs-the-stance-up-and-the-body-standing-ready",
        move |_| stance_down_refuses && stance_up_allows && a_moving_body_refuses,
    );
}

/// The window's button, the keyboard's frame slot and the shard's acknowledgement all ask the
/// body, and all three refuse the same state.
pub fn all_three_attack_arms_consume_the_produced_flavour() {
    use dereth_client::interaction::{self, Interaction};
    use dereth_client_model::combat::{CombatMode, MISSILE_READY_STYLES};
    use dereth_primitives::{LocalTime, ServerTime};
    use dereth_ui_screens::view::UiRequest;

    let store = store();

    // 1. The combat window's own arm, through the interaction layer's UI request queue.
    let by_window = |mode: CombatMode, style: u32| -> bool {
        let mut w = world_in(mode, true);
        w.combat.current_style = style;
        let mut inter = Interaction::new();
        inter.queue(
            Vec::new(),
            vec![UiRequest::CombatSetAttackHeight { height: 2 }],
        );
        let _ = inter.run_ui_requests(&mut w, false, ServerTime(100.0));
        w.combat.build_in_progress
    };

    // 2. The keyboard path, through the production frame slot.
    let by_key = |mode: CombatMode, style: u32| -> bool {
        let mut w = world_in(mode, true);
        w.combat.current_style = style;
        let mut objects = dereth_client::objects::ObjectStream::default();
        objects.world = w;
        let mut inter = Interaction::new();
        let _ = interaction::use_time(
            &mut inter,
            &store,
            None,
            &mut objects,
            None,
            vec![key(interaction::action::COMBAT_MEDIUM_ATTACK, true)],
            false,
            (800, 600),
            LocalTime(100.0),
        );
        objects.world.combat.build_in_progress
    };

    // The peace stance is what a settled missile character stands in; the bow stance is the first
    // one the client accepts.
    const DOWN: u32 = 0x8000_003D;
    let up = MISSILE_READY_STYLES[0];

    let window = by_window(CombatMode::Melee, DOWN)
        && !by_window(CombatMode::Missile, DOWN)
        && by_window(CombatMode::Missile, up);
    let keyboard = by_key(CombatMode::Melee, DOWN)
        && !by_key(CombatMode::Missile, DOWN)
        && by_key(CombatMode::Missile, up);

    // 3. The shard's acknowledgement re-arms the build, and reads the body at the moment it
    // arrives rather than carrying the answer from the press.
    let mut w = world_in(CombatMode::Missile, true);
    w.combat.current_style = DOWN;
    let mut objects = dereth_client::objects::ObjectStream::default();
    objects.world = w;
    let mut inter = Interaction::new();
    let _ = interaction::use_time(
        &mut inter,
        &store,
        None,
        &mut objects,
        None,
        vec![key(interaction::action::COMBAT_MEDIUM_ATTACK, true)],
        false,
        (800, 600),
        LocalTime(100.0),
    );
    let request_open_but_no_build =
        objects.world.combat.attack_request_in_progress && !objects.world.combat.build_in_progress;

    let done = || dereth_client_net::client_session::SessionEvent::UiEvent {
        opcode: dereth_protocol::Opcode::COMBAT_HANDLE_ATTACK_DONE_EVENT,
        blob: vec![0u8; 8],
    };
    interaction::apply_events(&mut inter, &[done()], &mut objects.world);
    let stance_down_refuses_the_rearm = !objects.world.combat.build_in_progress;
    objects.world.combat.current_style = up;
    interaction::apply_events(&mut inter, &[done()], &mut objects.world);
    let stance_up_rearms = objects.world.combat.build_in_progress;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.readiness.the-window-the-keyboard-and-the-shards-reply-all-ask-the-body",
        move |_| {
            window
                && keyboard
                && request_open_but_no_build
                && stance_down_refuses_the_rearm
                && stance_up_rearms
        },
    );
}

// ---------------------------------------------------------------------------------------------
// combat.attack.*
//
// The bench's calibration -- that it reaches the wire at all -- is folded into each scenario as
// the known positive beside the silence.
// ---------------------------------------------------------------------------------------------

/// A click starts a charge; the swing comes on the frame the bar reaches the gauge setting.
pub fn one_click_charges_the_bar_and_swings_when_it_fills() {
    use dereth_client::interaction::action as ia;
    use dereth_client_model::combat::PowerBarMode;

    let mut b = Bench::new();
    let gauge_starts_half_way = (b.combat().ui_requested_power - 0.5).abs() < 1e-9;

    // The click: press and release in the same frame.
    b.frame(
        vec![
            key(ia::COMBAT_MEDIUM_ATTACK, true),
            key(ia::COMBAT_MEDIUM_ATTACK, false),
        ],
        10.0,
    );
    let the_release_does_not_swing = b.attacks().is_empty()
        && b.combat().build_in_progress
        && !b.combat().attack_request_in_progress
        && b.combat().power_bar_mode == PowerBarMode::Combat;

    // Four frames of charging: nothing is sent, and the bar tracks the clock. The silence is per
    // frame -- "eventually one attack" is satisfied by a swing that fired immediately.
    let mut charges_quietly = true;
    for i in 1..=4 {
        let t = 10.0 + f64::from(i) * 0.1;
        b.frame(Vec::new(), t);
        #[allow(clippy::cast_possible_truncation)]
        let want = (f64::from(i) * 0.1) as f32;
        charges_quietly &=
            b.attacks().is_empty() && (b.combat().latest_power_bar_level - want).abs() < 1e-5;
    }

    // Arrival, and it is arrival rather than "still charging" at exactly the cap.
    b.frame(Vec::new(), 10.5);
    let one_swing_at_the_gauge = b.attacks() == vec![0.5] && !b.combat().build_in_progress;

    // Nothing repeats it: the loop restarts only on the shard's acknowledgement.
    for i in 6..12 {
        b.frame(Vec::new(), 10.0 + f64::from(i) * 0.1);
    }
    let exactly_one_per_click = b.attacks().len() == 1;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.attack.a-click-charges-the-bar-and-the-swing-comes-when-it-fills",
        move |_| {
            gauge_starts_half_way
                && the_release_does_not_swing
                && charges_quietly
                && one_swing_at_the_gauge
                && exactly_one_per_click
        },
    );
}

/// Held, the bar charges past the gauge to full and fires nothing; the release swings.
pub fn a_held_control_charges_to_full_and_swings_on_release() {
    use dereth_client::interaction::action as ia;
    use dereth_client_model::combat::AttackHeight;

    let mut b = Bench::new();
    b.frame(vec![key(ia::COMBAT_LOW_ATTACK, true)], 50.0);
    let asks_for_full_power = (b.combat().requested_attack_power - 1.0).abs() < 1e-9;

    // Well past the gauge setting and past full charge. A client that fired on arrival regardless
    // of the held flag would have swung at the first of these.
    let mut silent_while_held = true;
    for t in [50.2, 50.5, 50.8, 51.0, 51.5, 52.0] {
        b.frame(Vec::new(), t);
        silent_while_held &= b.attacks().is_empty() && b.combat().attack_request_in_progress;
    }
    let saturates_at_full = (b.combat().latest_power_bar_level - 1.0).abs() < 1e-9;

    b.frame(vec![key(ia::COMBAT_LOW_ATTACK, false)], 52.1);
    // **Two**, and it is the client's: the release swings at the level reached and again clamped
    // to the gauge setting. That is what makes a hold and a click tellable apart.
    let release_swings =
        b.attacks() == vec![1.0, 0.5] && b.combat().requested_attack_height == AttackHeight::Low;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.attack.a-held-control-charges-to-full-and-swings-only-on-release",
        move |_| asks_for_full_power && silent_while_held && saturates_at_full && release_swings,
    );
}

/// The window's two messages and the key's two edges start the very same charge.
pub fn the_button_and_the_key_start_the_same_charge() {
    use dereth_client::interaction::action as ia;
    use dereth_client_model::combat::AttackHeight;
    use dereth_ui_screens::view::UiRequest;

    let by_key = {
        let mut b = Bench::new();
        b.frame(
            vec![
                key(ia::COMBAT_HIGH_ATTACK, true),
                key(ia::COMBAT_HIGH_ATTACK, false),
            ],
            10.0,
        );
        b.combat().clone()
    };
    let by_button = {
        let mut b = Bench::new();
        b.queue(vec![
            UiRequest::CombatSetAttackHeight {
                height: AttackHeight::High as u32,
            },
            UiRequest::CombatEndAttack {
                height: AttackHeight::High as u32,
            },
        ]);
        b.frame(Vec::new(), 10.0);
        b.combat().clone()
    };

    let same_height = by_key.requested_attack_height == AttackHeight::High
        && by_button.requested_attack_height == AttackHeight::High;
    let both_started = by_key.build_in_progress && by_button.build_in_progress;
    let at_the_same_instant = (by_key.build_start_time - by_button.build_start_time).abs() < 1e-9;
    let both_ended_the_request =
        !by_key.attack_request_in_progress && !by_button.attack_request_in_progress;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.attack.the-window-button-and-the-key-start-the-same-charge",
        move |_| same_height && both_started && at_the_same_instant && both_ended_the_request,
    );
}

// ---------------------------------------------------------------------------------------------
// combat.attack-done.*
//
// Each scenario drives one real swing with the shipped keymap first, so every silence below is a
// silence after an attack that genuinely went out.
// ---------------------------------------------------------------------------------------------

/// A clean acknowledgement re-arms the automatic loop and puts nothing on the wire.
pub fn a_clean_acknowledgement_rearms_the_loop() {
    use dereth_client_model::combat::PowerBarMode;

    let mut b = a_melee_bench();
    b.one_real_attack();

    let sent = b.attack_done(0);

    let still_armed = b.repeating() && b.combat_state().current_build_is_automatic;
    let not_waiting = !b.combat_state().attack_server_response_pending;
    let no_cancel = cancels(&sent) == 0;
    let bar_still_up = b.combat_state().power_bar_mode != PowerBarMode::Undef;

    // The automatic rebuild fills the bar and stops: the client does not re-send on its own.
    let after = b.run(80);
    let the_rebuild_is_silent =
        melee_attacks(&after) == 0 && !b.combat_state().build_in_progress && b.repeating();

    b.shutdown();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.attack-done.a-clean-acknowledgement-rearms-the-loop-and-sends-nothing",
        move |_| still_armed && not_waiting && no_cancel && bar_still_up && the_rebuild_is_silent,
    );
}

/// Every other value disarms the repeat, cancels once, and empties the bar.
pub fn any_other_acknowledgement_cancels_the_repeat() {
    use dereth_client_model::combat::PowerBarMode;

    // Every value in its own station: one that left the previous station's state behind would
    // pass on the previous one's abort. The four are the shard's cancel, its busy refusal, the
    // out-of-band value and the all-ones one.
    let mut held = true;
    for result in [0x0036u32, 0x001D, 0x0400, 0xFFFF_FFFF] {
        let mut b = a_melee_bench();
        b.one_real_attack();
        // Both flags set first, so the clearing is shown to be the handler's and not the state
        // it was already in.
        b.set_attack_in_progress(true);

        let sent = b.attack_done(result);

        held &= !b.repeating()
            && !b.combat_state().attack_server_response_pending
            && !b.combat_state().build_in_progress
            && b.combat_state().power_bar_mode == PowerBarMode::Undef
            && cancels(&sent) == 1
            && !b.combat_state().attack_in_progress
            && !b.world_attack_in_progress();
        b.shutdown();
    }

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.attack-done.any-other-value-cancels-the-repeat-and-empties-the-bar",
        move |_| held,
    );
}

/// The slider moved while the swing was in flight, so the acknowledgement re-fires at the new
/// power -- and only then.
pub fn a_moved_slider_refires_at_its_new_power() {
    let unmoved = {
        let mut b = a_melee_bench();
        b.one_real_attack();
        let sent = b.attack_done(0);
        let quiet = melee_attacks(&sent) == 0;
        b.shutdown();
        quiet
    };
    let (cleared_the_tolerance, refired_once, copied_the_slider) = {
        let mut b = a_melee_bench();
        b.one_real_attack();
        let before = b.combat_state().requested_attack_power;
        let moved = b.set_ui_requested_power_from_scrollbar(0);
        let cleared = (before - moved).abs() > 0.01;

        let sent = b.attack_done(0);
        let once = melee_attacks(&sent) == 1;
        let copied = (b.combat_state().requested_attack_power - moved).abs() < f32::EPSILON;
        b.shutdown();
        (cleared, once, copied)
    };

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.attack-done.a-moved-slider-refires-the-swing-at-its-new-power",
        move |_| unmoved && cleared_the_tolerance && refired_once && copied_the_slider,
    );
}

/// The acknowledgement never writes a chat line, whatever it carries -- and it did arrive.
pub fn the_acknowledgement_writes_no_line() {
    let mut b = a_melee_bench();
    b.one_real_attack();
    let before = b.attacks_done_count();
    let mut n = 0u64;
    let mut silent = true;
    let mut reached_the_arm = true;
    for result in [0x0000u32, 0x0036, 0x001D, 0x0001, 0x0400, 0xFFFF_FFFF] {
        let lines = b.chat_len();
        let _ = b.attack_done(result);
        n += 1;
        silent &= b.chat_len() == lines;
        reached_the_arm &= b.attacks_done_count() == before + n;
    }
    b.shutdown();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.attack-done.the-acknowledgement-writes-no-line-whatever-it-carries",
        move |_| silent && reached_the_arm,
    );
}

// ---------------------------------------------------------------------------------------------
// combat.auto-attack.*
//
// The gesture is the shipped keymap's own control for the movement action, found in the shipped
// keymap and never written down here, fired through the client's own input manager.
// ---------------------------------------------------------------------------------------------

/// Walking backwards breaks a repeating melee attack -- once, and without eating the movement.
pub fn walking_backwards_breaks_a_repeating_swing() {
    use dereth_input::InputMapId;

    let mut b = a_melee_bench();
    b.one_real_attack();
    let premise = b.repeating() && !b.char_input().back;

    let back = b.control(
        InputMapId(4),
        dereth_client_runtime::actions::movement::action::MOVE_BACKWARD,
    );
    let aborts = b.forward_attack_aborts();
    let sent = b.press(back, true, 1);

    let the_body_still_walks = b.char_input().back;
    let the_repeat_cleared = !b.repeating();
    let one_edge_and_one_cancel = b.forward_attack_aborts() == (aborts.0 + 1, aborts.1 + 1);
    // Eight more frames, so a defect that merely *delays* the cancel does not read as this one.
    // Eight more frames, so a defect that merely *delays* the cancel does not read as this one:
    // the cancel is a short action the flow queue builds into a datagram at its own next pass and
    // not at the press.
    let later = b.run(8);
    let all: Vec<dereth_client_model::Request> = sent.into_iter().chain(later).collect();
    // **The cancel is read off the datagrams the client built**, because it never reaches the
    // frame's own outbox: the abort puts its message straight on the flow queue. The swing is
    // read off the outbox as everywhere else. The body is walking, so these frames also carry the
    // movement the walk itself reports -- which is the half that must *not* be eaten.
    let exactly_one_cancel_and_no_new_swing = b.wire_cancels() == 1 && melee_attacks(&all) == 0;

    // The shard re-sends the acknowledgement for the swing already in flight; with the repeat off
    // it must not re-arm.
    let after = b.attack_done(0);
    let nothing_rearmed = !b.repeating() && melee_attacks(&after) == 0 && b.wire_cancels() == 0;
    b.shutdown();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.auto-attack.walking-backwards-breaks-a-repeating-swing",
        move |_| {
            premise
                && the_body_still_walks
                && the_repeat_cleared
                && one_edge_and_one_cancel
                && exactly_one_cancel_and_no_new_swing
                && nothing_rearmed
        },
    );
}

/// A release, a turn and a walk with nothing in flight all move the body and cancel nothing.
pub fn a_release_a_turn_and_an_idle_walk_cancel_nothing() {
    use dereth_input::InputMapId;

    // 1. The release of a key that really was held: a release of a key never held is refused and
    // would prove nothing.
    let release = {
        let mut b = a_melee_bench();
        let back = b.control(
            InputMapId(4),
            dereth_client_runtime::actions::movement::action::MOVE_BACKWARD,
        );
        let _ = b.press(back, true, 1);
        let held = b.char_input().back;
        b.one_real_attack();
        let aborts = b.forward_attack_aborts();
        let mut sent = b.press(back, false, 1);
        let stopped = !b.char_input().back;
        let still_repeating = b.repeating();
        let no_edge = b.forward_attack_aborts() == aborts;
        sent.extend(b.run(8));
        let quiet = b.wire_cancels() == 0 && melee_attacks(&sent) == 0;
        b.shutdown();
        held && stopped && still_repeating && no_edge && quiet
    };

    // 2. The turn: it is on the turn list, so the substate test fails and nothing cancels.
    let turn = {
        let mut b = a_melee_bench();
        b.one_real_attack();
        let aborts = b.forward_attack_aborts();
        let left = b.control(
            InputMapId(4),
            dereth_client_runtime::actions::movement::action::TURN_LEFT,
        );
        let mut sent = b.press(left, true, 1);
        let turned = b.char_input().turn_left;
        let still_repeating = b.repeating();
        let no_edge = b.forward_attack_aborts() == aborts;
        sent.extend(b.run(8));
        let quiet = b.wire_cancels() == 0 && melee_attacks(&sent) == 0;
        b.shutdown();
        turned && still_repeating && no_edge && quiet
    };

    // 3. The same press with nothing in flight: the edge is raised and the state gate refuses it,
    // which is what stops the client spraying cancels at the shard while a player walks.
    let idle = {
        let mut b = a_melee_bench();
        let nothing_in_flight = !b.repeating();
        let back = b.control(
            InputMapId(4),
            dereth_client_runtime::actions::movement::action::MOVE_BACKWARD,
        );
        let aborts = b.forward_attack_aborts();
        let mut sent = b.press(back, true, 1);
        let walks = b.char_input().back;
        let edge_raised_and_refused = b.forward_attack_aborts() == (aborts.0 + 1, aborts.1);
        sent.extend(b.run(8));
        let quiet = b.wire_cancels() == 0 && melee_attacks(&sent) == 0;
        b.shutdown();
        nothing_in_flight && walks && edge_raised_and_refused && quiet
    };

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.auto-attack.a-release-a-turn-and-an-idle-walk-cancel-nothing",
        move |_| release && turn && idle,
    );
}

// ---------------------------------------------------------------------------------------------
// The `#[test]` beside each scenario.
// ---------------------------------------------------------------------------------------------

#[test]
fn scenario_ready_is_the_bodys_own_motion_queue() {
    scenario("ready_is_the_bodys_own_motion_queue");
}

#[test]
fn scenario_a_stance_change_the_body_refuses_is_queued() {
    scenario("a_stance_change_the_body_refuses_is_queued");
}

#[test]
fn scenario_the_two_flavours_disagree_on_one_body() {
    scenario("the_two_flavours_disagree_on_one_body");
}

#[test]
fn scenario_a_melee_swing_needs_the_weapons_combat_table() {
    scenario("a_melee_swing_needs_the_weapons_combat_table");
}

#[test]
fn scenario_a_missile_swing_needs_the_stance_and_the_ready_command() {
    scenario("a_missile_swing_needs_the_stance_and_the_ready_command");
}

#[test]
fn scenario_all_three_attack_arms_consume_the_produced_flavour() {
    scenario("all_three_attack_arms_consume_the_produced_flavour");
}

#[test]
fn scenario_one_click_charges_the_bar_and_swings_when_it_fills() {
    scenario("one_click_charges_the_bar_and_swings_when_it_fills");
}

#[test]
fn scenario_a_held_control_charges_to_full_and_swings_on_release() {
    scenario("a_held_control_charges_to_full_and_swings_on_release");
}

#[test]
fn scenario_the_button_and_the_key_start_the_same_charge() {
    scenario("the_button_and_the_key_start_the_same_charge");
}

#[test]
fn scenario_a_clean_acknowledgement_rearms_the_loop() {
    scenario("a_clean_acknowledgement_rearms_the_loop");
}

#[test]
fn scenario_any_other_acknowledgement_cancels_the_repeat() {
    scenario("any_other_acknowledgement_cancels_the_repeat");
}

#[test]
fn scenario_a_moved_slider_refires_at_its_new_power() {
    scenario("a_moved_slider_refires_at_its_new_power");
}

#[test]
fn scenario_the_acknowledgement_writes_no_line() {
    scenario("the_acknowledgement_writes_no_line");
}

#[test]
fn scenario_walking_backwards_breaks_a_repeating_swing() {
    scenario("walking_backwards_breaks_a_repeating_swing");
}

#[test]
fn scenario_a_release_a_turn_and_an_idle_walk_cancel_nothing() {
    scenario("a_release_a_turn_and_an_idle_walk_cancel_nothing");
}

mod support {
    use std::sync::Arc;

    use dereth_client::character::Character;
    use dereth_client::interaction::Interaction;
    use dereth_client::world::{load_region, DEFAULT_LANDBLOCK};
    use dereth_client_model::combat::{CombatMode, COMBAT_TABLE_DID};
    use dereth_client_model::Request;
    use dereth_dat::RetailDatStore;
    use dereth_input::spec::ControlChord;
    use dereth_input::{ActionId, InputMapId};
    use dereth_primitives::{DataId, LocalTime, ObjectId};
    use dereth_testkit::{ClientSpec, HeadlessClient};

    /// The combat table every character is born carrying.
    const A_COMBAT_TABLE: u32 = 0x3000_0021;
    /// The middle of the starting landblock, which is where every body below settles.
    const SPAWN: (f32, f32) = (96.0, 96.0);

    pub fn store() -> Arc<RetailDatStore> {
        Arc::new(dereth_dat::testing::open_store_or_fail())
    }

    /// A body standing still on the retail terrain, settled for two seconds of thirty-hertz
    /// frames. The settle is not a nicety: an unsettled body still has its entry motion
    /// outstanding, so the ready arm below would be measuring a body that was never ready.
    pub fn settled(store: &Arc<RetailDatStore>) -> (Character, u32) {
        let region = load_region(store).expect("the region decodes");
        let mut c =
            Character::new(store, &region, DEFAULT_LANDBLOCK, SPAWN).expect("the body is created");
        for i in 1..=60 {
            c.update(LocalTime(f64::from(i) / 30.0));
        }
        assert!(
            c.on_ground(),
            "the body must settle before anything is measured"
        );
        assert!(
            !c.driver().movement.motions_pending(),
            "and it must have finished its entry motion, or the ready arm measures the entry"
        );
        (c, 60)
    }

    /// A world with a player in `mode`, an attackable creature selected, and optionally the
    /// combat table the melee arm reads off the wielded weapon.
    ///
    /// The creature is not decoration: the attack request refuses without an attackable target,
    /// so a world without one would make every silence a refusal the scenario did not intend.
    pub fn world_in(mode: CombatMode, table: bool) -> dereth_client_model::World {
        let mut w = dereth_client_model::World::new();
        let player = ObjectId(0x5000_064D);
        w.player = Some(player);
        let mut pw = dereth_client_model::Weenie::new(player);
        pw.pwd.name = "Aldis".into();
        if table {
            pw.qualities
                .get_or_insert_with(dereth_client_model::Qualities::new)
                .set(
                    dereth_client_model::StatKey::new(
                        dereth_client_model::StatType::Did,
                        COMBAT_TABLE_DID,
                    ),
                    dereth_client_model::StatValue::Did(DataId(A_COMBAT_TABLE)),
                );
        }
        w.tables.weenies.insert(player, pw);
        w.tables.inventories.insert(
            player,
            dereth_client_model::objects::ObjectInventory::new(player),
        );
        let monster = ObjectId(0x8000_064D);
        let mut m = dereth_client_model::Weenie::new(monster);
        m.pwd.name = "Mosswart".into();
        m.pwd.obj_type = dereth_client_model::weenie::item_type::CREATURE;
        m.pwd.bitfield |= dereth_client_model::weenie::bitfield::ATTACKABLE;
        w.tables.weenies.insert(monster, m);
        w.set_selected_object(Some(monster), false, &mut dereth_client_model::NullSink);
        w.combat.combat_mode = mode;
        w
    }

    /// One combat action, as the input dispatch delivers it.
    pub fn key(action: u32, start: bool) -> dereth_client_runtime::actions::Action {
        dereth_client_runtime::actions::Action {
            id: ActionId(action),
            phase: if start {
                dereth_client_runtime::actions::ActionPhase::Begin
            } else {
                dereth_client_runtime::actions::ActionPhase::End
            },
            extent: 1.0,
            repeats: 1,
        }
    }

    /// How many of `requests` are the melee attack the player's swing sends.
    ///
    /// The requests are counted by variant rather than by opcode, because a variant is what the
    /// claim is about and an opcode would be one indirection further from it.
    pub fn melee_attacks(requests: &[Request]) -> usize {
        requests
            .iter()
            .filter(|r| matches!(r, Request::TargetedMeleeAttack(_)))
            .count()
    }

    /// How many of `requests` are the cancel that breaks an automatic attack.
    pub fn cancels(requests: &[Request]) -> usize {
        requests
            .iter()
            .filter(|r| matches!(r, Request::CancelAttack(_)))
            .count()
    }

    /// The client's per-frame outbox. The frame replaces it every pass, so reading it once per
    /// frame is "what did this frame send" rather than "what has ever been sent".
    pub fn drain(c: &HeadlessClient) -> Vec<Request> {
        c.view().expect_app().interaction().last_sent.to_vec()
    }

    /// The shipped keymap's own control for `action` in `map` -- discovered, never written down.
    pub fn shipped_control(
        c: &mut HeadlessClient,
        map: InputMapId,
        action: ActionId,
    ) -> ControlChord {
        let shell = c.app_mut().input_manager_mut().expect("the input shell");
        let section = shell
            .manager
            .keymap
            .section(map)
            .unwrap_or_else(|| panic!("the shipped keymap has a section for {map:?}"));
        // The unmodified defaults only: a binding carrying a meta mode would need its modifier
        // held across the press.
        let found: Vec<ControlChord> = section
            .bindings()
            .iter()
            .filter(|(q, a)| *a == action && q.meta_mode == 0)
            .map(|(q, _)| *q)
            .collect();
        assert!(
            !found.is_empty(),
            "the shipped keymap binds {action:?} in {map:?}"
        );
        found[0]
    }

    /// Fire one control edge through the client's own input manager.
    pub fn fire(c: &mut HeadlessClient, qc: ControlChord, down: bool, t: u32) {
        use dereth_input::fire::ControlType;
        let data = if down { 0x80 } else { 0 };
        c.app_mut()
            .input_manager_mut()
            .expect("the input shell")
            .manager
            .fire_input_event(qc.control, ControlType::Button, data, t);
    }

    // -----------------------------------------------------------------------------------------
    // The model bench: the client's own frame slot, driven at clocks the scenario chooses.
    // -----------------------------------------------------------------------------------------

    /// `App::frame`'s interaction slot over a synthetic melee world.
    ///
    /// A whole client cannot be driven at arbitrary clocks -- its clock is the fixed-step one --
    /// and the charge claims are about what the bar does between two chosen instants, so this
    /// drives the production frame slot directly.
    pub struct Bench {
        inter: Interaction,
        objects: dereth_client::objects::ObjectStream,
        store: Arc<RetailDatStore>,
        /// Accumulated, because the frame's outbox is replaced every pass: reading it directly
        /// would make "one swing, five frames ago" and "no swing at all" the same observation.
        sent: Vec<Request>,
    }

    impl Bench {
        pub fn new() -> Self {
            let mut objects = dereth_client::objects::ObjectStream::default();
            objects.world = world_in(CombatMode::Melee, true);
            Self {
                inter: Interaction::new(),
                objects,
                store: store(),
                sent: Vec::new(),
            }
        }

        /// One frame of the interaction slot, at `t`, carrying `actions`.
        pub fn frame(&mut self, actions: Vec<dereth_client_runtime::actions::Action>, t: f64) {
            let _ = dereth_client::interaction::use_time(
                &mut self.inter,
                &self.store,
                None,
                &mut self.objects,
                None,
                actions,
                false,
                (800, 600),
                LocalTime(t),
            );
            self.sent.extend(self.inter.last_sent.iter().cloned());
        }

        /// Queue the requests a panel raises, as the combat window's own arms do.
        pub fn queue(&mut self, requests: Vec<dereth_ui_screens::view::UiRequest>) {
            self.inter.queue(Vec::new(), requests);
        }

        pub fn combat(&self) -> &dereth_client_model::combat::CombatState {
            &self.objects.world.combat
        }

        /// The power level of every melee attack this bench would have sent, in order.
        pub fn attacks(&self) -> Vec<f32> {
            self.sent
                .iter()
                .filter_map(|r| match r {
                    Request::TargetedMeleeAttack(a) => Some(a.power_level),
                    _ => None,
                })
                .collect()
        }
    }

    // -----------------------------------------------------------------------------------------
    // The whole-client bench: the shipped keymap, the shipped panels, the client's own frames.
    // -----------------------------------------------------------------------------------------

    /// A whole headless client in the gameplay screen, with a player who carries the combat table
    /// the shard's own description brings, an attackable creature selected, melee mode live and
    /// the window's slider at the top -- so the shipped melee input map is registered and its
    /// keys resolve.
    pub fn a_melee_bench() -> AppBench {
        use dereth_protocol::archive::{PackedHash, Reader};
        use dereth_protocol::login::LoginPlayerDescription;
        use dereth_protocol::types::qualities::base_flags;
        use dereth_protocol::Message as _;

        const PLAYER: ObjectId = ObjectId(0x5000_064E);
        const MONSTER: ObjectId = ObjectId(0x8000_064E);

        let mut c = HeadlessClient::new(ClientSpec::gameplay(6));
        {
            let w = c.world_mut();
            w.player = Some(PLAYER);
            let mut pw = dereth_client_model::Weenie::new(PLAYER);
            pw.pwd.name = "Aldis".into();
            pw.qualities = Some(dereth_client_model::Qualities::new());
            w.tables.weenies.insert(PLAYER, pw);
            w.tables.inventories.insert(
                PLAYER,
                dereth_client_model::objects::ObjectInventory::new(PLAYER),
            );
            let mut m = dereth_client_model::Weenie::new(MONSTER);
            m.pwd.name = "Mosswart".into();
            m.pwd.obj_type = dereth_client_model::weenie::item_type::CREATURE;
            m.pwd.bitfield |= dereth_client_model::weenie::bitfield::ATTACKABLE;
            w.tables.weenies.insert(MONSTER, m);
            w.set_selected_object(Some(MONSTER), false, &mut dereth_client_model::NullSink);
            w.inventory_mask = dereth_client_model::inventory::slots::loc::MELEE_WEAPON
                | dereth_client_model::inventory::slots::loc::HELD;
            w.combat.combat_mode = CombatMode::Melee;
            // The window's slider at the top: the cap the bar must reach before the frame swings.
            w.combat.set_ui_requested_power_from_scrollbar(1000);
        }

        // The description a shard sends a melee character: the combat table the melee arm reads,
        // and the two option words that carry the automatic-repeat setting.
        let mut d = LoginPlayerDescription::default();
        d.qualities.base.weenie_type = 0x0A;
        d.qualities.base.flags |= base_flags::DID;
        let o = dereth_client_model::player::options::Options::default();
        d.player_module.options = o.options;
        d.player_module.options2 = o.options2;
        d.qualities.base.tables.dids = Some(PackedHash {
            table_size: 8,
            entries: vec![(COMBAT_TABLE_DID, A_COMBAT_TABLE)],
        });
        let bytes = dereth_protocol::write_body(&d).expect("the description encodes");
        let d = LoginPlayerDescription::read(&mut Reader::new(&bytes)).expect("and decodes");
        c.when(dereth_testkit::Inbound::event(
            dereth_client_net::client_session::SessionEvent::PlayerDescription(Box::new(d)),
        ));

        attach_endpoint(&mut c);
        let mut b = AppBench { c, t: 100_000 };
        // Two frames, so the combat input maps are registered before any combat key is pressed.
        let _ = b.run(2);
        assert_eq!(
            b.c.view().world().combat_table_did(),
            Some(DataId(A_COMBAT_TABLE)),
            "the description reached the game model, so the melee ready arm can answer"
        );
        assert!(
            b.c.view()
                .world()
                .player_system
                .options
                .auto_repeat_attack(),
            "the automatic repeat is on, which is what makes a clean acknowledgement re-arm"
        );
        b
    }

    /// The wire reader is `dereth_testkit::wire::Wire`, and `HeadlessClient::take_wire_count` is
    /// what these scenarios read it through; the endpoint under it is `dereth_testkit::replay`'s.
    pub fn attach_endpoint(c: &mut HeadlessClient) {
        c.attach_replay(dereth_testkit::replay::connected_endpoint(
            dereth_testkit::replay::PEER_ADDR,
        ));
    }

    /// A whole client, plus the input device's own millisecond clock.
    pub struct AppBench {
        c: HeadlessClient,
        /// Well clear of the double-click window, so one press is not read as a double click of
        /// itself.
        t: u32,
    }

    impl AppBench {
        /// Run `n` frames and answer the requests they sent.
        pub fn run(&mut self, n: u64) -> Vec<Request> {
            let mut out = Vec::new();
            for _ in 0..n {
                self.c.tick(1);
                out.extend(drain(&self.c));
            }
            out
        }

        pub fn control(&mut self, map: InputMapId, action: ActionId) -> ControlChord {
            shipped_control(&mut self.c, map, action)
        }

        /// One control edge and the frames that dispatch what it raised.
        pub fn press(&mut self, qc: ControlChord, down: bool, frames: u64) -> Vec<Request> {
            self.t += 2_000;
            fire(&mut self.c, qc, down, self.t);
            self.run(frames)
        }

        /// One click of the shipped medium-attack key and the frames the bar needs to fill.
        pub fn one_real_attack(&mut self) {
            let k = self.control(
                dereth_input::combat::MELEE_COMBAT_MAP,
                ActionId(dereth_client::interaction::action::COMBAT_MEDIUM_ATTACK),
            );
            let mut sent = self.press(k, true, 1);
            sent.extend(self.press(k, false, 1));
            sent.extend(self.run(60));
            assert!(
                melee_attacks(&sent) == 1,
                "the shipped attack key must put exactly one melee attack on the wire, got {sent:?}"
            );
            assert!(
                self.combat_state().attack_server_response_pending,
                "and the client is waiting for its acknowledgement"
            );
            assert!(self.repeating(), "the automatic repeat armed the loop");
        }

        /// The shard's acknowledgement, arriving the way it does in a session.
        pub fn attack_done(&mut self, error: u32) -> Vec<Request> {
            let m = dereth_protocol::combat::CombatHandleAttackDoneEvent { error };
            self.c.when(dereth_testkit::Inbound::message(&m));
            let mut out = drain(&self.c);
            out.extend(self.run(2));
            out
        }

        pub fn combat_state(&self) -> &dereth_client_model::combat::CombatState {
            &self.c.view().world().combat
        }

        pub fn repeating(&self) -> bool {
            self.c.view().world().combat.repeat_attacking
        }

        pub fn world_attack_in_progress(&self) -> bool {
            self.c.view().world().attack_in_progress
        }

        pub fn set_attack_in_progress(&mut self, v: bool) {
            self.c.world_mut().combat.attack_in_progress = v;
            self.c.world_mut().attack_in_progress = v;
        }

        pub fn set_ui_requested_power_from_scrollbar(&mut self, v: u32) -> f32 {
            self.c
                .world_mut()
                .combat
                .set_ui_requested_power_from_scrollbar(v)
        }

        pub fn attacks_done_count(&self) -> u64 {
            self.c.view().interaction().stats.attacks_done
        }

        pub fn chat_len(&self) -> usize {
            self.c.view().chat_lines().len()
        }

        pub fn char_input(&self) -> dereth_client::character::CharacterInput {
            self.c.view().expect_app().char_input()
        }

        pub fn forward_attack_aborts(&self) -> (u64, u64) {
            self.c.view().expect_app().new_forward_attack_aborts()
        }

        /// How many cancels this client has built into a datagram since the last look.
        pub fn wire_cancels(&mut self) -> usize {
            /// The ordered sub-type of the message that breaks an automatic attack.
            const CANCEL_ATTACK: u32 = 0x01B7;
            self.c.take_wire_count(CANCEL_ATTACK)
        }

        pub fn shutdown(self) {
            self.c.shutdown();
        }
    }
}

// ---------------------------------------------------------------------------------------------
// combat.damage-line.*
//
// The tables below are written out rather than left behind the evidence handle, because here the
// transcription **is** the behaviour: what the player reads is the adjective,
// the damage word and the body part, and a scenario that asserted one cell of each would be a
// scenario about one fight. The corpus's single combat recording carries eight attacks, all of
// one damage type at one damage number, which is why the tables are here.
// ---------------------------------------------------------------------------------------------

/// The channel the player's own hits are written on.
const COMBAT_SELF: u8 = 22;
/// And the channel the hits he takes are written on.
const COMBAT_ENEMY: u8 = 21;

/// The damage-type bits the shard sends.
mod dt {
    pub const SLASH: u32 = 0x0001;
    pub const PIERCE: u32 = 0x0002;
    pub const BLUDGEON: u32 = 0x0004;
    pub const COLD: u32 = 0x0008;
    pub const FIRE: u32 = 0x0010;
    pub const ACID: u32 = 0x0020;
    pub const ELECTRIC: u32 = 0x0040;
    pub const HEALTH: u32 = 0x0080;
    pub const STAMINA: u32 = 0x0100;
    pub const MANA: u32 = 0x0200;
    pub const NETHER: u32 = 0x0400;
    pub const BASE: u32 = 0x1000_0000;
}

/// The conditions a swing can carry.
mod cond {
    pub const CRITICAL_PROTECTION: u32 = 1;
    pub const RECKLESSNESS: u32 = 2;
    pub const SNEAK_ATTACK: u32 = 4;
}

/// `(damage type, the four singular verbs, the four plural ones)`, weakest bucket first.
const ADJECTIVES: [(u32, [&str; 4], [&str; 4]); 9] = [
    (
        dt::SLASH,
        ["scratch", "cut", "slash", "mangle"],
        ["scratches", "cuts", "slashes", "mangles"],
    ),
    (
        dt::PIERCE,
        ["nick", "stab", "impale", "gore"],
        ["nicks", "stabs", "impales", "gores"],
    ),
    (
        dt::BLUDGEON,
        ["graze", "bash", "smash", "crush"],
        ["grazes", "bashes", "smashes", "crushes"],
    ),
    (
        dt::COLD,
        ["numb", "chill", "frost", "freeze"],
        ["numbs", "chills", "frosts", "freezes"],
    ),
    (
        dt::FIRE,
        ["singe", "scorch", "burn", "incinerate"],
        ["singes", "scorches", "burns", "incinerates"],
    ),
    (
        dt::ACID,
        ["blister", "sear", "corrode", "dissolve"],
        ["blisters", "sears", "corrodes", "dissolves"],
    ),
    (
        dt::ELECTRIC,
        ["spark", "shock", "jolt", "blast"],
        ["sparks", "shocks", "jolts", "blasts"],
    ),
    (
        dt::HEALTH,
        ["drain", "exhaust", "siphon", "deplete"],
        ["drains", "exhausts", "siphons", "depletes"],
    ),
    (
        dt::NETHER,
        ["scar", "twist", "wither", "eradicate"],
        ["scars", "twists", "withers", "eradicates"],
    ),
];

/// Which of the four verbs a share of the target's health picks. **The comparison is "at most"**,
/// so a share of exactly a tenth, a quarter or a half falls in the *lower* bucket.
fn bucket(percent: f64) -> usize {
    assert!(percent >= 0.0, "a negative share never reaches the table");
    if percent <= 0.10 {
        0
    } else if percent <= 0.25 {
        1
    } else if percent <= 0.50 {
        2
    } else {
        3
    }
}

/// Twelve shares: every threshold from both sides, and both ends.
const PERCENTS: [f64; 12] = [
    0.0, 0.0999, 0.1, 0.1001, 0.2499, 0.25, 0.2501, 0.4999, 0.5, 0.5001, 0.75, 1.0,
];

/// The nine damage types that have a name, in the order the client tests them.
const NAMES: [(u32, &str); 9] = [
    (dt::SLASH, "slashing"),
    (dt::PIERCE, "piercing"),
    (dt::BLUDGEON, "bludgeoning"),
    (dt::COLD, "cold"),
    (dt::FIRE, "fire"),
    (dt::ACID, "acid"),
    (dt::ELECTRIC, "electrical"),
    (dt::NETHER, "nether"),
    (dt::BASE, "prismatic"),
];

/// The word before `damage!`: the joined list and a space, or nothing at all when the list is
/// empty or does not fit the sixty-four bytes the client gives it.
fn damage_word(mask: u32) -> String {
    let joined = NAMES
        .iter()
        .filter(|(m, _)| mask & m != 0)
        .map(|(_, n)| *n)
        .collect::<Vec<_>>()
        .join("/");
    if joined.is_empty() || joined.len() + 1 > 0x40 {
        String::new()
    } else {
        format!("{joined} ")
    }
}

/// The twenty-nine body parts, in the order the client indexes them.
const PARTS: [&str; 29] = [
    "undefined",
    "head",
    "chest",
    "abdomen",
    "upper arm",
    "lower arm",
    "hand",
    "upper leg",
    "lower leg",
    "foot",
    "horn",
    "front leg",
    "unknown",
    "front foot",
    "rear leg",
    "unknown",
    "rear foot",
    "torso",
    "tail",
    "arm",
    "leg",
    "claw",
    "wings",
    "breath",
    "tentacle",
    "upper tentacle",
    "lower tentacle",
    "cloak",
    "num",
];

fn part_name(part: u32) -> &'static str {
    let idx = part.wrapping_add(1);
    if idx <= 0x1c {
        PARTS[idx as usize]
    } else {
        "unknown"
    }
}

/// One notification, encoded through the production writer and delivered on the queue a panel
/// consumes -- so the blob the client reads is the blob the codec makes.
fn attacker(
    defender_name: &str,
    damage_type: u32,
    percent: f64,
    damage: u32,
    critical: u32,
    attack_conditions: u32,
) -> dereth_testkit::Inbound {
    dereth_testkit::Inbound::message(&dereth_protocol::combat::AttackerNotification {
        defender_name: defender_name.to_owned(),
        damage_type,
        percent,
        damage,
        critical,
        attack_conditions,
        attack_conditions_high: 0,
    })
}

#[allow(clippy::too_many_arguments)]
fn defender(
    attacker_name: &str,
    damage_type: u32,
    percent: f64,
    damage: u32,
    damage_location: u32,
    critical: u32,
    attack_conditions: u32,
) -> dereth_testkit::Inbound {
    dereth_testkit::Inbound::message(&dereth_protocol::combat::DefenderNotification {
        attacker_name: attacker_name.to_owned(),
        damage_type,
        percent,
        damage,
        damage_location,
        critical,
        attack_conditions,
        attack_conditions_high: 0,
    })
}

/// Deliver one notification and answer the single line the client composed from it.
fn line(c: &mut HeadlessClient, m: dereth_testkit::Inbound) -> (u8, String) {
    let before = c.view().chat_lines().len();
    c.when(m);
    let composed: Vec<(u8, String)> = c.view().chat_lines()[before..]
        .iter()
        .map(|l| (l.ty, l.body.clone()))
        .collect();
    assert_eq!(
        composed.len(),
        1,
        "one notification composes exactly one line, got {composed:?}"
    );
    composed[0].clone()
}

/// A whole client with the shipped gameplay screen up, which is where a damage line is drawn.
fn a_client_in_the_world() -> HeadlessClient {
    HeadlessClient::new(ClientSpec::gameplay(4))
}

/// The two sentences, the plural rule, the damage word and the body part.
pub fn a_damage_line_says_what_was_hit_how_hard_and_with_what() {
    let mut c = a_client_in_the_world();

    // The two sentences, whole. The attacker's comes from one format; the defender's is assembled
    // from fragments, and the two have different word order and a different channel.
    let attacker_sentence = line(&mut c, attacker("Drudge Slave", dt::SLASH, 0.30, 12, 0, 0))
        == (
            COMBAT_SELF,
            "You slash Drudge Slave for 12 points of slashing damage!\n".to_owned(),
        );
    let defender_sentence = line(
        &mut c,
        defender("Drudge Slave", dt::BLUDGEON, 0.125, 7, 3, 0, 0),
    ) == (
        COMBAT_ENEMY,
        "Drudge Slave bashes your upper arm for 7 points of bludgeoning damage!\n".to_owned(),
    );

    // Exactly one point drops the plural; zero takes it.
    let mut plural = true;
    for (damage, tail) in [(0u32, "0 points"), (1, "1 point"), (2, "2 points")] {
        plural &= line(&mut c, attacker("Rat", dt::PIERCE, 0.05, damage, 0, 0)).1
            == format!("You nick Rat for {tail} of piercing damage!\n");
        plural &= line(&mut c, defender("Rat", dt::PIERCE, 0.05, damage, 1, 0, 0)).1
            == format!("Rat nicks your chest for {tail} of piercing damage!\n");
    }

    // Every damage type that has a name, and the three that do not: a health, stamina or mana
    // drain names no damage at all, exactly as an unspecified type does.
    let mut words = true;
    for (ty, word) in [
        (0u32, ""),
        (dt::SLASH, "slashing "),
        (dt::PIERCE, "piercing "),
        (dt::BLUDGEON, "bludgeoning "),
        (dt::COLD, "cold "),
        (dt::FIRE, "fire "),
        (dt::ACID, "acid "),
        (dt::ELECTRIC, "electrical "),
        (dt::HEALTH, ""),
        (dt::STAMINA, ""),
        (dt::MANA, ""),
        (dt::NETHER, "nether "),
        (dt::BASE, "prismatic "),
    ] {
        words &= damage_word(ty) == word;
        words &= line(&mut c, attacker("Shreth", ty, 0.6, 40, 0, 0))
            .1
            .ends_with(&format!("for 40 points of {word}damage!\n"));
    }

    // Several types at once are joined, in the client's own order, and a list too long for the
    // room the client gives it leaves the sentence with no damage word at all.
    let eight = dt::SLASH
        | dt::PIERCE
        | dt::BLUDGEON
        | dt::COLD
        | dt::FIRE
        | dt::ACID
        | dt::ELECTRIC
        | dt::NETHER;
    let joined = line(
        &mut c,
        attacker("Virindi", dt::SLASH | dt::PIERCE, 0.2, 9, 0, 0),
    )
    .1 == "You hit Virindi for 9 points of slashing/piercing damage!\n"
        && line(&mut c, attacker("Virindi", eight, 0.2, 9, 0, 0)).1
            == "You hit Virindi for 9 points of \
                slashing/piercing/bludgeoning/cold/fire/acid/electrical/nether damage!\n"
        && line(&mut c, attacker("Virindi", eight | dt::BASE, 0.2, 9, 0, 0)).1
            == "You hit Virindi for 9 points of damage!\n";

    // Every body part row, both ends of the range check, and the two rows the client itself
    // leaves unnamed.
    let mut parts = 0usize;
    let mut body_parts = true;
    for part in (0u32..=0x1d).chain([0xFF, 0x8000_0000, 0xFFFF_FFFF]) {
        let name = part_name(part);
        let (ty, body) = line(
            &mut c,
            defender("Banderling", dt::COLD, 0.4, 18, part, 0, 0),
        );
        body_parts &= ty == COMBAT_ENEMY
            && body == format!("Banderling frosts your {name} for 18 points of cold damage!\n");
        parts += 1;
    }
    body_parts &= parts == 33
        && part_name(11) == "unknown"
        && part_name(14) == "unknown"
        && part_name(0xFFFF_FFFF) == "undefined"
        && part_name(0x1b) == "num"
        && part_name(0x1c) == "unknown";

    c.assert_behaviour(
        "combat.damage-line.says-what-was-hit-how-hard-and-with-what",
        move |_| attacker_sentence && defender_sentence && plural && words && joined && body_parts,
    );
    c.shutdown();
}

/// The four verbs, both sides of every threshold, both sentences.
pub fn the_verb_steps_at_the_thresholds() {
    let mut c = a_client_in_the_world();

    let mut checked = 0usize;
    let mut held = true;
    for (ty, singular, plural) in ADJECTIVES {
        let word = damage_word(ty);
        for percent in PERCENTS {
            let b = bucket(percent);
            let (t, body) = line(&mut c, attacker("Olthoi Worker", ty, percent, 23, 0, 0));
            held &= t == COMBAT_SELF
                && body
                    == format!(
                        "You {} Olthoi Worker for 23 points of {word}damage!\n",
                        singular[b]
                    );
            let (t, body) = line(&mut c, defender("Olthoi Worker", ty, percent, 23, 0, 0, 0));
            held &= t == COMBAT_ENEMY
                && body
                    == format!(
                        "Olthoi Worker {} your head for 23 points of {word}damage!\n",
                        plural[b]
                    );
            checked += 2;
        }
    }
    let all_of_them = checked == 216;

    // A share below zero never reaches the table at all: the sentence keeps the plain verb.
    let mut negatives = true;
    for percent in [-0.0001, -0.5, -1.0] {
        negatives &= line(&mut c, attacker("Drudge", dt::FIRE, percent, 3, 0, 0)).1
            == "You hit Drudge for 3 points of fire damage!\n";
        negatives &= line(&mut c, defender("Drudge", dt::FIRE, percent, 3, 1, 0, 0)).1
            == "Drudge hits your chest for 3 points of fire damage!\n";
    }

    // And a type with no verb row of its own -- including two types combined -- says the same.
    let mut unrecognised = true;
    for ty in [
        0,
        dt::SLASH | dt::PIERCE,
        dt::SLASH | dt::BLUDGEON,
        dt::COLD | dt::FIRE,
        dt::STAMINA,
        dt::MANA,
        dt::BASE,
        0xFFFF_FFFF,
    ] {
        let word = damage_word(ty);
        unrecognised &= line(&mut c, attacker("Tusker", ty, 0.9, 5, 0, 0)).1
            == format!("You hit Tusker for 5 points of {word}damage!\n");
    }

    c.assert_behaviour(
        "combat.damage-line.the-verb-steps-at-the-thresholds-and-an-unranked-hit-just-hits",
        move |_| held && all_of_them && negatives && unrecognised,
    );
    c.shutdown();
}

/// A critical, a sneak attack and a reckless swing each announce themselves, in their own words
/// for each side and in one order.
pub fn a_critical_a_sneak_and_a_reckless_swing_each_say_so() {
    let mut c = a_client_in_the_world();

    let critical = line(&mut c, attacker("Rat", dt::SLASH, 0.05, 1, 1, 0)).1
        == "Critical hit!  You scratch Rat for 1 point of slashing damage!\n"
        && line(&mut c, defender("Rat", dt::SLASH, 0.05, 1, 5, 1, 0)).1
            == "Critical hit! Rat scratches your hand for 1 point of slashing damage!\n";

    let sneak = line(
        &mut c,
        attacker("Rat", dt::SLASH, 0.05, 1, 0, cond::SNEAK_ATTACK),
    )
    .1 == "Sneak Attack! You scratch Rat for 1 point of slashing damage!\n"
        && line(
            &mut c,
            defender("Rat", dt::SLASH, 0.05, 1, 5, 0, cond::SNEAK_ATTACK),
        )
        .1 == "Sneak Attack! Rat scratches your hand for 1 point of slashing damage!\n";

    // The two sides word this one differently, which is why they are asserted separately.
    let reckless = line(
        &mut c,
        attacker("Rat", dt::SLASH, 0.05, 1, 0, cond::RECKLESSNESS),
    )
    .1 == "Recklessness! You scratch Rat for 1 point of slashing damage!\n"
        && line(
            &mut c,
            defender("Rat", dt::SLASH, 0.05, 1, 5, 0, cond::RECKLESSNESS),
        )
        .1 == "Reckless! Rat scratches your hand for 1 point of slashing damage!\n";

    let both = cond::SNEAK_ATTACK | cond::RECKLESSNESS;
    let all_three_in_order = line(&mut c, attacker("Rat", dt::SLASH, 0.05, 1, 1, both)).1
        == "Critical hit!  Sneak Attack! Recklessness! You scratch Rat for 1 point of slashing \
            damage!\n"
        && line(&mut c, defender("Rat", dt::SLASH, 0.05, 1, 5, 1, both)).1
            == "Critical hit! Sneak Attack! Reckless! Rat scratches your hand for 1 point of \
                slashing damage!\n";

    // Critical protection is the one that appends a whole sentence after the damage rather than
    // before the verb.
    let protection = line(
        &mut c,
        attacker("Drudge", dt::FIRE, 0.7, 60, 1, cond::CRITICAL_PROTECTION),
    )
    .1
        == "Critical hit!  You incinerate Drudge for 60 points of fire damage! Your target's \
                Critical Protection augmentation allows them to avoid your critical hit!\n"
        && line(
            &mut c,
            defender("Drudge", dt::FIRE, 0.7, 60, 0, 1, cond::CRITICAL_PROTECTION),
        )
        .1 == "Critical hit! Drudge incinerates your head for 60 points of fire damage! Your \
                    Critical Protection augmentation allows you to avoid a critical hit!\n";

    c.assert_behaviour(
        "combat.damage-line.a-critical-a-sneak-and-a-reckless-swing-each-say-so",
        move |_| critical && sneak && reckless && all_three_in_order && protection,
    );
    c.shutdown();
}

/// Both lines reach the window the player reads, and squelching the combat channel stops them
/// being composed at all.
pub fn both_lines_reach_the_window_and_a_squelch_stops_them() {
    use dereth_ui::ElementId;
    use dereth_ui_screens::screens::gameplay::{window, GamePlayScreen};

    let mut c = a_client_in_the_world();
    c.when(attacker("Drudge Slave", dt::SLASH, 0.30, 12, 0, 0));
    c.when(defender("Drudge Slave", dt::BLUDGEON, 0.125, 7, 3, 0, 0));
    let composed = c.view().chat_lines().len() == 2;
    c.tick(1);

    let (in_the_log, trimmed, drawn) = {
        let shell = c.app_mut().ui_mut().expect("the UI shell");
        let ui = &mut shell.ui;
        let screen = shell.flow.current_mut().expect("a live screen");
        let any: &mut dyn std::any::Any = &mut **screen;
        let screen = any
            .downcast_mut::<GamePlayScreen>()
            .expect("the gameplay screen");

        let main = screen
            .chat
            .iter()
            .find(|w| w.window_id == 8)
            .expect("the main chat window");
        let log = main.log_text();
        let in_the_log = [
            "You slash Drudge Slave for 12 points of slashing damage!",
            "Drudge Slave bashes your upper arm for 7 points of bludgeoning damage!",
        ]
        .iter()
        .all(|want| log.contains(want));
        // The window trims the newline the handler appends; the bare separator rows either side
        // of a line are a different thing.
        let trimmed = main
            .log
            .iter()
            .filter(|(_, t)| t.contains("damage!"))
            .all(|(_, t)| t.ends_with('!'));

        // And the element the player actually reads carries it.
        let root = ui
            .get_element(ElementId(0x1000_0495))
            .expect("the gameplay root");
        let chat_window = ui
            .get_child_recursive(root, window::MAIN_CHAT)
            .expect("the main chat window element");
        let log_element = ui
            .get_child_recursive(chat_window, ElementId(0x1000_0011))
            .expect("the chat scrollback");
        let text: String = ui
            .node(log_element)
            .expect("the scrollback node")
            .behaviour
            .as_ref()
            .map(|b| b.compose_text(ui.screen_box(log_element)))
            .unwrap_or_default()
            .iter()
            .map(|g| char::from_u32(u32::from(g.ch)).unwrap_or('?'))
            .collect();
        // The glyph list drops the whitespace between runs, so compare on what is printed.
        let strip = |t: &str| {
            t.chars()
                .filter(|ch| !ch.is_whitespace())
                .collect::<String>()
        };
        let drawn = strip(&text).contains(&strip(
            "You slash Drudge Slave for 12 points of slashing damage!",
        ));
        (in_the_log, trimmed, drawn)
    };
    c.shutdown();

    // The squelch, in its own client: the gate is the first thing both handlers reach, so a
    // squelched combat channel composes nothing at all. It is the negative control for every
    // sentence above.
    let mut c = a_client_in_the_world();
    c.world_mut()
        .chat
        .squelch
        .global
        .types
        .insert(dereth_client_model::chat::text_type::COMBAT);
    c.when(attacker("Drudge", dt::SLASH, 0.3, 12, 0, 0));
    c.when(defender("Drudge", dt::SLASH, 0.3, 12, 1, 0, 0));
    let squelched = c.view().chat_lines().is_empty()
        && c.view().hud().stats.combat_lines_squelched == 2
        && c.view().hud().stats.combat_lines == 0;

    c.assert_behaviour(
        "combat.damage-line.both-lines-reach-the-window-and-a-squelch-stops-them",
        move |_| composed && in_the_log && trimmed && drawn && squelched,
    );
    c.shutdown();
}

#[test]
fn scenario_a_damage_line_says_what_was_hit_how_hard_and_with_what() {
    scenario("a_damage_line_says_what_was_hit_how_hard_and_with_what");
}

#[test]
fn scenario_the_verb_steps_at_the_thresholds() {
    scenario("the_verb_steps_at_the_thresholds");
}

#[test]
fn scenario_a_critical_a_sneak_and_a_reckless_swing_each_say_so() {
    scenario("a_critical_a_sneak_and_a_reckless_swing_each_say_so");
}

#[test]
fn scenario_both_lines_reach_the_window_and_a_squelch_stops_them() {
    scenario("both_lines_reach_the_window_and_a_squelch_stops_them");
}

// ---------------------------------------------------------------------------------------------
// combat.input-map.*
//
// **Nothing here writes a key name or a map's contents down.** The contested controls are
// discovered from the shipped melee section, the quick bar's are discovered from the shipped
// quick-bar section, and the calibration control is the one the shipped combat section binds --
// so the population is the shipped data and the claim is what the client does with it.
//
// **The shipped conflict table is not read here.** That the three mode maps are declared not to
// clash with one another, and that the quick bar and the window commands are, is the shipped data's
// own statement and not a behaviour of this client; it stays behind the evidence handle, and what
// it justified -- which map really takes each contested key -- is driven below through the client's
// own input shell.
//
// **One narrowing, stated rather than hidden.** The peace-mode registration band is not pinned as
// a literal list of map ids: that would be a transcription of the client's own start-up table.
// What is asserted instead is every claim such a list would carry: the combat map is up throughout,
// the mode's map is in front of it, the quick bar is in front of the window commands, and the
// band comes back bit for bit when the player leaves combat.
// ---------------------------------------------------------------------------------------------

pub fn exactly_one_combat_map_is_registered_and_it_follows_the_mode() {
    use dereth_input::combat::mode;

    let mut shell = maps::shell();
    let at_rest = (
        shell.combat_input_mode() == mode::NONCOMBAT,
        maps::live_mode_maps(&shell).is_empty(),
        maps::none_at_start_up(),
        maps::band(&shell).contains(&maps::COMBAT_MAP),
        !shell.set_combat_input_maps(mode::NONCOMBAT),
    );

    let mut stations = Vec::new();
    for (m, want) in [
        (mode::MELEE, maps::MELEE_MAP),
        (mode::MISSILE, maps::MISSILE_MAP),
        (mode::MAGIC, maps::MAGIC_MAP),
        (mode::MELEE, maps::MELEE_MAP),
    ] {
        let changed = shell.set_combat_input_maps(m);
        stations.push((
            changed,
            maps::live_mode_maps(&shell) == vec![want],
            maps::band(&shell).first().copied() == Some(want),
            !shell.set_combat_input_maps(m),
        ));
    }

    let back = (
        shell.set_combat_input_maps(mode::NONCOMBAT),
        maps::live_mode_maps(&shell).is_empty(),
        maps::band(&shell).contains(&maps::COMBAT_MAP),
    );

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.input-map.exactly-one-is-registered-and-it-follows-the-mode",
        move |_| {
            at_rest == (true, true, true, true, true)
                && stations.iter().all(|s| *s == (true, true, true, true))
                && back == (true, true, true)
        },
    );
}

pub fn the_mode_decides_which_map_takes_the_contested_keys() {
    use dereth_input::combat::mode;

    let mut shell = maps::shell();
    let mut d = maps::Driver::new();
    let contested = maps::contested_controls(&shell);
    // The key that belongs to combat itself: it must answer in every mode, or a silence below
    // would only mean the driver is dead.
    let calibration = maps::the_combat_maps_own_control(&shell);

    // Peace: every contested key reaches nothing, and the calibration key still answers.
    let peace = (
        contested
            .iter()
            .filter(|q| d.resolve(&mut shell, q).is_none())
            .count(),
        d.resolve(&mut shell, &calibration).is_some(),
    );

    // The nine readings: three modes over three maps, five controls each.
    let mut readings = Vec::new();
    for (m, want) in [
        (mode::MELEE, maps::MELEE_MAP),
        (mode::MISSILE, maps::MISSILE_MAP),
        (mode::MAGIC, maps::MAGIC_MAP),
    ] {
        shell.set_combat_input_maps(m);
        let mut per_map = [0usize; 3];
        for q in &contested {
            let (gm, _) = d
                .resolve(&mut shell, q)
                .unwrap_or_else(|| panic!("a contested key reached nothing in mode {m}"));
            if let Some(i) = maps::MODE_MAPS.iter().position(|x| *x == gm.0) {
                per_map[i] += 1;
            }
        }
        let want_row: [usize; 3] =
            std::array::from_fn(|i| usize::from(maps::MODE_MAPS[i] == want) * contested.len());
        readings.push((per_map, want_row));
        // And the calibration key still answers with a mode map up.
        assert!(
            d.resolve(&mut shell, &calibration).is_some(),
            "the combat key answers in {m}"
        );
    }
    let five = contested.len();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.input-map.the-mode-decides-which-map-takes-the-contested-keys",
        move |_| {
            five > 0
                && peace == (five, true)
                && readings.len() == 3
                && readings.iter().all(|(got, want)| got == want)
        },
    );
}

pub fn every_shipped_quickslot_key_reaches_the_toolbar() {
    use dereth_input::combat::mode;
    use dereth_ui_screens::toolbar::shortcuts::dispatch;

    let mut shell = maps::shell();
    let mut d = maps::Driver::new();
    let bindings = maps::shipped_bindings(&shell, maps::QUICKSLOT_MAP);
    assert!(
        bindings.len() > 20,
        "the shipped quick-bar section is a real one"
    );

    // The registration order the keys they share with the window commands turn on.
    let band = maps::band(&shell);
    let ordered = match (
        band.iter().position(|m| *m == maps::QUICKSLOT_MAP),
        band.iter().position(|m| *m == maps::UI_COMMANDS_MAP),
    ) {
        (Some(q), Some(u)) => q < u,
        _ => false,
    };

    // Peace and the two modes that bind no number key: every shipped quick-bar key reaches the
    // quick bar, resolves to the action it names, and the toolbar accepts it.
    let mut quiet_modes = Vec::new();
    for m in [mode::NONCOMBAT, mode::MELEE, mode::MISSILE] {
        shell.set_combat_input_maps(m);
        let mut reached = 0;
        for (want, q) in &bindings {
            if let Some((gm, ga)) = d.resolve(&mut shell, q) {
                if gm == maps::quick() {
                    assert_eq!(ga, *want, "the quick bar answered a different action");
                    assert!(
                        dispatch(ga.0, false).is_some(),
                        "the toolbar takes every action its own section names"
                    );
                    reached += 1;
                }
            }
        }
        quiet_modes.push(reached);
    }

    // Magic: the plain number keys go to the spell bar instead. Discovered, not counted here.
    shell.set_combat_input_maps(mode::MAGIC);
    let mut shadowed = 0usize;
    let mut reached_in_magic = 0usize;
    let mut every_shadow_is_a_spell = true;
    let mut every_modified_key_survives = true;
    for (want, q) in &bindings {
        match d.resolve(&mut shell, q) {
            Some((gm, ga)) if gm == maps::quick() => {
                assert_eq!(ga, *want);
                reached_in_magic += 1;
            }
            Some((gm, _)) => {
                shadowed += 1;
                every_shadow_is_a_spell &= gm.0 == maps::MAGIC_MAP;
                every_modified_key_survives &= q.meta_mode == 0;
            }
            None => every_shadow_is_a_spell = false,
        }
    }
    let total = bindings.len();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.input-map.every-shipped-quickslot-key-reaches-the-toolbar-except-the-nine-magic-takes",
        move |_| {
            ordered
                && quiet_modes == vec![total, total, total]
                && shadowed > 0
                && shadowed < total
                && reached_in_magic == total - shadowed
                && every_shadow_is_a_spell
                && every_modified_key_survives
        },
    );
}

pub fn the_combat_map_itself_is_always_up_and_never_moves() {
    use dereth_input::combat::mode;

    let mut shell = maps::shell();
    let before = maps::band(&shell);
    let mut once_each_time = true;
    for m in [mode::MELEE, mode::MISSILE, mode::MAGIC, mode::NONCOMBAT] {
        shell.set_combat_input_maps(m);
        once_each_time &= maps::band(&shell)
            .iter()
            .filter(|x| **x == maps::COMBAT_MAP)
            .count()
            == 1;
    }
    let unchanged = maps::band(&shell) == before;

    // And the registration itself, driven directly rather than through the mode latch, with the
    // two arguments the client's own mode change passes it.
    let mut s = shell;
    s.register_combat_input_maps(mode::MAGIC, mode::NONCOMBAT);
    let magic_first = maps::band(&s).first().copied() == Some(maps::MAGIC_MAP);
    s.register_combat_input_maps(mode::MELEE, mode::MAGIC);
    let swapped = {
        let b = maps::band(&s);
        !b.contains(&maps::MAGIC_MAP) && b.first().copied() == Some(maps::MELEE_MAP)
    };
    s.register_combat_input_maps(mode::UNDEF, mode::MELEE);
    let none_at_all = maps::live_mode_maps(&s).is_empty();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.input-map.the-combat-map-itself-is-always-up-and-never-moves",
        move |_| once_each_time && unchanged && magic_first && swapped && none_at_all,
    );
}

pub fn a_running_client_swaps_the_keys_the_frame_after_the_mode_changes() {
    use dereth_client_model::combat::CombatMode;
    use dereth_input::combat::mode;

    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));

    // A client in the world is at peace and carries none of the three.
    let at_rest = maps::live_in(&mut c) == (mode::NONCOMBAT, Vec::new());

    // The shard says the mode changed -- the form that sends nothing back.
    maps::the_shard_sets_the_mode(&mut c, CombatMode::Magic);
    let same_frame = maps::live_in(&mut c) == (mode::NONCOMBAT, Vec::new());

    c.tick(1);
    let next_frame = maps::live_in(&mut c) == (mode::MAGIC, vec![maps::MAGIC_MAP]);

    // And back. A swap that only ever added would pass everything above.
    maps::the_shard_sets_the_mode(&mut c, CombatMode::NonCombat);
    c.tick(1);
    let back = maps::live_in(&mut c) == (mode::NONCOMBAT, Vec::new());

    c.assert_behaviour(
        "combat.input-map.a-running-client-swaps-the-keys-the-frame-after-the-mode-changes",
        move |_| at_rest && same_frame && next_frame && back,
    );
    c.shutdown();
}

#[test]
fn scenario_exactly_one_combat_map_is_registered_and_it_follows_the_mode() {
    scenario("exactly_one_combat_map_is_registered_and_it_follows_the_mode");
}

#[test]
fn scenario_the_mode_decides_which_map_takes_the_contested_keys() {
    scenario("the_mode_decides_which_map_takes_the_contested_keys");
}

#[test]
fn scenario_every_shipped_quickslot_key_reaches_the_toolbar() {
    scenario("every_shipped_quickslot_key_reaches_the_toolbar");
}

#[test]
fn scenario_the_combat_map_itself_is_always_up_and_never_moves() {
    scenario("the_combat_map_itself_is_always_up_and_never_moves");
}

#[test]
fn scenario_a_running_client_swaps_the_keys_the_frame_after_the_mode_changes() {
    scenario("a_running_client_swaps_the_keys_the_frame_after_the_mode_changes");
}

/// The input shell, the registration band, and one key pressed through it.
mod maps {
    use dereth_client::input::{InputShell, BASE_MAP_REGISTRATIONS};
    use dereth_input::fire::ControlType;
    use dereth_input::spec::ControlChord;
    use dereth_input::{ActionId, InputMapId};
    use dereth_testkit::HeadlessClient;

    /// The three sets of keys a combat mode chooses between, and the one that belongs to combat
    /// itself. Ids of shipped data, which is what the client's own table names them by.
    pub const MELEE_MAP: u32 = 0x1000_0003;
    pub const MISSILE_MAP: u32 = 0x1000_0004;
    pub const MAGIC_MAP: u32 = 0x1000_0005;
    pub const COMBAT_MAP: u32 = 0x1000_0002;
    pub const QUICKSLOT_MAP: u32 = 0x1000_000C;
    pub const UI_COMMANDS_MAP: u32 = 0x1000_0009;
    pub const MODE_MAPS: [u32; 3] = [MELEE_MAP, MISSILE_MAP, MAGIC_MAP];

    pub fn quick() -> InputMapId {
        InputMapId(QUICKSLOT_MAP)
    }

    /// The client's own input shell over the shipped tables.
    pub fn shell() -> InputShell {
        let store = dereth_dat::testing::open_store_or_fail();
        InputShell::new(&store, None).expect("the shipped input tables decode")
    }

    /// The registration band the gameplay keys live in, in walk order.
    pub fn band(s: &InputShell) -> Vec<u32> {
        s.manager
            .maps
            .entries()
            .iter()
            .filter(|e| e.priority == dereth_input::dispatch::priority::GAMEPLAY)
            .map(|e| e.map.0)
            .collect()
    }

    /// Which of the three mode sets are live, in walk order.
    pub fn live_mode_maps(s: &InputShell) -> Vec<u32> {
        band(s)
            .into_iter()
            .filter(|m| MODE_MAPS.contains(m))
            .collect()
    }

    /// None of the three is in the client's own start-up table.
    pub fn none_at_start_up() -> bool {
        !BASE_MAP_REGISTRATIONS
            .iter()
            .any(|(_, m, _)| MODE_MAPS.contains(m))
    }

    /// Every `(action, control)` the shipped keymap carries for one set of keys.
    pub fn shipped_bindings(s: &InputShell, map: u32) -> Vec<(ActionId, ControlChord)> {
        s.manager
            .keymap
            .section(InputMapId(map))
            .map(|sec| sec.bindings().iter().map(|(q, a)| (*a, *q)).collect())
            .unwrap_or_default()
    }

    /// The one control the shipped keymap binds `action` to in `map`, discovered rather than
    /// written down.
    pub fn the_shipped_control(s: &InputShell, map: u32, action: u32) -> ControlChord {
        let section = s
            .manager
            .keymap
            .section(InputMapId(map))
            .unwrap_or_else(|| panic!("the shipped keymap has a section for {map:#010X}"));
        let mut found: Vec<ControlChord> = section
            .bindings()
            .iter()
            .filter(|(_, a)| a.0 == action)
            .map(|(q, _)| *q)
            .collect();
        found.dedup();
        assert_eq!(
            found.len(),
            1,
            "{action:#010X} has exactly one shipped default control in {map:#010X}"
        );
        found[0]
    }

    /// The controls all three combat modes bind -- taken from the melee set's own shipped
    /// section, so the population is the data and not a list here.
    pub fn contested_controls(s: &InputShell) -> Vec<ControlChord> {
        let out: Vec<ControlChord> = shipped_bindings(s, MELEE_MAP)
            .iter()
            .map(|(_, q)| *q)
            .collect();
        assert!(!out.is_empty(), "the shipped melee section binds something");
        out
    }

    /// The one control the set that belongs to combat itself binds: the calibration key.
    pub fn the_combat_maps_own_control(s: &InputShell) -> ControlChord {
        let b = shipped_bindings(s, COMBAT_MAP);
        assert_eq!(
            b.len(),
            1,
            "the shipped combat section binds exactly one control"
        );
        b[0].1
    }

    /// The mode and the mode sets a **running** client is carrying.
    pub fn live_in(c: &mut HeadlessClient) -> (u32, Vec<u32>) {
        let s = c
            .app_mut()
            .input_manager_mut()
            .expect("the input shell is up");
        (
            s.combat_input_mode(),
            s.manager
                .maps
                .entries()
                .iter()
                .map(|e| e.map.0)
                .filter(|m| MODE_MAPS.contains(m))
                .collect(),
        )
    }

    /// The shard's own mode change: the form that takes no readiness check and sends nothing back.
    pub fn the_shard_sets_the_mode(
        c: &mut HeadlessClient,
        m: dereth_client_model::combat::CombatMode,
    ) {
        c.world_mut()
            .set_combat_mode(
                &mut dereth_client_model::NullRequests,
                &mut dereth_client_model::RecordingSink::default(),
                m,
                false,
                true,
                false,
            )
            .expect("the shard's own form takes no ready check");
    }

    /// One key press through the production shell, drained through the shell's own frame.
    pub struct Driver {
        /// Well clear of the double-click window and of the button history, so two presses of one
        /// physical key are never read as a gesture of each other.
        clock: u32,
    }

    impl Driver {
        pub const fn new() -> Self {
            Self { clock: 1_000 }
        }

        fn press(
            &mut self,
            shell: &mut InputShell,
            qc: &ControlChord,
        ) -> Vec<(InputMapId, ActionId)> {
            let metas: Vec<_> = shell
                .manager
                .keymap
                .meta_keys
                .iter()
                .filter(|(_, bit)| qc.meta_mode & bit != 0)
                .map(|(cs, _)| *cs)
                .collect();
            self.clock += 6_000;
            let mut t = self.clock;
            for cs in &metas {
                shell
                    .manager
                    .fire_input_event(*cs, ControlType::Button, 0x80, t);
                t += 10;
            }
            shell.use_time(dereth_primitives::LocalTime(f64::from(t) / 1000.0));
            shell.take_events();

            shell
                .manager
                .fire_input_event(qc.control, ControlType::Button, 0x80, t);
            shell.use_time(dereth_primitives::LocalTime(f64::from(t) / 1000.0));
            // Membership rather than the first: an action state outlives the key that set it, so
            // a latched toggle's release can be queued ahead of the new action.
            let got: Vec<(InputMapId, ActionId)> = shell
                .take_events()
                .iter()
                .map(|e| (e.input_map, e.action))
                .collect();

            t += 10;
            shell
                .manager
                .fire_input_event(qc.control, ControlType::Button, 0, t);
            for cs in metas.iter().rev() {
                t += 10;
                shell
                    .manager
                    .fire_input_event(*cs, ControlType::Button, 0, t);
            }
            shell.use_time(dereth_primitives::LocalTime(f64::from(t) / 1000.0));
            shell.take_events();
            got
        }

        /// A press followed by its release, with any modifiers held across both, answering the
        /// two event batches the client's own input layer produced.
        pub fn press_release(
            &mut self,
            shell: &mut InputShell,
            qc: &ControlChord,
        ) -> (
            Vec<dereth_client_runtime::actions::Action>,
            Vec<dereth_client_runtime::actions::Action>,
        ) {
            let metas: Vec<_> = shell
                .manager
                .keymap
                .meta_keys
                .iter()
                .filter(|(_, bit)| qc.meta_mode & bit != 0)
                .map(|(cs, _)| *cs)
                .collect();
            self.clock += 6_000;
            let mut t = self.clock;
            for cs in &metas {
                shell
                    .manager
                    .fire_input_event(*cs, ControlType::Button, 0x80, t);
                t += 10;
            }
            shell.use_time(dereth_primitives::LocalTime(f64::from(t) / 1000.0));
            shell.take_events();

            shell
                .manager
                .fire_input_event(qc.control, ControlType::Button, 0x80, t);
            shell.use_time(dereth_primitives::LocalTime(f64::from(t) / 1000.0));
            let down = shell.take_events();

            t += 10;
            shell
                .manager
                .fire_input_event(qc.control, ControlType::Button, 0, t);
            shell.use_time(dereth_primitives::LocalTime(f64::from(t) / 1000.0));
            let up = shell.take_events();

            for cs in metas.iter().rev() {
                t += 10;
                shell
                    .manager
                    .fire_input_event(*cs, ControlType::Button, 0, t);
            }
            shell.use_time(dereth_primitives::LocalTime(f64::from(t) / 1000.0));
            shell.take_events();
            self.clock = t;
            let hand_on = |events: Vec<dereth_input::InputEvent>| {
                events
                    .iter()
                    .map(dereth_input::InputEvent::to_action)
                    .collect()
            };
            (hand_on(down), hand_on(up))
        }

        /// The one action a press resolved to, or `None` for silence.
        pub fn resolve(
            &mut self,
            shell: &mut InputShell,
            qc: &ControlChord,
        ) -> Option<(InputMapId, ActionId)> {
            let got = self.press(shell, qc);
            assert!(
                got.len() <= 1,
                "a press resolved to {} actions: {got:?}",
                got.len()
            );
            got.first().copied()
        }
    }
}

// ---------------------------------------------------------------------------------------------
// combat.mode.* -- the seam between the model's combat mode and the window
//
// **The corpus census is read at run time and never pinned.** No recording and no count of mode
// changes is written down: this walks
// `dereth_client_net::client_session::testing::session_index()`, asserts that the corpus carries
// such a change at all, and asserts the client's answer to every one it finds.
// ---------------------------------------------------------------------------------------------

pub fn a_combat_mode_change_in_the_model_opens_the_window() {
    use dereth_client_model::combat::CombatMode;

    let mut c = window::a_client_in_the_world();
    window::settle(&mut c);
    let at_peace = window::the_window_is_up(&mut c);

    window::the_shard_sets_the_mode(&mut c, CombatMode::Missile);
    window::settle(&mut c);
    let in_combat = window::the_window_is_up(&mut c);

    window::the_shard_sets_the_mode(&mut c, CombatMode::NonCombat);
    window::settle(&mut c);
    let at_peace_again = window::the_window_is_up(&mut c);

    c.assert_behaviour(
        "combat.mode.a-change-in-the-model-opens-the-window-and-closing-combat-shuts-it",
        move |_| {
            at_peace == (false, false)
                && in_combat == (true, true)
                && at_peace_again == (false, false)
        },
    );
    c.shutdown();
}

pub fn the_shards_own_word_opens_the_window_and_the_client_sends_nothing() {
    use dereth_client_model::combat::CombatMode;
    use dereth_primitives::ObjectId;

    const THE_PLAYER: ObjectId = ObjectId(0x5290_0001);
    /// The number the shard keeps this character's combat mode in.
    const COMBAT_MODE: u32 = 0x28;

    let mut c = window::a_client_in_the_world();
    window::this_character_is(&mut c, THE_PLAYER);
    {
        let w = c.world_mut();
        w.player_system.options.set(
            dereth_client_model::player::options::option::ADVANCED_COMBAT_UI,
            false,
        );
        // Deliberately stale, so the mode change is what clears it.
        w.combat.advanced_combat_mode = true;
        w.player_system.options.set(
            dereth_client_model::player::options::option::AUTO_TARGET,
            false,
        );
        // A request of the player's own, left pending, so "the shard's word is not an answer to
        // it" is a measurement.
        w.combat.pending_combat_mode = CombatMode::Magic;
    }
    let shut = !window::the_window_is_up(&mut c).0;
    let sent_before = c.view().interaction().outbox().len();

    c.when(window::the_shard_says(
        None,
        10,
        COMBAT_MODE,
        CombatMode::Missile.raw(),
    ));
    let taken = (
        c.view().world().combat.combat_mode,
        c.view().world().combat.pending_combat_mode,
        c.view().world().combat.advanced_combat_mode,
        c.view().interaction().outbox().len(),
    );
    let keys = window::combat_keys(&mut c);
    c.world_mut().combat.pending_combat_mode = CombatMode::Undef;
    window::settle(&mut c);
    let up = window::the_window_is_up(&mut c);

    // The public form of the same message about this character is authority too, including the
    // way back to peace.
    c.when(window::the_shard_says(
        Some(THE_PLAYER),
        11,
        COMBAT_MODE,
        CombatMode::NonCombat.raw(),
    ));
    let back = (
        c.view().world().combat.combat_mode,
        window::combat_keys(&mut c),
    );
    window::settle(&mut c);
    let down = window::the_window_is_up(&mut c);

    c.assert_behaviour(
        "combat.mode.the-shards-own-word-opens-the-window-and-the-client-sends-nothing",
        move |_| {
            shut && taken.0 == CombatMode::Missile
                && taken.1 == CombatMode::Magic
                && !taken.2
                && taken.3 == sent_before
                && keys == CombatMode::Missile.raw()
                && up == (true, true)
                && back.0 == CombatMode::NonCombat
                && back.1 == CombatMode::NonCombat.raw()
                && down == (false, false)
        },
    );
    c.shutdown();
}

pub fn an_update_that_is_not_this_players_or_is_older_is_refused() {
    use dereth_client_model::combat::CombatMode;
    use dereth_primitives::ObjectId;

    const THE_PLAYER: ObjectId = ObjectId(0x5290_0002);
    const COMBAT_MODE: u32 = 0x28;
    /// Another of this character's numbers, so "the right property" is a measurement.
    const SOME_OTHER_NUMBER: u32 = 0x29;

    let mut c = window::a_client_in_the_world();
    window::this_character_is(&mut c, THE_PLAYER);
    c.when(window::the_shard_says(
        None,
        10,
        COMBAT_MODE,
        CombatMode::Missile.raw(),
    ));
    let live = c.view().world().combat.combat_mode;

    let stale_before = c.view().hud().stats.quality_updates_stale;
    c.when(window::the_shard_says(
        Some(ObjectId(THE_PLAYER.0 + 1)),
        11,
        COMBAT_MODE,
        1,
    ));
    c.when(window::the_shard_says(None, 11, SOME_OTHER_NUMBER, 1));
    c.when(window::the_shard_says(None, 9, COMBAT_MODE, 1));
    let after_the_three = (
        c.view().world().combat.combat_mode,
        c.view().hud().stats.quality_updates_stale,
    );

    let undecodable_before = c.view().hud().stats.undecodable;
    c.when(window::a_truncated_update(11, COMBAT_MODE, 1));
    let after_the_broken = (
        c.view().world().combat.combat_mode,
        c.view().hud().stats.undecodable,
    );

    // The two refusals are told apart: the stale one moved the stale count and not the
    // undecodable one, and the broken one moved the undecodable count and not the stale one.
    let told_apart = after_the_broken.1 == undecodable_before + 1
        && c.view().hud().stats.quality_updates_stale == after_the_three.1;

    // **What this scenario does not assert**: that after the character's own description is torn
    // down no such message moves the mode at all. That guard is the part of the client that keeps
    // the character's own numbers, and it is only one consumer. A `when` step delivers the message
    // to the **whole** frame's consumers, as a real frame does, and the object stream applies the
    // number with no such guard -- so the mode does move. Neither half is asserted here.
    c.assert_behaviour(
        "combat.mode.an-update-that-is-not-this-players-or-is-older-than-the-last-is-refused",
        move |_| {
            live == CombatMode::Missile
                && after_the_three == (CombatMode::Missile, stale_before + 1)
                && after_the_broken == (CombatMode::Missile, undecodable_before + 1)
                && told_apart
        },
    );
    c.shutdown();
}

pub fn the_recorded_combat_mode_changes_reach_the_window_the_buttons_and_the_keys() {
    use dereth_client_net::client_session::testing::{Corpus, Direction};
    use dereth_primitives::NetQueue;
    use dereth_protocol::{Message, Opcode};

    /// The number the shard keeps this character's combat mode in.
    const COMBAT_MODE: u32 = 0x28;

    let mut c = window::a_client_in_the_world();
    {
        let w = c.world_mut();
        w.player_system.options.set(
            dereth_client_model::player::options::option::ADVANCED_COMBAT_UI,
            false,
        );
        w.player_system.options.set(
            dereth_client_model::player::options::option::AUTO_TARGET,
            false,
        );
    }

    // Every recording the index names, and every mode change each of them really carries. No
    // count and no recording's name is written down.
    let mut seen = 0usize;
    let mut every_reading_holds = true;
    for (id, _slug) in dereth_client_net::client_session::testing::session_index() {
        let Ok(Some(corpus)) = Corpus::load(id) else {
            continue;
        };
        c.hud_mut().player_desc_received = true;
        let mut have_player = false;
        for row in &corpus.blobs {
            if row.dir != Direction::ServerToClient {
                continue;
            }
            if row.opcode == Opcode::LOGIN_CREATE_PLAYER.0 {
                let player = dereth_protocol::objects::LoginCreatePlayer::read(
                    &mut dereth_protocol::Reader::new(&row.payload[4..]),
                )
                .expect("the recorded identity decodes")
                .player_id;
                c.when(dereth_testkit::Inbound::event(
                    dereth_client_net::client_session::SessionEvent::PlayerCreated(player),
                ));
                window::this_character_is(&mut c, player);
                have_player = true;
            }
            if row.queue != NetQueue::UiQueue {
                continue;
            }
            let Ok(mut blob) = dereth_protocol::events::split_ui_blob(&row.payload) else {
                continue;
            };
            if blob.sub_type != Opcode::QUALITIES_PRIVATE_UPDATE_INT {
                continue;
            }
            let Ok(update) =
                dereth_protocol::qualities::QualitiesPrivateUpdateInt::read(&mut blob.body)
            else {
                continue;
            };
            if update.0.property_id != COMBAT_MODE {
                continue;
            }
            assert!(
                have_player,
                "a private update needs the recorded identity first"
            );
            let offset = if blob.order.is_some() {
                dereth_protocol::OrderedEventHeader::PACK_SIZE
            } else {
                0
            };
            let sent_before = c.view().interaction().outbox().len();
            c.when(dereth_testkit::Inbound::event(
                dereth_client_net::client_session::SessionEvent::UiEvent {
                    opcode: blob.sub_type,
                    blob: row.payload[offset..].to_vec(),
                },
            ));
            #[allow(clippy::cast_sign_loss)]
            let mode = update.0.value as u32;
            window::settle(&mut c);
            every_reading_holds &= c.view().world().combat.combat_mode.raw() == mode
                && window::combat_keys(&mut c) == mode
                && c.view().interaction().outbox().len() == sent_before
                && window::cluster_pages(&mut c) == (mode == 2 || mode == 4, mode == 8)
                && window::lit_mode_buttons(&mut c) == vec![mode];
            seen += 1;
        }
    }

    c.assert_behaviour(
        "combat.mode.the-recorded-changes-reach-the-window-the-buttons-and-the-keys",
        move |_| seen > 0 && every_reading_holds,
    );
    c.shutdown();
}

#[test]
fn scenario_a_combat_mode_change_in_the_model_opens_the_window() {
    scenario("a_combat_mode_change_in_the_model_opens_the_window");
}

#[test]
fn scenario_the_shards_own_word_opens_the_window_and_the_client_sends_nothing() {
    scenario("the_shards_own_word_opens_the_window_and_the_client_sends_nothing");
}

#[test]
fn scenario_an_update_that_is_not_this_players_or_is_older_is_refused() {
    scenario("an_update_that_is_not_this_players_or_is_older_is_refused");
}

#[test]
fn scenario_the_recorded_combat_mode_changes_reach_the_window_the_buttons_and_the_keys() {
    scenario("the_recorded_combat_mode_changes_reach_the_window_the_buttons_and_the_keys");
}

/// A whole client in the world, and what the combat cluster is showing.
mod window {
    use dereth_client_net::client_session::SessionEvent;
    use dereth_primitives::ObjectId;
    use dereth_testkit::{ClientSpec, HeadlessClient, Inbound};
    use dereth_ui::{Delivery, ElementId, Screen as _, UiSystem};
    use dereth_ui_screens::hud::combat_notice as cn;
    use dereth_ui_screens::screens::gameplay::{window as win, GamePlayScreen};

    /// The shipped gameplay screen over a loaded scene, which is what the combat cluster is drawn
    /// into.
    pub fn a_client_in_the_world() -> HeadlessClient {
        HeadlessClient::new(ClientSpec::gameplay_in_world(4))
    }

    /// The character the shard's own updates are about. The weenie has to exist, because that is
    /// the object the character's numbers live on.
    pub fn this_character_is(c: &mut HeadlessClient, id: ObjectId) {
        let w = c.world_mut();
        w.player = None;
        w.tables
            .weenies
            .insert(id, dereth_client_model::weenie::Weenie::new(id));
        assert!(w.set_player(id), "the identity is adopted once");
        c.hud_mut().player_desc_received = true;
    }

    /// One frame, plus the bounded delivery pass the queued show-and-hide messages need.
    pub fn settle(c: &mut HeadlessClient) {
        c.tick(1);
        for _ in 0..8 {
            let (ui, screen) = parts(c);
            let batch = ui.drain_outbox();
            if batch.is_empty() {
                return;
            }
            for d in batch {
                if let Delivery::Element { msg, .. } = d {
                    screen.on_element_message(&mut dereth_ui::framework::ScreenCx::new(ui), &msg);
                }
            }
        }
    }

    pub fn parts(c: &mut HeadlessClient) -> (&mut UiSystem, &mut GamePlayScreen) {
        let shell = c.app_mut().ui_mut().expect("the UI shell");
        let ui = &mut shell.ui;
        let screen = shell.flow.current_mut().expect("a current screen");
        let any: &mut dyn std::any::Any = &mut **screen;
        let screen = any
            .downcast_mut::<GamePlayScreen>()
            .expect("the gameplay screen");
        (ui, screen)
    }

    fn visible(c: &mut HeadlessClient, id: ElementId) -> bool {
        let (ui, screen) = parts(c);
        let root = screen.root().expect("the gameplay root");
        let h = ui
            .get_child_recursive(root, id)
            .unwrap_or_else(|| panic!("{id:?} is in the shipped tree"));
        ui.node(h).expect("alive").region.flags.visible
    }

    /// `(the cluster of controls, the window itself)`.
    pub fn the_window_is_up(c: &mut HeadlessClient) -> (bool, bool) {
        (
            visible(c, cn::COMBAT_UI_PAGE),
            visible(c, win::COMBAT_PANEL),
        )
    }

    /// `(the fighting cluster, the casting page)`.
    pub fn cluster_pages(c: &mut HeadlessClient) -> (bool, bool) {
        (
            visible(c, cn::COMBAT_UI_PAGE),
            visible(c, cn::SPELLCASTING_PAGE),
        )
    }

    /// The modes whose toolbar button is lit.
    pub fn lit_mode_buttons(c: &mut HeadlessClient) -> Vec<u32> {
        let buttons: Vec<(u32, ElementId)> =
            dereth_ui_screens::toolbar::combat_mode::BUTTONS.to_vec();
        buttons
            .into_iter()
            .filter(|(_, id)| visible(c, *id))
            .map(|(m, _)| m)
            .collect()
    }

    /// Which set of combat keys the client is carrying.
    pub fn combat_keys(c: &mut HeadlessClient) -> u32 {
        c.app_mut()
            .input_manager_mut()
            .expect("the input shell")
            .combat_input_mode()
    }

    /// The shard's own mode change through the model, for the scenario whose subject is the
    /// window rather than the message.
    pub fn the_shard_sets_the_mode(
        c: &mut HeadlessClient,
        m: dereth_client_model::combat::CombatMode,
    ) {
        c.world_mut()
            .set_combat_mode(
                &mut dereth_client_model::NullRequests,
                &mut dereth_client_model::RecordingSink::default(),
                m,
                false,
                true,
                false,
            )
            .expect("the shard's own form takes no ready check");
    }

    /// One quality update, built the way the shard writes it: the private form when no subject is
    /// named, and the public one when one is.
    pub fn the_shard_says(
        subject: Option<ObjectId>,
        sequence: u8,
        property: u32,
        value: u32,
    ) -> Inbound {
        Inbound::event(update(subject, sequence, property, value))
    }

    /// The same message with its last byte missing, which is what an undecodable one is.
    pub fn a_truncated_update(sequence: u8, property: u32, value: u32) -> Inbound {
        let mut e = update(None, sequence, property, value);
        if let SessionEvent::UiEvent { blob, .. } = &mut e {
            blob.pop();
        }
        Inbound::event(e)
    }

    fn update(subject: Option<ObjectId>, sequence: u8, property: u32, value: u32) -> SessionEvent {
        let opcode = if subject.is_some() {
            dereth_protocol::Opcode::QUALITIES_UPDATE_INT
        } else {
            dereth_protocol::Opcode::QUALITIES_PRIVATE_UPDATE_INT
        };
        let mut blob = opcode.0.to_le_bytes().to_vec();
        blob.push(sequence);
        if let Some(id) = subject {
            blob.extend_from_slice(&id.0.to_le_bytes());
        }
        blob.extend_from_slice(&property.to_le_bytes());
        blob.extend_from_slice(&value.to_le_bytes());
        SessionEvent::UiEvent { opcode, blob }
    }
}

// ---------------------------------------------------------------------------------------------
// combat.window.* and vitals.row.*
//
// **The vitals row is `ui`'s subject and is kept here**, in `behaviours/combat.rs`, beside the
// combat window it is drawn with.
//
// **One narrowing, stated rather than hidden.** Each option box's caption id is the hash of its
// shipped localisation name (`ID_CombatPanelOption_*`), so the ids are the shipped names'; the
// English words are not asserted, because they are a transcription of the shipped table and the
// table is what resolves them. What is asserted instead is stronger about the client: the words
// the table gives reach the element **and the draw list**, and the three boxes do not all say the
// same thing.
// ---------------------------------------------------------------------------------------------

pub fn the_combat_option_boxes_draw_their_shipped_captions() {
    let (mut ui, mut screen, combat) = panels::a_combat_window();
    panels::show_combat(&mut ui, &mut screen);
    let table =
        dereth_ui_screens::env::did_by_enum(&ui, 4, 0x1000_0003).expect("the shipped text table");
    let back = panels::drawn(&mut ui);

    let mut composed: Vec<String> = Vec::new();
    let mut every_box_holds = true;
    for (i, (caption, help)) in [
        (
            "ID_CombatPanelOption_AutoRepeatAttack",
            "ID_PlayerOption_AutoRepeatAttack_Help",
        ),
        (
            "ID_CombatPanelOption_AutoTarget",
            "ID_PlayerOption_AutoTarget_Help",
        ),
        (
            "ID_CombatPanelOption_ViewCombatTarget",
            "ID_PlayerOption_ViewCombatTarget_Help",
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let h = combat.options[i].element;
        let caption_id = dereth_ui::persist::preferences::token_of(caption);
        let resolved = ui
            .resolve_string(table, caption_id)
            .expect("the table resolves it");
        let help_text = ui
            .resolve_string(table, dereth_ui::persist::preferences::token_of(help))
            .expect("and resolves the help line");
        let named = ui
            .node(h)
            .expect("alive")
            .merged_properties()
            .get_string_info(dereth_ui::props::attr::TEXT_STRING)
            .map(|si| (si.table_id, si.string_id));
        every_box_holds &= !resolved.is_empty()
            && named == Some((Some(table), Some(caption_id)))
            && panels::text(&mut ui, h) == resolved
            && panels::drawn_text(&back, h) == resolved
            && !help_text.is_empty()
            && ui.node(h).expect("alive").tooltip_text.as_deref() == Some(help_text.as_str());
        composed.push(resolved);
    }
    let all_different = {
        let mut v = composed.clone();
        v.sort();
        v.dedup();
        v.len() == composed.len()
    };

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.window.the-option-boxes-draw-their-shipped-captions-and-carry-their-shipped-help",
        move |_| every_box_holds && all_different && composed.len() == 3,
    );
}

pub fn the_attack_height_buttons_follow_the_notice() {
    use dereth_ui_screens::hud::combat_window::height;

    let (mut ui, mut screen, mut combat) = panels::a_combat_window();
    let group = combat.height_group.expect("the shipped height group");
    let ships_medium = ui
        .node(group)
        .expect("alive")
        .merged_properties()
        .get_enum(0xB0)
        == Some(0x1000_0058);
    panels::show_combat(&mut ui, &mut screen);

    // No notice yet: the window opens on the shipped default.
    let opens_on_medium = panels::height_art_holds(&mut ui, &combat, height::MEDIUM)
        && ui
            .node(group)
            .expect("alive")
            .merged_properties()
            .get_enum(0xB1)
            == Some(0x1000_0058);

    // Every change, including the first one repeating the default.
    let mut every_change_holds = true;
    for h in [height::MEDIUM, height::HIGH, height::LOW, height::MEDIUM] {
        every_change_holds &= combat.on_attack_height_changed(&mut ui, h)
            && panels::height_art_holds(&mut ui, &combat, h);
    }
    // And a change to no height at all is refused, leaving the height where it was.
    let refused = !combat.on_attack_height_changed(&mut ui, height::UNDEF)
        && panels::height_art_holds(&mut ui, &combat, height::MEDIUM);

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.window.the-attack-height-buttons-follow-the-notice-and-draw-the-chosen-one",
        move |_| ships_medium && opens_on_medium && every_change_holds && refused,
    );
}

pub fn pressing_the_chosen_height_again_keeps_it_chosen() {
    use dereth_ui::msg::element::id::BUTTON_CLICKED;
    use dereth_ui::widgets::groupbox as gb;
    use dereth_ui::StateId;

    let (mut ui, _, combat) = panels::a_combat_window();
    let group = combat.height_group.expect("the shipped height group");
    let high = combat.height_buttons[0].0;
    let low = combat.height_buttons[2].0;
    let receiver = dereth_ui::ListenerId::External(0x_66D0);

    let allows_repeats = ui
        .node(group)
        .expect("alive")
        .merged_properties()
        .get_bool(gb::ALLOW_RESELECT)
        == Some(true);
    ui.register_for_element_messages(group, receiver);
    ui.drain_outbox();
    let mut heard = |ui: &mut dereth_ui::UiSystem| {
        ui.drain_outbox()
            .into_iter()
            .filter(|d| {
                matches!(d, dereth_ui::Delivery::Element { to, msg }
                    if *to == receiver && msg.id == BUTTON_CLICKED)
            })
            .count()
    };

    ui.broadcast_element_message(high, BUTTON_CLICKED, 7, 0);
    let first = heard(&mut ui) == 1
        && ui.node(high).expect("alive").state == StateId(6)
        && ui
            .node(group)
            .expect("alive")
            .merged_properties()
            .get_enum(gb::SELECTED_BUTTON)
            == Some(0x1000_0057);

    // The press has already turned the button off by the time the panel hears about it, so the
    // panel putting it back is the whole of the claim -- with and without the gate.
    let mut both_gates_hold = true;
    for allow in [false, true] {
        ui.set_attribute_bool(group, gb::ALLOW_RESELECT, allow);
        ui.set_state(high, StateId(1));
        ui.broadcast_element_message(high, BUTTON_CLICKED, 7, 0);
        both_gates_hold &= heard(&mut ui) == usize::from(allow)
            && ui.node(high).expect("alive").state == StateId(6)
            && ui
                .node(high)
                .expect("alive")
                .merged_properties()
                .get_bool(0x0E)
                == Some(true);
    }

    // And the closed gate does not stop a press on a **different** height.
    ui.set_attribute_bool(group, gb::ALLOW_RESELECT, false);
    ui.broadcast_element_message(low, BUTTON_CLICKED, 7, 0);
    let a_different_one = heard(&mut ui) == 1
        && ui.node(low).expect("alive").state == StateId(6)
        && ui.node(high).expect("alive").state == StateId(1);

    // A message that is not a press selects nothing.
    ui.broadcast_element_message(high, dereth_ui::MessageId(0x0A), 0, 0);
    let not_a_press = ui.node(low).expect("alive").state == StateId(6);

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.window.pressing-the-chosen-height-again-keeps-it-chosen",
        move |_| allows_repeats && first && both_gates_hold && a_different_one && not_a_press,
    );
}

pub fn choosing_one_height_leaves_every_other_button_alone() {
    use dereth_ui::widgets::groupbox as gb;
    use dereth_ui::StateId;

    let (mut ui, _, combat) = panels::a_combat_window();
    let group = combat.height_group.expect("the shipped height group");
    let high = combat.height_buttons[0].0;
    let medium = combat.height_buttons[1].0;
    let low = combat.height_buttons[2].0;

    // The chosen one is found however deep it sits, and a button in a state of its own is left
    // exactly where it was.
    ui.set_parent(low, Some(high));
    ui.set_state(high, StateId(3));
    ui.set_attribute_enum(group, gb::SELECTED_BUTTON, 0x1000_0059);
    let nested = ui.node(low).expect("alive").state == StateId(6)
        && ui.node(medium).expect("alive").state == StateId(1)
        && ui.node(high).expect("alive").state == StateId(3);

    // Naming a button the panel does not have clears the old one and chooses nothing.
    ui.set_attribute_enum(group, gb::SELECTED_BUTTON, 0xDEAD_BEEF);
    let missing = ui.node(low).expect("alive").state == StateId(1);
    ui.set_attribute_enum(group, gb::SELECTED_BUTTON, 0);
    let nothing_named = ui.node(high).expect("alive").state == StateId(3);

    // A default of nothing does not overwrite a choice that has been made.
    ui.set_attribute_enum(group, gb::SELECTED_BUTTON, 0x1000_0058);
    ui.set_attribute_enum(group, gb::DEFAULT_BUTTON, 0);
    ui.post_init_tree(group);
    let kept = ui
        .node(group)
        .expect("alive")
        .merged_properties()
        .get_enum(gb::SELECTED_BUTTON)
        == Some(0x1000_0058)
        && ui.node(medium).expect("alive").state == StateId(6);

    // And re-reading the choice with nothing new to say puts the same one back.
    ui.set_state(medium, StateId(1));
    ui.on_set_attribute(group, gb::SELECTED_BUTTON, None);
    let re_applied = ui.node(medium).expect("alive").state == StateId(6);

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.window.choosing-one-height-leaves-every-other-button-alone",
        move |_| nested && missing && nothing_named && kept && re_applied,
    );
}

pub fn the_vital_rows_are_drawn_into_the_shipped_template() {
    use dereth_ui_screens::view::Vital;

    let (mut ui, mut screen, _) = panels::a_combat_window();
    let table_id =
        dereth_ui_screens::env::did_by_enum(&ui, 4, 0x1000_0001).expect("the shipped table");
    let row = ui
        .env()
        .map(|e| {
            e.with_assets(|assets| {
                let table =
                    <dereth_assets::ui::StringTable as dereth_assets::Decode>::decode_payload(
                        table_id,
                        &assets.read(table_id).expect("the table reads"),
                    )
                    .expect("and decodes");
                table
                    .strings
                    .into_iter()
                    .find(|(id, _)| *id == 0x0593_85AC)
                    .expect("the vitals row is in it")
                    .1
            })
        })
        .expect("the shipped assets are installed");
    // The row is a template of fragments with two holes in it, not a ready-made sentence.
    let is_a_template = row.strings.len() == 3 && row.variables.len() == 2;

    let mut every_reading_holds = true;
    for values in [
        [(35, 35), (70, 70), (50, 100)],
        [(5, 35), (0, 70), (91, 101)],
    ] {
        every_reading_holds &= screen.update_vitals(&mut ui, &panels::Vitals(values));
        // A frame whose numbers have not moved does not rewrite them.
        every_reading_holds &= !screen.update_vitals(&mut ui, &panels::Vitals(values));
        for (vital, (cur, max)) in Vital::ALL.into_iter().zip(values) {
            let (_, label) = dereth_ui_screens::hud::vitals::fields(vital);
            let want = format!(
                "{}{cur}{}{max}{}",
                row.strings[0], row.strings[1], row.strings[2]
            );
            for h in [
                screen
                    .stacked_vitals
                    .get(label)
                    .expect("the stacked layout's row"),
                screen
                    .side_vitals
                    .get(label)
                    .expect("the side layout's row"),
            ] {
                every_reading_holds &= panels::text(&mut ui, h) == want;
            }
        }
    }

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "vitals.row.the-numbers-are-drawn-into-the-shipped-template-in-both-layouts",
        move |_| is_a_template && every_reading_holds,
    );
}

#[test]
fn scenario_the_combat_option_boxes_draw_their_shipped_captions() {
    scenario("the_combat_option_boxes_draw_their_shipped_captions");
}

#[test]
fn scenario_the_attack_height_buttons_follow_the_notice() {
    scenario("the_attack_height_buttons_follow_the_notice");
}

#[test]
fn scenario_pressing_the_chosen_height_again_keeps_it_chosen() {
    scenario("pressing_the_chosen_height_again_keeps_it_chosen");
}

#[test]
fn scenario_choosing_one_height_leaves_every_other_button_alone() {
    scenario("choosing_one_height_leaves_every_other_button_alone");
}

#[test]
fn scenario_the_vital_rows_are_drawn_into_the_shipped_template() {
    scenario("the_vital_rows_are_drawn_into_the_shipped_template");
}

/// The shipped gameplay tree with no client under it: the panels these claims are about are bound
/// off the real layout and driven directly, which is the only shape that can ask a panel a question
/// at a clock the scenario chooses.
mod panels {
    use std::rc::Rc;
    use std::sync::Arc;

    use dereth_primitives::{AssetSource, DataId, ObjectId};
    use dereth_ui::{Delivery, ElemHandle, RecordingDrawBackend, Screen as _, StateId, UiSystem};
    use dereth_ui_screens::hud::combat_window::CombatWindow;
    use dereth_ui_screens::screens::gameplay::GamePlayScreen;
    use dereth_ui_screens::view::{GameView, Vital};

    /// The shipped `classic_gameplay` tree, built by the screen's own startup.
    pub fn gameplay() -> (UiSystem, GamePlayScreen) {
        let store = Arc::new(dereth_dat::testing::open_store_or_fail());
        let master_id = DataId(0x3900_0001);
        let master = <dereth_assets::MasterProperty as dereth_assets::Decode>::decode_payload(
            master_id,
            &store.read(master_id).expect("the shipped property table"),
        )
        .expect("it decodes");
        let mut ui = UiSystem::new((800, 600));
        ui.property_types = master.property_types();
        ui.strings = Some(Rc::new(dereth_client::ui_draw::DatStringResolver::new(
            Arc::clone(&store),
        )));
        ui.fonts = Some(Rc::new(dereth_client::ui_draw::DatFontProvider::new(
            Arc::clone(&store),
        )));
        let mut flow = dereth_ui::UiFlow::new();
        dereth_ui_screens::register_all(&mut ui, &mut flow);
        let assets = Rc::new(store);
        let resolver = Rc::new(
            dereth_ui::framework::DidMapperResolver::load_via_master(assets.as_ref())
                .expect("the shipped mapper"),
        );
        dereth_ui_screens::env::install(&mut ui, assets, resolver);
        let mut screen = GamePlayScreen::default();
        screen
            .create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
            .expect("the gameplay screen builds from the shipped layout");
        // The screen's own start-up requests and notices belong to the screen and not to any
        // scenario; a bench that left them queued would hand the first frame somebody else's work.
        ui.requests.clear();
        ui.notice_inbox.clear();
        (ui, screen)
    }

    /// The same, with the combat window bound off it.
    pub fn a_combat_window() -> (UiSystem, GamePlayScreen, CombatWindow) {
        let (mut ui, mut screen) = gameplay();
        pump(&mut ui, &mut screen);
        let mut combat = CombatWindow::default();
        combat.post_init(&mut ui, screen.root().expect("the gameplay root"));
        assert_eq!(
            combat.failures, 0,
            "every child the window looks up is in the shipped tree"
        );
        (ui, screen, combat)
    }

    /// Deliver what the tree has queued until it settles.
    pub fn pump(ui: &mut UiSystem, screen: &mut GamePlayScreen) {
        for _ in 0..16 {
            let batch = ui.drain_outbox();
            if batch.is_empty() {
                return;
            }
            for d in batch {
                if let Delivery::Element { msg, .. } = d {
                    screen.on_element_message(&mut dereth_ui::framework::ScreenCx::new(ui), &msg);
                }
            }
        }
        panic!("the tree's own messages did not settle");
    }

    /// Put the window up, as a combat mode arriving does.
    pub fn show_combat(ui: &mut UiSystem, screen: &mut GamePlayScreen) {
        screen.on_set_combat_mode(ui, 4, false, 0);
        pump(ui, screen);
    }

    pub fn text(ui: &mut UiSystem, h: ElemHandle) -> String {
        ui.text_element_mut(h)
            .expect("a text element")
            .glyphs
            .inq_text(false)
    }

    /// What the tree really put in the draw list this pass.
    pub fn drawn(ui: &mut UiSystem) -> RecordingDrawBackend {
        let mut back = RecordingDrawBackend::default();
        ui.draw(&mut back);
        back
    }

    /// The glyphs one element really contributed to it.
    pub fn drawn_text(back: &RecordingDrawBackend, h: ElemHandle) -> String {
        String::from_utf16(
            &back
                .calls
                .iter()
                .filter(|c| c.who == h)
                .flat_map(|c| c.glyphs.iter().map(|g| g.ch))
                .collect::<Vec<_>>(),
        )
        .expect("the client's own glyphs are text")
    }

    /// Whether every height button is drawing the picture a chosen or an unchosen one gets.
    pub fn height_art_holds(ui: &mut UiSystem, combat: &CombatWindow, selected: u32) -> bool {
        let back = drawn(ui);
        let mut holds = true;
        for &(h, height) in &combat.height_buttons {
            let chosen = height == selected;
            let state = StateId(if chosen { 6 } else { 1 });
            let n = ui.node(h).expect("alive");
            // The two pictures are the shipped button's own state media, read back off the
            // element rather than inferred.
            let want = DataId(if chosen { 0x0600_4D1E } else { 0x0600_4D1C });
            let declared = n.desc.access_state(state).is_some_and(|s| {
                s.media.iter().any(|m| {
                    matches!(m.fields, dereth_assets::ui::MediaFields::Image { file, .. }
                        if file == want)
                })
            });
            let images: Vec<DataId> = back
                .calls
                .iter()
                .filter(|c| c.who == h)
                .filter_map(|c| c.image)
                .collect();
            holds &= n.state == state
                && n.merged_properties().get_bool(0x0E).unwrap_or(false) == chosen
                && declared
                && images == vec![want];
        }
        holds
    }

    /// The three numbers a vitals row is drawn from.
    #[derive(Debug)]
    pub struct Vitals(pub [(u32, u32); 3]);

    impl GameView for Vitals {
        fn player(&self) -> Option<ObjectId> {
            Some(ObjectId(0x5066_0042))
        }
        fn vital(&self, _: ObjectId, vital: Vital) -> Option<(u32, u32)> {
            Some(
                self.0[match vital {
                    Vital::Health => 0,
                    Vital::Stamina => 1,
                    Vital::Mana => 2,
                }],
            )
        }
    }
}

// ---------------------------------------------------------------------------------------------
// combat.power-bar.*
//
// **Two benches, because the claims ask different questions.** The charge-meter claims are
// about a whole client's frame and use one; the rate and gauge claims are about what the panels
// draw at clocks the scenario chooses, and a whole client cannot be driven at an arbitrary clock,
// so they drive the same three pieces `App::frame` drives -- the interaction slot, the panel
// update and the notice fan-out -- in the order the real frame runs them. **That order is the
// other way round from the natural one** and every reading is consequently one frame behind the
// clock; driving them the natural way would hide the lag, so the lag is measured rather than
// described.
//
// **Two calibrations have no row of their own**: that the shipped tree carries two of these bars
// and binds both, and that a level read back off the tree can be made non-zero before any zero is
// believed. They are what makes every zero below mean something, and they are asserted inside the
// scenarios as premises.
// ---------------------------------------------------------------------------------------------

pub fn a_terminal_answer_empties_the_classic_meter_and_the_next_charge_works() {
    let mut c = meter::a_client_ready_to_shoot();

    meter::begin(&mut c);
    c.tick(12);
    let charged = meter::window_meter(&mut c).expect("a real frame wrote a level");
    let bars_are_down = meter::every_bar_is_down(&mut c);

    meter::release(&mut c);
    c.tick(25);
    let swung = c.view().world().combat.attack_server_response_pending;

    let done_before = c.view().interaction().stats.attacks_done;
    meter::the_shard_answers(&mut c, meter::A_TERMINAL_ANSWER);
    let after = (
        c.view().interaction().stats.attacks_done == done_before + 1,
        c.view().world().combat.repeat_attacking,
        c.view().world().combat.build_in_progress,
        c.view().world().combat.power_bar_mode,
    );
    c.tick(1);
    let emptied = meter::window_meter(&mut c) == Some(0.0);
    let mut stays_empty = true;
    for _ in 0..3 {
        c.tick(1);
        stays_empty &= meter::window_meter(&mut c) == Some(0.0);
    }

    // And the next charge fills it again and swings again.
    meter::begin(&mut c);
    c.tick(12);
    let charges_again = meter::window_meter(&mut c).is_some_and(|v| v > 0.1);
    meter::release(&mut c);
    c.tick(25);
    let swings_again = c.view().world().combat.attack_server_response_pending;
    meter::the_shard_answers(&mut c, meter::A_TERMINAL_ANSWER);
    c.tick(1);
    let emptied_again = meter::window_meter(&mut c) == Some(0.0);

    c.assert_behaviour(
        "combat.power-bar.a-terminal-answer-empties-the-classic-meter-and-the-next-charge-works",
        move |_| {
            charged > 0.1
                && bars_are_down
                && swung
                && after
                    == (
                        true,
                        false,
                        false,
                        dereth_client_model::combat::PowerBarMode::Undef,
                    )
                && emptied
                && stays_empty
                && charges_again
                && swings_again
                && emptied_again
        },
    );
    c.shutdown();
}

pub fn the_standalone_bar_and_the_classic_meter_are_separate_routes() {
    use dereth_client_model::combat::PowerBarMode;

    let mut c = meter::a_client_ready_to_shoot();
    // A classic charge first, so an accidental write by an advanced notice would be visible.
    {
        let w = c.world_mut();
        w.combat.begin_power_bar(PowerBarMode::Combat, false, 0);
        w.combat.set_power_bar_level(0.625);
    }
    c.tick(1);
    let classic = meter::window_meter(&mut c) == Some(0.625);

    // Hide the classic one, then begin and charge an advanced one, all before a frame.
    {
        let w = c.world_mut();
        w.combat.hide_power_bar();
        w.combat
            .begin_power_bar(PowerBarMode::AdvancedCombat, true, 2);
        w.combat.set_power_bar_level(0.75);
    }
    c.tick(1);
    let advanced = (
        meter::window_meter(&mut c) == Some(0.0),
        meter::subscribers(&mut c) == vec![(true, 0.75)],
        meter::unregistered_are_down(&mut c),
    );

    // Three things happening to one charge, ending in the state it was already in: the panel is
    // told each of them, which is the difference between being told and being shown.
    let written = c.view().hud().stats.power_bar_writes;
    {
        let w = c.world_mut();
        w.combat.hide_power_bar();
        // Hiding leaves the cache where it was, which is what makes the end state identical.
        assert!((w.combat.latest_power_bar_level - 0.75).abs() < 1e-6);
        w.combat
            .begin_power_bar(PowerBarMode::AdvancedCombat, false, 0);
        w.combat.set_power_bar_level(0.75);
    }
    c.tick(1);
    let told_each = c.view().hud().stats.power_bar_writes == written + 3
        && meter::window_meter(&mut c) == Some(0.0)
        && meter::subscribers(&mut c) == vec![(true, 0.75)];

    c.world_mut().combat.hide_power_bar();
    c.tick(1);
    let put_away = meter::every_bar_is_down(&mut c) && meter::window_meter(&mut c) == Some(0.0);

    // An idle frame says nothing again.
    let settled = c.view().hud().stats.power_bar_writes;
    c.tick(1);
    let idle = c.view().hud().stats.power_bar_writes == settled;

    c.assert_behaviour(
        "combat.power-bar.the-standalone-bar-and-the-classic-meter-are-separate-routes",
        move |_| classic && advanced == (true, true, true) && told_each && put_away && idle,
    );
    c.shutdown();
}

pub fn a_jump_raises_the_standalone_bar_and_not_the_classic_one() {
    use dereth_client_model::combat::PowerBarMode;

    let mut c = meter::a_client_ready_to_shoot();
    {
        let w = c.world_mut();
        w.combat.begin_power_bar(PowerBarMode::Jump, false, 0);
        w.combat.jump_pending = true;
        w.combat.set_power_bar_level(0.4);
    }
    c.tick(1);
    let raised = meter::subscribers(&mut c) == vec![(true, 0.4)];
    let classic_stays_down = meter::unregistered_are_down(&mut c);

    c.world_mut().combat.finish_jump();
    c.tick(1);
    // The window's own meter is not asked about here: nothing in this scenario has ever written
    // it, and `never written` is a different reading from `written with a nought`.
    let put_away = meter::every_bar_is_down(&mut c);
    let consumed = c.world_mut().combat.take_power_bar_notices().is_empty();

    c.assert_behaviour(
        "combat.power-bar.a-jump-raises-the-standalone-bar-and-not-the-classic-one",
        move |_| raised && classic_stays_down && put_away && consumed,
    );
    c.shutdown();
}

pub fn a_notice_with_no_panel_to_hear_it_is_dropped() {
    use dereth_client_model::combat::PowerBarMode;

    // **What this scenario does not do, and why.** A client whose UI is configured and whose
    // shell has not been started would have no panel of any kind for a charge to reach, but
    // `ClientSpec` cannot build that: its one switch turns the shell on and the UI configuration
    // with it. The half asserted is the one a player can reach -- another screen destroys the
    // panels, and what was raised while it was up must not animate the tree that is built when he
    // comes back.
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    let two_bars = c.view().expect_app().hud().panels.power_bar.bars.len() == 2;
    let starts_empty = meter::window_meter(&mut c).is_none() && meter::every_bar_is_down(&mut c);

    {
        let w = c.world_mut();
        w.combat
            .begin_power_bar(PowerBarMode::AdvancedCombat, false, 0);
        w.combat.set_power_bar_level(0.25);
    }
    c.tick(1);
    let a_new_charge_arrives = meter::subscribers(&mut c) == vec![(true, 0.25)];

    // Another screen: the panels are destroyed, and charges raised while it is up must not
    // animate the tree that is rebuilt afterwards.
    c.app_mut()
        .queue_ui_mode(dereth_ui::framework::mode::CREDITS);
    c.tick(2);
    let mut nothing_kept_elsewhere = true;
    for _ in 0..2 {
        c.world_mut().combat.set_power_bar_level(0.9);
        c.tick(1);
        nothing_kept_elsewhere &= c.world_mut().combat.take_power_bar_notices().is_empty();
    }
    {
        let w = c.world_mut();
        w.combat
            .begin_power_bar(PowerBarMode::AdvancedCombat, false, 0);
        w.combat.set_power_bar_level(0.9);
    }
    c.app_mut()
        .queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    c.tick(4);
    let rebuilt_empty = meter::window_meter(&mut c).is_none()
        && meter::every_bar_is_down(&mut c)
        && c.view().expect_app().hud().panels.power_bar.bars.len() == 2;

    {
        let w = c.world_mut();
        w.combat
            .begin_power_bar(PowerBarMode::AdvancedCombat, false, 0);
        w.combat.set_power_bar_level(0.5);
    }
    c.tick(1);
    let and_it_still_works = meter::subscribers(&mut c) == vec![(true, 0.5)];

    c.assert_behaviour(
        "combat.power-bar.a-notice-with-no-panel-to-hear-it-is-dropped-and-never-replayed",
        move |_| {
            two_bars
                && starts_empty
                && a_new_charge_arrives
                && nothing_kept_elsewhere
                && rebuilt_empty
                && and_it_still_works
        },
    );
    c.shutdown();
}

pub fn the_power_bar_follows_the_clock_at_one_full_charge_a_second() {
    use dereth_client::interaction::action as ia;
    use dereth_client_model::combat::PowerBarMode;

    let readable = bars::the_readback_can_produce_a_non_zero();
    let mut b = bars::Bench::new();
    b.enter_advanced_combat();

    b.frame(vec![bars::key(ia::COMBAT_MEDIUM_ATTACK, true)], 100.0);
    let building =
        b.combat().power_bar_mode == PowerBarMode::AdvancedCombat && b.combat().build_in_progress;

    let mut drawn = Vec::new();
    let mut only_the_subscriber = true;
    for step in 1..=6 {
        b.frame(vec![], 100.0 + f64::from(step) * 0.1);
        let levels = b.drawn_levels();
        let sub = b.subscriber();
        only_the_subscriber &= levels.len() == 2 && levels[1 - sub].is_none();
        drawn.push(levels[sub].expect("the subscriber was written"));
    }

    // The ramp, one frame behind the clock, and a rate rather than six readings that match.
    let on_the_ramp = drawn
        .iter()
        .enumerate()
        .all(|(i, v)| (v - bars::step_of(i)).abs() < 1e-5);
    let steps: Vec<f32> = drawn.windows(2).map(|w| w[1] - w[0]).collect();
    let even = steps.iter().all(|s| (s - 0.1).abs() < 1e-5);
    let rising = drawn.windows(2).all(|w| w[1] > w[0]);

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.power-bar.it-follows-the-clock-at-one-full-charge-a-second",
        move |_| readable && building && only_the_subscriber && on_the_ramp && even && rising,
    );
}

pub fn a_two_handed_style_charges_a_quarter_faster() {
    use dereth_client::interaction::action as ia;

    let mut b = bars::Bench::new();
    b.enter_advanced_combat();
    b.set_style(dereth_client_model::combat::DUAL_WIELD_COMBAT_STYLE);

    b.frame(vec![bars::key(ia::COMBAT_MEDIUM_ATTACK, true)], 200.0);
    let mut drawn = Vec::new();
    for step in 1..=6 {
        b.frame(vec![], 200.0 + f64::from(step) * 0.1);
        drawn.push(b.drawn_level());
    }
    let steps: Vec<f32> = drawn.windows(2).map(|w| w[1] - w[0]).collect();
    let steeper = steps.iter().all(|s| (s - 0.125).abs() < 1e-5);

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.power-bar.a-two-handed-style-charges-a-quarter-faster-over-the-same-frames",
        move |_| steeper && !steps.is_empty(),
    );
}

pub fn exactly_one_of_the_two_power_displays_is_live() {
    use dereth_client::interaction::action as ia;
    use dereth_client_model::combat::PowerBarMode;
    use dereth_ui_screens::hud::powerbar as pb;

    let readable = bars::the_readback_can_produce_a_non_zero();
    // Ordinary combat: the window's meter carries it and the standalone bars are never written.
    let mut classic = bars::Bench::new();
    classic.frame(vec![bars::key(ia::COMBAT_MEDIUM_ATTACK, true)], 300.0);
    let classic_mode = classic.combat().power_bar_mode == PowerBarMode::Combat;
    let mut classic_shown = Vec::new();
    let mut classic_holds = true;
    for step in 1..=4 {
        let notified = classic.combat().latest_power_bar_level;
        classic.frame(vec![], 300.0 + f64::from(step) * 0.1);
        classic_holds &= classic
            .bar_modes()
            .iter()
            .all(|m| *m == pb::PowerBarMode::Undef)
            && classic.drawn_levels() == vec![None, None]
            && classic.bars_visible() == vec![false, false];
        let shown = classic
            .window_meter()
            .expect("the window's meter was written");
        classic_holds &= (shown - notified).abs() < 1e-6;
        classic_shown.push(shown);
    }
    let classic_moves = classic_shown.windows(2).all(|w| w[1] > w[0]);

    // The advanced interface: the standalone bar carries it and the window's meter refuses.
    let mut advanced = bars::Bench::new();
    advanced.enter_advanced_combat();
    advanced.frame(vec![bars::key(ia::COMBAT_MEDIUM_ATTACK, true)], 400.0);
    let never_written = advanced.window_meter().is_none()
        && advanced.combat().power_bar_mode == PowerBarMode::AdvancedCombat;
    let mut advanced_shown = Vec::new();
    let mut advanced_holds = true;
    for step in 1..=4 {
        let notified = advanced.combat().latest_power_bar_level;
        advanced.frame(vec![], 400.0 + f64::from(step) * 0.1);
        let sub = advanced.subscriber();
        advanced_holds &=
            advanced.bars_visible() == vec![false, true] && advanced.window_meter().is_none();
        let shown = advanced.drawn_levels()[sub].expect("the standalone bar was written");
        advanced_holds &= (shown - notified).abs() < 1e-6;
        advanced_shown.push(shown);
    }
    let advanced_moves = advanced_shown.windows(2).all(|w| w[1] > w[0]);

    // The same clock and the same four steps: one value, drawn in two places.
    let one_value = advanced_shown
        .iter()
        .zip(classic_shown.iter())
        .all(|(a, c)| (a - c).abs() < 1e-5);

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.power-bar.exactly-one-of-the-two-displays-is-live-and-they-never-differ",
        move |_| {
            readable
                && classic_mode
                && classic_holds
                && classic_moves
                && never_written
                && advanced_holds
                && advanced_moves
                && one_value
        },
    );
}

pub fn the_power_bar_is_shown_when_a_charge_begins_and_emptied_when_it_ends() {
    use dereth_client::interaction::action as ia;
    use dereth_ui_screens::hud::powerbar as pb;

    let mut b = bars::Bench::new();
    b.enter_advanced_combat();
    let down_at_first = b.bars_visible() == vec![false, false];

    b.frame(vec![bars::key(ia::COMBAT_MEDIUM_ATTACK, true)], 500.0);
    b.frame(vec![], 500.4);
    let sub = b.subscriber();
    let shown = b.bars_visible() == vec![false, true]
        && b.bar_modes()[sub] == pb::PowerBarMode::AdvancedCombat
        && b.bar_modes()[1 - sub] == pb::PowerBarMode::Undef;

    b.frame(vec![], 500.5);
    let charged = b.drawn_level() > 0.0;

    b.frame(vec![bars::key(ia::COMBAT_MEDIUM_ATTACK, false)], 500.6);
    b.hide_the_bar();
    b.frame(vec![], 500.7);
    let put_away = b.bars_visible() == vec![false, false]
        && b.drawn_levels() == vec![None, Some(0.0)]
        && b.bar_modes().iter().all(|m| *m == pb::PowerBarMode::Undef);

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.power-bar.it-is-shown-when-the-charge-begins-and-hidden-and-emptied-when-it-ends",
        move |_| down_at_first && shown && charged && put_away,
    );
}

pub fn the_keyboard_gauge_has_seven_notches_and_starts_half_way() {
    let mut g = dereth_client_model::combat::CombatState::begin();
    let starts_half_way = (g.ui_requested_power - 0.5).abs() < 1e-9;

    let mut seen: Vec<f32> = vec![g.ui_requested_power];
    for _ in 0..10 {
        seen.push(g.adjust_ui_requested_power(false));
    }
    for _ in 0..12 {
        seen.push(g.adjust_ui_requested_power(true));
    }
    let mut notches = seen;
    notches.sort_by(f32::total_cmp);
    notches.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    let seven = notches.len() == 7;
    #[allow(clippy::cast_precision_loss)]
    let evenly_spaced = notches
        .iter()
        .enumerate()
        .all(|(i, n)| (n - i as f32 / 6.0).abs() < 1e-6);
    let saturates = (notches[0] - 0.0).abs() < 1e-9 && (notches[6] - 1.0).abs() < 1e-9;
    let half_is_a_notch = (notches[3] - 0.5).abs() < 1e-6;

    let mut d = dereth_client_model::combat::CombatState::begin();
    let up_and_back = (d.adjust_ui_requested_power(true) - 4.0 / 6.0).abs() < 1e-6
        && (d.adjust_ui_requested_power(false) - 0.5).abs() < 1e-6;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.power-bar.the-keyboard-gauge-has-seven-notches-and-starts-half-way",
        move |_| {
            starts_half_way && seven && evenly_spaced && saturates && half_is_a_notch && up_and_back
        },
    );
}

pub fn the_power_drag_is_continuous_and_lands_between_the_notches() {
    let mut g = dereth_client_model::combat::CombatState::begin();
    let a = g.set_ui_requested_power_from_scrollbar(500);
    let b = g.set_ui_requested_power_from_scrollbar(501);
    let one_unit =
        (a - 0.5).abs() < 1e-6 && (b - 0.501).abs() < 1e-6 && (b - a - 0.001).abs() < 1e-6;

    let values: Vec<f32> = (400..420)
        .map(|p| g.set_ui_requested_power_from_scrollbar(p))
        .collect();
    let distinct = {
        let mut v = values.clone();
        v.sort_by(f32::total_cmp);
        v.dedup_by(|x, y| (*x - *y).abs() < 1e-9);
        v.len() == 20
    };
    let notches: Vec<f32> = (0u8..=6).map(|i| f32::from(i) / 6.0).collect();
    let off_the_notches = values
        .iter()
        .all(|v| notches.iter().all(|n| (n - v).abs() > 1e-4));

    let clamps = (g.set_ui_requested_power_from_scrollbar(0) - 0.0).abs() < 1e-9
        && (g.set_ui_requested_power_from_scrollbar(1_000) - 1.0).abs() < 1e-9
        && (g.set_ui_requested_power_from_scrollbar(4_000) - 1.0).abs() < 1e-9
        && (g.set_ui_requested_power_from_scrollbar(u32::MAX) - 1.0).abs() < 1e-9;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.power-bar.the-drag-is-continuous-and-lands-between-the-notches",
        move |_| one_unit && distinct && off_the_notches && clamps,
    );
}

pub fn the_notch_and_the_fill_are_different_things() {
    use dereth_client::interaction::action as ia;

    let mut b = bars::Bench::new();
    b.set_requested_power_from_a_drag(833);
    b.frame(vec![bars::key(ia::COMBAT_MEDIUM_ATTACK, true)], 600.0);
    b.frame(vec![], 600.25);
    b.frame(vec![], 600.5);

    let notch = b.drawn_notch().expect("the drag's mark was written");
    let fill = b.window_meter().expect("the window's meter was written");
    let on_the_drag = (notch - 0.833).abs() < 1e-5;
    let and_the_fill_is_the_clocks = fill > 0.0 && (fill - notch).abs() > 1e-3;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.power-bar.the-notch-and-the-fill-are-different-things-on-different-elements",
        move |_| on_the_drag && and_the_fill_is_the_clocks,
    );
}

pub fn the_display_holds_the_level_the_swing_went_out_at() {
    use dereth_client::interaction::action as ia;
    use dereth_primitives::LocalTime;

    let mut b = bars::Bench::new();
    b.frame(vec![bars::key(ia::COMBAT_MEDIUM_ATTACK, true)], 700.0);
    b.frame(vec![], 700.5);
    // While the charge is building the clock and what was last sent agree, which is why a display
    // that recomputed looked right until the swing.
    let agree = (b.combat().power_bar_level(LocalTime(700.5)) - b.combat().latest_power_bar_level)
        .abs()
        < 1e-5;

    b.frame(vec![bars::key(ia::COMBAT_MEDIUM_ATTACK, false)], 700.55);
    let closed = !b.combat().attack_request_in_progress;
    b.the_swing_goes_out();
    let gone = !b.combat().build_in_progress
        && b.combat().power_bar_level(LocalTime(700.6)).abs() < f32::EPSILON;
    let sent = b.combat().latest_power_bar_level;

    b.frame(vec![], 700.6);
    let held = b.window_meter().is_some_and(|m| (m - sent).abs() < 1e-5);

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.power-bar.the-display-holds-the-level-the-swing-went-out-at",
        move |_| agree && closed && gone && sent > 0.0 && held,
    );
}

#[test]
fn scenario_a_terminal_answer_empties_the_classic_meter_and_the_next_charge_works() {
    scenario("a_terminal_answer_empties_the_classic_meter_and_the_next_charge_works");
}

#[test]
fn scenario_the_standalone_bar_and_the_classic_meter_are_separate_routes() {
    scenario("the_standalone_bar_and_the_classic_meter_are_separate_routes");
}

#[test]
fn scenario_a_jump_raises_the_standalone_bar_and_not_the_classic_one() {
    scenario("a_jump_raises_the_standalone_bar_and_not_the_classic_one");
}

#[test]
fn scenario_a_notice_with_no_panel_to_hear_it_is_dropped() {
    scenario("a_notice_with_no_panel_to_hear_it_is_dropped");
}

#[test]
fn scenario_the_power_bar_follows_the_clock_at_one_full_charge_a_second() {
    scenario("the_power_bar_follows_the_clock_at_one_full_charge_a_second");
}

#[test]
fn scenario_a_two_handed_style_charges_a_quarter_faster() {
    scenario("a_two_handed_style_charges_a_quarter_faster");
}

#[test]
fn scenario_exactly_one_of_the_two_power_displays_is_live() {
    scenario("exactly_one_of_the_two_power_displays_is_live");
}

#[test]
fn scenario_the_power_bar_is_shown_when_a_charge_begins_and_emptied_when_it_ends() {
    scenario("the_power_bar_is_shown_when_a_charge_begins_and_emptied_when_it_ends");
}

#[test]
fn scenario_the_keyboard_gauge_has_seven_notches_and_starts_half_way() {
    scenario("the_keyboard_gauge_has_seven_notches_and_starts_half_way");
}

#[test]
fn scenario_the_power_drag_is_continuous_and_lands_between_the_notches() {
    scenario("the_power_drag_is_continuous_and_lands_between_the_notches");
}

#[test]
fn scenario_the_notch_and_the_fill_are_different_things() {
    scenario("the_notch_and_the_fill_are_different_things");
}

#[test]
fn scenario_the_display_holds_the_level_the_swing_went_out_at() {
    scenario("the_display_holds_the_level_the_swing_went_out_at");
}

/// A whole client charging its bar, and what its two displays show.
mod meter {
    use dereth_primitives::{LocalTime, ObjectId};
    use dereth_testkit::{ClientSpec, HeadlessClient, Inbound};
    use dereth_ui_screens::bind::{attr, attr_float};

    /// The answer that ends a swing.
    pub const A_TERMINAL_ANSWER: u32 = 0x36;

    const PLAYER: ObjectId = ObjectId(0x5430_0002);
    const A_TARGET: ObjectId = ObjectId(0x8430_0777);

    /// A client in the gameplay screen whose body is ready to shoot at something.
    pub fn a_client_ready_to_shoot() -> HeadlessClient {
        let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
        {
            let w = c.world_mut();
            w.set_player(PLAYER);
            w.tables
                .weenies
                .insert(PLAYER, dereth_client_model::Weenie::new(PLAYER));
            let mut victim = dereth_client_model::Weenie::new(A_TARGET);
            victim.pwd.obj_type = dereth_client_model::weenie::item_type::CREATURE;
            victim.pwd.bitfield = dereth_client_model::weenie::bitfield::ATTACKABLE;
            w.tables.weenies.insert(A_TARGET, victim);
            w.set_selected_object(Some(A_TARGET), false, &mut dereth_client_model::NullSink);
            w.combat.combat_mode = dereth_client_model::combat::CombatMode::Missile;
            // The stance and the queue a body that is ready to shoot is in. This scenario is
            // about the display, not about how a body becomes ready.
            w.combat.current_style = dereth_client_model::combat::MISSILE_READY_STYLES[0];
            w.combat.forward_command = dereth_client_model::combat::MOTION_READY;
            w.player_system.options.set(
                dereth_client_model::player::options::option::AUTO_REPEAT_ATTACK,
                true,
            );
        }
        c.tick(1);
        c
    }

    /// Start a charge, as the body's own ready position allows.
    pub fn begin(c: &mut HeadlessClient) {
        let now = c.view().hud().now;
        let w = c.world_mut();
        assert!(
            w.player_in_ready_position(true, Some(false)),
            "the body is ready to shoot"
        );
        w.start_attack_request(true, LocalTime(now.0))
            .expect("a real target starts a request");
        assert!(w.combat.build_in_progress, "and the charge is building");
    }

    /// Let it go, and let the frame swing at the gauge's cap.
    pub fn release(c: &mut HeadlessClient) {
        let now = c.view().hud().now;
        c.world_mut().end_attack_request(
            &mut dereth_client_model::RecordingRequests::default(),
            dereth_client_model::combat::AttackHeight::Medium,
            None,
            true,
            now,
        );
    }

    /// The shard's answer to the swing.
    pub fn the_shard_answers(c: &mut HeadlessClient, error: u32) {
        c.when(Inbound::message(
            &dereth_protocol::combat::CombatHandleAttackDoneEvent { error },
        ));
    }

    /// The combat window's own meter.
    pub fn window_meter(c: &mut HeadlessClient) -> Option<f32> {
        let h = c
            .view()
            .expect_app()
            .hud()
            .panels
            .combat_window
            .actual_power
            .expect("the shipped window's meter");
        let ui = &c.view().expect_app().ui().expect("the UI shell").ui;
        attr_float(ui, h, attr::METER_LEVEL)
    }

    /// `(shown, level)` for every standalone bar that really hears the charge notices.
    pub fn subscribers(c: &mut HeadlessClient) -> Vec<(bool, f32)> {
        c.view()
            .expect_app()
            .hud()
            .panels
            .power_bar
            .bars
            .iter()
            .filter(|b| b.registered)
            .map(|b| (b.visible, b.level))
            .collect()
    }

    /// Whether every standalone bar that hears nothing is down.
    pub fn unregistered_are_down(c: &mut HeadlessClient) -> bool {
        c.view()
            .expect_app()
            .hud()
            .panels
            .power_bar
            .bars
            .iter()
            .filter(|b| !b.registered)
            .all(|b| !b.visible)
    }

    /// Whether every standalone bar is down and empty.
    pub fn every_bar_is_down(c: &mut HeadlessClient) -> bool {
        c.view()
            .expect_app()
            .hud()
            .panels
            .power_bar
            .bars
            .iter()
            .all(|b| !b.visible && b.level.abs() < f32::EPSILON)
    }
}

/// The three pieces of a frame the rate and gauge claims are about, driven at clocks the scenario
/// chooses. See this section's header for the order and the lag.
mod bars {
    use dereth_client::hud::Hud;
    use dereth_client::interaction::Interaction;
    use dereth_client_model::combat::{CombatMode, PowerBarMode};
    use dereth_primitives::{LocalTime, ObjectId};
    use dereth_ui::UiSystem;
    use dereth_ui_screens::bind::{attr, attr_float};
    use dereth_ui_screens::hud::powerbar as pb;
    use dereth_ui_screens::panels::remaining::RemainingPanels;

    use super::panels;

    /// What the ramp reads at step `i`: one frame behind the clock, in tenths.
    #[allow(clippy::cast_precision_loss)]
    pub fn step_of(i: usize) -> f32 {
        i as f32 / 10.0
    }

    pub fn key(action: u32, start: bool) -> dereth_client_runtime::actions::Action {
        dereth_client_runtime::actions::Action {
            id: dereth_input::ActionId(action),
            phase: if start {
                dereth_client_runtime::actions::ActionPhase::Begin
            } else {
                dereth_client_runtime::actions::ActionPhase::End
            },
            extent: 1.0,
            repeats: 1,
        }
    }

    /// **The calibration the two `None` readings rest on**, on a tree of its own because writing
    /// a level is exactly what the scenarios assert did not happen: a level read back off the
    /// shipped tree can be made non-zero, a charge beginning zeroes it, and the second bar in the
    /// tree is a second bar and not an alias of the first.
    pub fn the_readback_can_produce_a_non_zero() -> bool {
        let (mut ui, screen) = panels::gameplay();
        let mut p = RemainingPanels::default();
        p.post_init(&mut ui, screen.root().expect("the gameplay root"));
        let read = |ui: &UiSystem, b: &pb::PowerBar| {
            attr_float(ui, b.bound.get("bar").expect("bound"), attr::METER_LEVEL)
        };
        let before = read(&ui, &p.power_bar.bars[0]).is_none();
        let zeroed = p.power_bar.bars[0].begin(&mut ui, pb::PowerBarMode::Jump, false, 0)
            && read(&ui, &p.power_bar.bars[0]) == Some(0.0);
        let moved = p.power_bar.bars[0].set_level(&mut ui, pb::PowerBarMode::Jump, 0.375)
            && read(&ui, &p.power_bar.bars[0]) == Some(0.375);
        let independent = read(&ui, &p.power_bar.bars[1]).is_none();
        before && zeroed && moved && independent
    }

    pub struct Bench {
        inter: Interaction,
        objects: dereth_client::objects::ObjectStream,
        store: std::sync::Arc<dereth_dat::RetailDatStore>,
        hud: Hud,
        ui: UiSystem,
        panels: RemainingPanels,
    }

    impl Bench {
        pub fn new() -> Self {
            let (mut ui, screen) = panels::gameplay();
            let root = screen.root().expect("the gameplay root");
            let mut panels = RemainingPanels::default();
            panels.post_init(&mut ui, root);

            // **The two calibrations.** Both stand as premises here: every zero below is worthless
            // without them.
            assert_eq!(
                panels.power_bar.bound(),
                2,
                "the shipped tree carries two of these bars"
            );
            assert!(
                panels.power_bar.bars.iter().all(|b| b.bound.is_complete()),
                "and every child each of them looks up is in the tree"
            );
            assert!(
                panels.power_bar.bars.iter().all(|b| {
                    let h = b.element.expect("bound");
                    !ui.node(h).expect("alive").region.flags.visible
                }),
                "both start down, which is what makes the showing observable"
            );
            assert!(
                panels.power_bar.bars.iter().all(|b| {
                    attr_float(&ui, b.bound.get("bar").expect("bound"), attr::METER_LEVEL).is_none()
                }),
                "and neither carries a level until a notice writes one -- which is what makes \
                 `never written` and `written with a nought` two different readings"
            );

            let mut objects = dereth_client::objects::ObjectStream::default();
            objects.world = seed_world();
            Self {
                inter: Interaction::new(),
                objects,
                store: std::sync::Arc::new(dereth_dat::testing::open_store_or_fail()),
                hud: Hud::new(),
                ui,
                panels,
            }
        }

        /// One frame: draw what the previous frame produced, then run the combat system -- which
        /// is the order `App::frame` runs them in, and the reason every reading is one frame
        /// behind the clock.
        pub fn frame(&mut self, actions: Vec<dereth_client_runtime::actions::Action>, t: f64) {
            let notices = self.objects.world.combat.take_power_bar_notices();
            let view = self.hud.view(&self.objects);
            let _ = self.panels.update(&mut self.ui, &view);
            drop(view);
            let _ = dereth_client::hud::deliver_power_bar_notices(
                &mut self.ui,
                &mut self.panels,
                notices,
            );
            let _ = dereth_client::interaction::use_time(
                &mut self.inter,
                &self.store,
                None,
                &mut self.objects,
                None,
                actions,
                false,
                (800, 600),
                LocalTime(t),
            );
        }

        pub fn combat(&self) -> &dereth_client_model::combat::CombatState {
            &self.objects.world.combat
        }

        pub fn set_style(&mut self, style: u32) {
            self.objects.world.combat.current_style = style;
        }

        pub fn set_requested_power_from_a_drag(&mut self, position: u32) {
            self.objects
                .world
                .combat
                .set_ui_requested_power_from_scrollbar(position);
        }

        pub fn hide_the_bar(&mut self) {
            self.objects.world.combat.hide_power_bar();
        }

        pub fn the_swing_goes_out(&mut self) {
            self.objects.world.handle_commence_attack();
        }

        /// The level attribute on each standalone bar, read back off the element tree. **`None`
        /// is not zero**: the shipped layout declares none until a notice writes one.
        pub fn drawn_levels(&self) -> Vec<Option<f32>> {
            self.panels
                .power_bar
                .bars
                .iter()
                .map(|b| {
                    attr_float(
                        &self.ui,
                        b.bound.get("bar").expect("bound"),
                        attr::METER_LEVEL,
                    )
                })
                .collect()
        }

        pub fn bar_modes(&self) -> Vec<pb::PowerBarMode> {
            self.panels.power_bar.bars.iter().map(|b| b.mode).collect()
        }

        /// Which of the two bars the notices can really reach.
        pub fn subscriber(&self) -> usize {
            self.panels
                .power_bar
                .bars
                .iter()
                .position(|b| b.registered)
                .expect("exactly one of the two hears the charge notices")
        }

        pub fn drawn_level(&self) -> f32 {
            self.drawn_levels()[self.subscriber()].expect("a notice has written this bar")
        }

        pub fn window_meter(&self) -> Option<f32> {
            let h = self
                .panels
                .combat_window
                .actual_power
                .expect("the window's meter");
            attr_float(&self.ui, h, attr::METER_LEVEL)
        }

        /// The mark the drag leaves, which is a different attribute on a different element.
        pub fn drawn_notch(&self) -> Option<f32> {
            let h = self
                .panels
                .combat_window
                .desired_power
                .expect("the window's gauge");
            attr_float(&self.ui, h, attr::MARKER)
        }

        pub fn bars_visible(&self) -> Vec<bool> {
            self.panels
                .power_bar
                .bars
                .iter()
                .map(|b| {
                    let h = b.element.expect("bound");
                    self.ui.node(h).is_some_and(|n| n.region.flags.visible)
                })
                .collect()
        }

        /// Turn the advanced interface on through the client's own producer: the option, and then
        /// a mode change, which is the only writer of the cached flag in a running client.
        pub fn enter_advanced_combat(&mut self) {
            let w = &mut self.objects.world;
            w.player_system.options.set(
                dereth_client_model::player::options::option::ADVANCED_COMBAT_UI,
                true,
            );
            assert!(
                !w.combat.advanced_combat_mode,
                "the option alone changes nothing"
            );
            let mut req = dereth_client_model::NullRequests;
            let mut sink = dereth_client_model::NullSink;
            w.set_combat_mode(
                &mut req,
                &mut sink,
                CombatMode::NonCombat,
                false,
                true,
                false,
            )
            .expect("peace is always compatible");
            w.set_combat_mode(&mut req, &mut sink, CombatMode::Melee, false, true, false)
                .expect("unarmed melee is always compatible");
            assert!(
                w.combat.advanced_combat_mode,
                "the mode change re-read the option; without that every reading below would be \
                 about a field written by hand"
            );
            assert_ne!(w.combat.power_bar_mode, PowerBarMode::Combat);
        }
    }

    /// A player in melee with an attackable creature selected, carrying the table the melee ready
    /// arm reads -- which is the state a swing is actually allowed from.
    fn seed_world() -> dereth_client_model::World {
        let mut w = dereth_client_model::World::new();
        let player = ObjectId(0x5430_0002);
        let monster = ObjectId(0x8430_0777);
        w.player = Some(player);
        w.tables
            .weenies
            .insert(player, dereth_client_model::Weenie::new(player));
        w.tables
            .weenies
            .get_mut(player)
            .expect("just inserted")
            .pwd
            .name = "Aldis".into();
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
        w.combat.combat_mode = CombatMode::Melee;
        w.tables
            .weenies
            .get_mut(player)
            .expect("the player")
            .qualities
            .get_or_insert_with(dereth_client_model::Qualities::new)
            .set(
                dereth_client_model::qualities::StatKey::new(
                    dereth_client_model::qualities::StatType::Did,
                    dereth_client_model::combat::COMBAT_TABLE_DID,
                ),
                dereth_client_model::qualities::StatValue::Did(dereth_primitives::DataId(
                    0x3000_0000,
                )),
            );
        assert!(
            w.player_in_ready_position(true, Some(false)),
            "the body must be able to attack at all, or every reading is a refusal it did not \
             intend"
        );
        w
    }
}

// ---------------------------------------------------------------------------------------------
// combat.advanced.*
//
// **The bench is the shipped tree with the three pieces of a frame over it**, in the order the
// real frame runs them and with the delivery pass between them -- which is the whole mechanism by
// which a panel's visibility reaches the window it sits in, so a bench that skipped it would
// assert the cluster and never the window.
//
// **Two checks have no row of their own.** One is the calibration of the bench: that both windows
// can be seen moving in both directions, and that a level read back off a strip can be made
// non-zero -- which is what makes every absence below mean something, and which stands as a premise
// instead. The other is the recording's own frame -- that it is one clean session, reassembles
// completely and completes no blob twice. That is a claim about the corpus, the corpus property
// test owns it, and these two scenarios read the **locked decoded** corpus rather than reassembling
// a raw capture for themselves, so the reassembly is not theirs to check either.
//
// **No count and no word is pinned.** These walk the recording and assert the shape: the edge
// exists, the saves differ in one bit, the swings before it sit on notches and the ones after do
// not.
// ---------------------------------------------------------------------------------------------

pub fn the_advanced_combat_option_suppresses_the_classic_combat_window() {
    use dereth_client_model::combat::CombatMode;

    // The calibration the absences below rest on: the window can be seen coming up and going down
    // again, through the client's own producer.
    let mut off = advanced::Bench::new();
    off.frame(vec![], 200.0);
    let starts_down = !off.combat_window() && off.strip() == vec![false, false];
    let off_by_default = !off.world().player_system.options.advanced_combat_ui();
    off.set_mode(CombatMode::Melee);
    off.frame(vec![], 200.1);
    let can_come_up = off.combat_page() && off.combat_window() && off.delivered() > 0;
    off.set_mode(CombatMode::NonCombat);
    off.frame(vec![], 200.2);
    let can_go_down = !off.combat_window();

    // ...and back up, which is the reading the suppressed one is compared against.
    off.set_mode(CombatMode::Melee);
    off.frame(vec![], 200.3);
    let up_with_the_option_off = off.combat_page() && off.combat_window();

    // The same character and the same mode change with the option on.
    let mut on = advanced::Bench::new();
    on.frame(vec![], 200.0);
    on.set_advanced(true);
    on.set_mode(CombatMode::Melee);
    on.frame(vec![], 200.1);
    let really_in_melee = on.world().combat.combat_mode == CombatMode::Melee;
    let suppressed = !on.combat_page() && !on.combat_window();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.advanced.the-option-suppresses-the-classic-combat-window",
        move |_| {
            starts_down
                && off_by_default
                && can_come_up
                && can_go_down
                && up_with_the_option_off
                && really_in_melee
                && suppressed
        },
    );
}

pub fn turning_the_advanced_option_on_while_the_window_is_up_takes_it_down() {
    use dereth_client_model::combat::CombatMode;

    let mut b = advanced::Bench::new();
    b.set_mode(CombatMode::Melee);
    b.frame(vec![], 300.0);
    let up = b.combat_window();

    b.set_advanced(true);
    b.frame(vec![], 300.1);
    let down = !b.combat_page() && !b.combat_window();

    // And back: an edge that only ever hid would pass both stations above.
    b.set_advanced(false);
    b.frame(vec![], 300.2);
    let up_again = b.combat_window();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.advanced.turning-the-option-on-while-the-window-is-up-takes-it-down",
        move |_| up && down && up_again,
    );
}

pub fn the_strip_comes_up_while_an_attack_key_is_held_and_goes_on_release() {
    use dereth_client::interaction::action as ia;
    use dereth_client_model::combat::{CombatMode, PowerBarMode};

    let mut b = advanced::Bench::new();
    b.set_advanced(true);
    b.set_mode(CombatMode::Melee);
    // Deliberately nothing to swing at: the advanced arm starts a build anyway, so the release
    // sends nothing and the hide comes out of the release itself rather than out of a fixture
    // standing in for the shard.
    b.nothing_selected();
    b.frame(vec![], 400.0);
    let starts_down = b.strip() == vec![false, false];

    b.frame(vec![advanced::key(ia::COMBAT_MEDIUM_ATTACK, true)], 400.1);
    let building = b.world().combat.power_bar_mode == PowerBarMode::AdvancedCombat
        && b.world().combat.build_in_progress;
    b.frame(vec![], 400.2);
    let shown = b.strip() == vec![false, true]
        && !b.combat_window()
        // The charge beginning zeroes the one it reached, and the other is not written at all.
        && b.strip_levels() == vec![None, Some(0.0)];
    b.frame(vec![], 400.4);
    let held = b.strip_levels();
    let charging = held[1].is_some_and(|v| v > 0.0) && held[0].is_none();

    b.frame(vec![advanced::key(ia::COMBAT_MEDIUM_ATTACK, false)], 400.5);
    let did_not_send = !b.world().combat.attack_server_response_pending
        && b.world().combat.power_bar_mode == PowerBarMode::Undef;
    b.frame(vec![], 400.6);
    let gone = b.strip() == vec![false, false]
        && b.strip_levels() == vec![None, Some(0.0)]
        && !b.combat_window();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.advanced.the-strip-comes-up-while-an-attack-key-is-held-and-goes-on-release",
        move |_| starts_down && building && shown && charging && did_not_send && gone,
    );
}

pub fn a_release_that_reaches_the_shard_holds_the_strip_until_the_answer() {
    use dereth_client::interaction::action as ia;
    use dereth_client_model::combat::{CombatMode, PowerBarMode};
    use dereth_primitives::LocalTime;

    let mut b = advanced::Bench::new();
    b.set_advanced(true);
    b.set_mode(CombatMode::Melee);
    // The repeat is off, so the answer's other arm -- the one that hides -- is what this is about.
    b.set_auto_repeat(false);
    b.frame(vec![], 500.0);

    b.frame(vec![advanced::key(ia::COMBAT_MEDIUM_ATTACK, true)], 500.1);
    b.frame(vec![], 500.5);
    let held = b.strip() == vec![false, true];

    b.frame(vec![advanced::key(ia::COMBAT_MEDIUM_ATTACK, false)], 500.6);
    let sent =
        b.world().combat.attack_server_response_pending && !b.world().combat.build_in_progress;
    b.frame(vec![], 500.7);
    let still_up = b.strip() == vec![false, true];

    b.the_shard_answers_the_swing(LocalTime(500.8));
    let cleared = b.world().combat.power_bar_mode == PowerBarMode::Undef;
    b.frame(vec![], 500.9);
    let gone = b.strip() == vec![false, false];

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.advanced.a-release-that-reaches-the-shard-holds-the-strip-until-the-answer",
        move |_| held && sent && still_up && cleared && gone,
    );
}

pub fn the_advanced_option_chooses_which_display_is_live() {
    use dereth_client::interaction::action as ia;
    use dereth_client_model::combat::{CombatMode, PowerBarMode};

    let mut classic = advanced::Bench::new();
    classic.set_mode(CombatMode::Melee);
    classic.frame(vec![], 600.0);
    classic.frame(vec![advanced::key(ia::COMBAT_MEDIUM_ATTACK, true)], 600.1);
    for i in 1..=3 {
        classic.frame(vec![], 600.1 + f64::from(i) * 0.1);
    }
    let classic_holds = classic.world().combat.power_bar_mode == PowerBarMode::Combat
        && classic.strip() == vec![false, false]
        && classic.strip_levels() == vec![None, None]
        && classic.window_meter().is_some_and(|v| v > 0.0);

    let mut adv = advanced::Bench::new();
    adv.set_advanced(true);
    adv.set_mode(CombatMode::Melee);
    adv.frame(vec![], 600.0);
    adv.frame(vec![advanced::key(ia::COMBAT_MEDIUM_ATTACK, true)], 600.1);
    for i in 1..=3 {
        adv.frame(vec![], 600.1 + f64::from(i) * 0.1);
    }
    let levels = adv.strip_levels();
    let advanced_holds = adv.world().combat.power_bar_mode == PowerBarMode::AdvancedCombat
        && adv.strip() == vec![false, true]
        && levels[0].is_none()
        && levels[1].is_some_and(|v| v > 0.0)
        && adv.window_meter().is_none();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.advanced.the-option-chooses-which-display-is-live-and-only-one-ever-is",
        move |_| classic_holds && advanced_holds,
    );
}

pub fn the_advanced_option_is_off_at_login_and_the_toggle_is_one_bit() {
    let recorded = advanced::the_recorded_session();
    let default_word = dereth_client_model::player::options::DEFAULT_OPTIONS;
    let on_word = default_word | advanced::THE_OPTION_BIT;

    // The character's own description, as the shard sent it.
    let carries = |b: &[u8], w: u32| {
        b.windows(4)
            .any(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]) == w)
    };
    let described: Vec<&dereth_client_net::client_session::testing::CorpusBlob> = recorded
        .blobs
        .iter()
        .filter(|b| {
            b.dir == dereth_client_net::client_session::testing::Direction::ServerToClient
                && advanced::is_a_description(b)
        })
        .collect();
    let arrives_off = described.len() == 1
        && carries(&described[0].payload, default_word)
        && !carries(&described[0].payload, on_word);

    // Every option save the recorded client sent, in order.
    let saves: Vec<u32> = advanced::the_recorded_option_saves(&recorded);
    let flags: Vec<bool> = saves
        .iter()
        .map(|w| w & advanced::THE_OPTION_BIT != 0)
        .collect();
    let goes_on_and_off = flags.first() == Some(&true) && flags.last() == Some(&false);
    let one_bit = saves
        .iter()
        .all(|w| (w ^ default_word) & !advanced::THE_OPTION_BIT == 0);
    let off_is_the_default = saves.last() == Some(&default_word);

    // And the bit the table names is the one the wire moved.
    let (name, word, mask) = dereth_client_model::player::options::PLAYER_OPTIONS
        [dereth_client_model::player::options::option::ADVANCED_COMBAT_UI];
    let named = name == "AdvancedCombatUI"
        && word == dereth_client_model::player::options::OptionWord::One
        && mask == advanced::THE_OPTION_BIT
        && !dereth_client_model::player::options::default_option_value(
            dereth_client_model::player::options::option::ADVANCED_COMBAT_UI,
        );

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.advanced.the-option-is-off-at-login-and-the-toggle-is-one-bit-on-the-wire",
        move |_| {
            arrives_off
                && saves.len() >= 2
                && goes_on_and_off
                && one_bit
                && off_is_the_default
                && named
        },
    );
}

pub fn the_recorded_attacks_are_charges_after_the_toggle_and_notches_before() {
    let recorded = advanced::the_recorded_session();
    let on_at = advanced::when_the_option_went_on(&recorded)
        .expect("the recorded session turns the option on");
    let attacks = advanced::the_recorded_swings(&recorded);
    assert!(!attacks.is_empty(), "the recorded session carries swings");

    let (before, after): (Vec<_>, Vec<_>) = attacks.iter().partition(|(t, _)| *t < on_at);
    let both_halves = !before.is_empty() && !after.is_empty();

    // The gauge offers seven positions; a hair of tolerance, because the value crosses the wire
    // as a float and the client's own is the product of an integer step.
    let on_a_notch = |p: f32| (0u8..=6).any(|k| (p - f32::from(k) / 6.0).abs() < 1e-3);
    let off_the_notches: Vec<(f64, f32)> = before
        .iter()
        .filter(|(_, p)| !on_a_notch(*p))
        .copied()
        .collect();
    // The ones that are off the notches are the documented pairs: the release sends twice at one
    // instant, once at the power it was let go at and once capped.
    let every_odd_one_is_a_pair = off_the_notches
        .iter()
        .all(|(t, _)| before.iter().filter(|(u, _)| (u - t).abs() < 1e-6).count() == 2);
    let most_are_notches = off_the_notches.len() < before.len();

    let none_after_is = after.iter().all(|(_, p)| !on_a_notch(*p));
    let each_is_a_partial_hold = after.iter().all(|(_, p)| *p > 0.0 && *p < 1.0);
    // ...which is what a release before the bar fills looks like, because the bar is a second.
    let a_second = (dereth_client_model::combat::POWER_BAR_SECONDS - 1.0).abs() < f64::EPSILON;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.advanced.the-recorded-attacks-are-charges-after-the-toggle-and-notches-before",
        move |_| {
            both_halves
                && most_are_notches
                && every_odd_one_is_a_pair
                && none_after_is
                && each_is_a_partial_hold
                && a_second
        },
    );
}

#[test]
fn scenario_the_advanced_combat_option_suppresses_the_classic_combat_window() {
    scenario("the_advanced_combat_option_suppresses_the_classic_combat_window");
}

#[test]
fn scenario_turning_the_advanced_option_on_while_the_window_is_up_takes_it_down() {
    scenario("turning_the_advanced_option_on_while_the_window_is_up_takes_it_down");
}

#[test]
fn scenario_the_strip_comes_up_while_an_attack_key_is_held_and_goes_on_release() {
    scenario("the_strip_comes_up_while_an_attack_key_is_held_and_goes_on_release");
}

#[test]
fn scenario_a_release_that_reaches_the_shard_holds_the_strip_until_the_answer() {
    scenario("a_release_that_reaches_the_shard_holds_the_strip_until_the_answer");
}

#[test]
fn scenario_the_advanced_option_chooses_which_display_is_live() {
    scenario("the_advanced_option_chooses_which_display_is_live");
}

#[test]
fn scenario_the_advanced_option_is_off_at_login_and_the_toggle_is_one_bit() {
    scenario("the_advanced_option_is_off_at_login_and_the_toggle_is_one_bit");
}

#[test]
fn scenario_the_recorded_attacks_are_charges_after_the_toggle_and_notches_before() {
    scenario("the_recorded_attacks_are_charges_after_the_toggle_and_notches_before");
}

/// The shipped tree with the three pieces of a frame over it, and the recorded session the two
/// wire claims read.
mod advanced {
    use dereth_client::hud::Hud;
    use dereth_client::interaction::Interaction;
    use dereth_client::objects::ObjectStream;
    use dereth_client_model::combat::CombatMode;
    use dereth_client_net::client_session::testing::{Corpus, CorpusBlob, Direction};
    use dereth_primitives::{LocalTime, ObjectId};
    use dereth_ui::{Delivery, ElemHandle, ElementId, Screen as _, UiSystem};
    use dereth_ui_screens::bind::{attr, attr_float};
    use dereth_ui_screens::hud::combat_notice as cn;
    use dereth_ui_screens::hud::powerbar as pb;
    use dereth_ui_screens::screens::gameplay::{window, GamePlayScreen};

    use super::panels;

    /// The recorded session that carries the option going on and off again.
    const A_RECORDED_SESSION: &str = "melee-attack-run";
    /// The bit the option lives in, written as a literal rather than read back through the same
    /// table the client uses, so a wrong mask on either side cannot hide behind the symbol.
    pub const THE_OPTION_BIT: u32 = 0x0000_1000;
    /// The envelope a client's own game action leaves in, and where its sub-opcode sits.
    const A_CLIENT_ACTION: u32 = 0xF7B1;
    const ACTION_SUB_TYPE: usize = 8;
    /// The envelope the shard's ordered events arrive in, and where their sub-opcode sits.
    const AN_ORDERED_EVENT: u32 = 0xF7B0;
    const EVENT_SUB_TYPE: usize = 12;
    /// The message that saves the whole option word, and where that word sits in its body.
    const SAVES_THE_OPTIONS: u32 = 0x01A1;
    const OPTIONS_WORD: usize = 16;
    /// The character's own description.
    const A_DESCRIPTION: u32 = 0x0013;
    /// The swing, and where the power it went out at sits in its body.
    const A_MELEE_SWING: u32 = 0x0008;
    const SWING_POWER: usize = 20;

    const PLAYER: ObjectId = ObjectId(0x5490_0002);
    const MONSTER: ObjectId = ObjectId(0x8490_0777);

    fn dword(b: &[u8], off: usize) -> Option<u32> {
        let s = b.get(off..off + 4)?;
        Some(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }

    pub fn the_recorded_session() -> Corpus {
        Corpus::load(A_RECORDED_SESSION)
            .expect("the recording parses")
            .expect("the decoded corpus carries it")
    }

    pub fn is_a_description(b: &CorpusBlob) -> bool {
        dword(&b.payload, 0) == Some(AN_ORDERED_EVENT)
            && dword(&b.payload, EVENT_SUB_TYPE) == Some(A_DESCRIPTION)
    }

    fn is_an_option_save(b: &CorpusBlob) -> bool {
        b.dir == Direction::ClientToServer
            && dword(&b.payload, 0) == Some(A_CLIENT_ACTION)
            && dword(&b.payload, ACTION_SUB_TYPE) == Some(SAVES_THE_OPTIONS)
    }

    /// The option word of every save the recorded client sent, in recorded order.
    pub fn the_recorded_option_saves(c: &Corpus) -> Vec<u32> {
        c.blobs
            .iter()
            .filter(|b| is_an_option_save(b))
            .filter_map(|b| dword(&b.payload, OPTIONS_WORD))
            .collect()
    }

    /// When the recorded player turned the option on.
    pub fn when_the_option_went_on(c: &Corpus) -> Option<f64> {
        c.blobs
            .iter()
            .filter(|b| is_an_option_save(b))
            .find(|b| dword(&b.payload, OPTIONS_WORD).is_some_and(|w| w & THE_OPTION_BIT != 0))
            .map(seconds)
    }

    /// `(when, the power it went out at)` for every swing the recorded client sent.
    pub fn the_recorded_swings(c: &Corpus) -> Vec<(f64, f32)> {
        c.blobs
            .iter()
            .filter(|b| {
                b.dir == Direction::ClientToServer
                    && dword(&b.payload, 0) == Some(A_CLIENT_ACTION)
                    && dword(&b.payload, ACTION_SUB_TYPE) == Some(A_MELEE_SWING)
            })
            .filter_map(|b| {
                let p = b.payload.get(SWING_POWER..SWING_POWER + 4)?;
                Some((seconds(b), f32::from_le_bytes([p[0], p[1], p[2], p[3]])))
            })
            .collect()
    }

    fn seconds(b: &CorpusBlob) -> f64 {
        std::time::Duration::from_micros(b.t_rel_micros).as_secs_f64()
    }

    pub fn key(action: u32, start: bool) -> dereth_client_runtime::actions::Action {
        dereth_client_runtime::actions::Action {
            id: dereth_input::ActionId(action),
            phase: if start {
                dereth_client_runtime::actions::ActionPhase::Begin
            } else {
                dereth_client_runtime::actions::ActionPhase::End
            },
            extent: 1.0,
            repeats: 1,
        }
    }

    pub struct Bench {
        ui: UiSystem,
        screen: GamePlayScreen,
        hud: Hud,
        objects: ObjectStream,
        inter: Interaction,
        store: std::sync::Arc<dereth_dat::RetailDatStore>,
        serial: u64,
        /// How many element messages the delivery pass handed to the screen: a denominator that
        /// separates "the window did not move" from "no message was ever delivered".
        delivered: u64,
    }

    impl Bench {
        pub fn new() -> Self {
            let (ui, screen) = panels::gameplay();
            let mut objects = ObjectStream::default();
            objects.world = seed_world();
            Self {
                ui,
                screen,
                hud: Hud::new(),
                objects,
                inter: Interaction::new(),
                store: std::sync::Arc::new(dereth_dat::testing::open_store_or_fail()),
                serial: 1,
                delivered: 0,
            }
        }

        /// One frame, in the order the real one runs: the panels, the charge notices, the
        /// delivery pass, then the combat system.
        pub fn frame(&mut self, actions: Vec<dereth_client_runtime::actions::Action>, t: f64) {
            self.ui.now = LocalTime(t);
            let notices = self.objects.world.combat.take_power_bar_notices();
            self.hud
                .drive(&mut self.ui, &mut self.screen, self.serial, &self.objects);
            dereth_client::hud::deliver_power_bar_notices(
                &mut self.ui,
                &mut self.hud.panels,
                notices,
            );
            self.deliver();
            let _ = dereth_client::interaction::use_time(
                &mut self.inter,
                &self.store,
                None,
                &mut self.objects,
                None,
                actions,
                false,
                (800, 600),
                LocalTime(t),
            );
        }

        /// The element-message pass, bounded. It is bounded in the client too: a show or a hide
        /// is only broadcast on a change.
        fn deliver(&mut self) {
            for _ in 0..8 {
                let batch = self.ui.drain_outbox();
                if batch.is_empty() {
                    return;
                }
                for d in batch {
                    if let Delivery::Element { msg, .. } = d {
                        self.delivered += 1;
                        self.screen.on_element_message(
                            &mut dereth_ui::framework::ScreenCx::new(&mut self.ui),
                            &msg,
                        );
                    }
                }
            }
        }

        pub const fn delivered(&self) -> u64 {
            self.delivered
        }

        pub const fn world(&self) -> &dereth_client_model::World {
            &self.objects.world
        }

        fn handle(&mut self, id: ElementId) -> ElemHandle {
            let root = self.screen.root().expect("the gameplay root");
            self.ui
                .get_child_recursive(root, id)
                .unwrap_or_else(|| panic!("{id:?} is in the shipped layout"))
        }

        fn visible(&mut self, id: ElementId) -> bool {
            let h = self.handle(id);
            self.ui.node(h).expect("alive").region.flags.visible
        }

        /// The combat window itself -- what the player sees.
        pub fn combat_window(&mut self) -> bool {
            self.visible(window::COMBAT_PANEL)
        }

        /// Its cluster of controls, asserted separately: a change that reaches one and not the
        /// other is a defect of its own.
        pub fn combat_page(&mut self) -> bool {
            self.visible(cn::COMBAT_UI_PAGE)
        }

        /// Both standalone strips.
        pub fn strip(&mut self) -> Vec<bool> {
            vec![
                self.visible(window::SMART_BOX_POWER_BAR),
                self.visible(window::POWER_BAR),
            ]
        }

        /// The level on each strip's own meter. **`None` is not zero**: the shipped layout
        /// declares none until a notice writes one.
        pub fn strip_levels(&mut self) -> Vec<Option<f32>> {
            [window::SMART_BOX_POWER_BAR, window::POWER_BAR]
                .into_iter()
                .map(|id| {
                    let owner = self.handle(id);
                    let bar = self
                        .ui
                        .get_child_recursive(owner, pb::BAR)
                        .expect("every strip carries its own meter");
                    attr_float(&self.ui, bar, attr::METER_LEVEL)
                })
                .collect()
        }

        /// The combat window's own meter, which is the classic display.
        pub fn window_meter(&mut self) -> Option<f32> {
            let h = self.handle(dereth_ui_screens::hud::combat_window::ACTUAL_POWER);
            attr_float(&self.ui, h, attr::METER_LEVEL)
        }

        pub fn set_mode(&mut self, mode: CombatMode) {
            self.objects
                .world
                .set_combat_mode(
                    &mut dereth_client_model::NullRequests,
                    &mut dereth_client_model::NullSink,
                    mode,
                    false,
                    true,
                    false,
                )
                .expect("the shard's own form takes no ready check");
        }

        /// Turn the advanced interface on or off, as the options page does. Nothing else is
        /// touched: the point is that the option alone changes what the UI does.
        pub fn set_advanced(&mut self, on: bool) {
            self.objects.world.player_system.options.set(
                dereth_client_model::player::options::option::ADVANCED_COMBAT_UI,
                on,
            );
        }

        pub fn set_auto_repeat(&mut self, on: bool) {
            self.objects.world.player_system.options.set(
                dereth_client_model::player::options::option::AUTO_REPEAT_ATTACK,
                on,
            );
        }

        /// Nothing to swing at, through the client's own setter.
        pub fn nothing_selected(&mut self) {
            self.objects
                .world
                .set_selected_object(None, false, &mut dereth_client_model::NullSink);
        }

        /// The shard's answer to the swing that went out.
        pub fn the_shard_answers_the_swing(&mut self, now: LocalTime) {
            self.objects.world.handle_attack_done(
                &mut dereth_client_model::NullRequests,
                0,
                true,
                now,
            );
        }
    }

    /// A player standing at **peace** with an attackable creature selected and the table the melee
    /// ready arm reads. Peace deliberately: the first thing these scenarios watch is the mode
    /// change that opens the window, and a fixture already in melee would have nothing to show.
    fn seed_world() -> dereth_client_model::World {
        let mut w = dereth_client_model::World::new();
        w.player = Some(PLAYER);
        w.tables
            .weenies
            .insert(PLAYER, dereth_client_model::Weenie::new(PLAYER));
        w.tables
            .weenies
            .get_mut(PLAYER)
            .expect("just inserted")
            .pwd
            .name = "Aldis".into();
        w.tables.inventories.insert(
            PLAYER,
            dereth_client_model::objects::ObjectInventory::new(PLAYER),
        );
        let mut m = dereth_client_model::Weenie::new(MONSTER);
        m.pwd.name = "Mosswart".into();
        m.pwd.obj_type = dereth_client_model::weenie::item_type::CREATURE;
        m.pwd.bitfield |= dereth_client_model::weenie::bitfield::ATTACKABLE;
        w.tables.weenies.insert(MONSTER, m);
        w.set_selected_object(Some(MONSTER), false, &mut dereth_client_model::NullSink);
        w.combat.combat_mode = CombatMode::NonCombat;
        w.tables
            .weenies
            .get_mut(PLAYER)
            .expect("the player")
            .qualities
            .get_or_insert_with(dereth_client_model::Qualities::new)
            .set(
                dereth_client_model::qualities::StatKey::new(
                    dereth_client_model::qualities::StatType::Did,
                    dereth_client_model::combat::COMBAT_TABLE_DID,
                ),
                dereth_client_model::qualities::StatValue::Did(dereth_primitives::DataId(
                    0x3000_0000,
                )),
            );
        assert!(
            w.player_in_ready_position(true, Some(false)),
            "the body must be able to attack at all, or every reading is a refusal it did not \
             intend"
        );
        w
    }
}

// ---------------------------------------------------------------------------------------------
// The combat keys, the gauge and the combat window's own controls
//
// The claims about the casting bar are `magic`'s subject and are in `magic.rs`.
//
// **The calibration has no row of its own** and stands as a premise instead: that the driver can
// produce an action at all and that the arm can move the world. Without it every silence below
// would be indistinguishable from a broken harness.
//
// Every control is discovered from the shipped keymap: nothing here writes a key down.
// ---------------------------------------------------------------------------------------------

pub fn one_set_of_keys_means_three_different_things() {
    use dereth_input::combat::{mode, MAGIC_COMBAT_MAP, MELEE_COMBAT_MAP, MISSILE_COMBAT_MAP};

    let mut shell = maps::shell();
    let shared = keys::shared_controls(&shell);
    let mut d = maps::Driver::new();

    let mut by_mode: Vec<Vec<u32>> = Vec::new();
    for m in [mode::MELEE, mode::MISSILE, mode::MAGIC] {
        shell.set_combat_input_maps(m);
        let mut actions = Vec::new();
        for qc in &shared {
            let (down, _) = d.press_release(&mut shell, qc);
            assert_eq!(
                down.len(),
                1,
                "a shared key resolves to one action in every mode"
            );
            actions.push(down[0].id.0);
        }
        by_mode.push(actions);
    }

    // The three sets are **disjoint**: no meaning is reachable in more than one mode, which is
    // what says the mode selects the meaning rather than one map winning always.
    let disjoint = by_mode[0].iter().all(|a| !by_mode[1].contains(a))
        && by_mode[0].iter().all(|a| !by_mode[2].contains(a))
        && by_mode[1].iter().all(|a| !by_mode[2].contains(a));

    // ...and each set is the one that mode's own section binds.
    let is_the_sections = [MELEE_COMBAT_MAP, MISSILE_COMBAT_MAP, MAGIC_COMBAT_MAP]
        .iter()
        .zip(by_mode.iter())
        .all(|(map, actions)| {
            let section: std::collections::BTreeSet<u32> = maps::shipped_bindings(&shell, map.0)
                .iter()
                .map(|(a, _)| a.0)
                .collect();
            actions.iter().all(|a| section.contains(a))
        });
    let five = shared.len();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.input-map.one-set-of-keys-means-three-different-things-and-the-mode-picks-which",
        move |_| five > 0 && by_mode.iter().all(|m| m.len() == five) && disjoint && is_the_sections,
    );
}

pub fn every_height_key_moves_the_height_in_both_modes() {
    use dereth_client::interaction::action as ia;
    use dereth_client_model::combat::{AttackHeight, CombatMode};

    // The calibration: the driver produces an action and the arm moves the world.
    let calibrated = keys::the_driver_and_the_arm_both_work();

    let mut every_mode_holds = true;
    let mut every_swing_carries_the_set_height = true;
    for (mode, imap, keys_of) in [
        (
            CombatMode::Melee,
            dereth_input::combat::MELEE_COMBAT_MAP,
            [
                ia::COMBAT_LOW_ATTACK,
                ia::COMBAT_HIGH_ATTACK,
                ia::COMBAT_MEDIUM_ATTACK,
            ],
        ),
        (
            CombatMode::Missile,
            dereth_input::combat::MISSILE_COMBAT_MAP,
            [
                ia::COMBAT_AIM_LOW,
                ia::COMBAT_AIM_HIGH,
                ia::COMBAT_AIM_MEDIUM,
            ],
        ),
    ] {
        let mut shell = maps::shell();
        shell.set_combat_input_maps(mode.raw());
        let mut d = maps::Driver::new();
        let mut bench = keys::Bench::new(mode);
        let mut now = 100.0;

        every_mode_holds &= bench.world().combat.requested_attack_height == AttackHeight::Medium;

        for (want_action, want_height) in [
            (keys_of[0], AttackHeight::Low),
            (keys_of[1], AttackHeight::High),
            (keys_of[2], AttackHeight::Medium),
        ] {
            let qc = maps::the_shipped_control(&shell, imap.0, want_action);
            let (down, up) = d.press_release(&mut shell, &qc);
            every_mode_holds &= down.iter().map(|e| e.id.0).eq([want_action]);
            now += 1.0;
            bench.drive(down, now);
            // Both halves: a handler that only writes the field looks identical from the field.
            every_mode_holds &= bench.world().combat.requested_attack_height == want_height
                && bench.world().combat.attack_request_in_progress
                && bench.world().combat.build_in_progress;
            now += 1.0;
            bench.drive(up, now);
            every_mode_holds &= !bench.world().combat.attack_request_in_progress
                // The release reads the height; it does not write it.
                && bench.world().combat.requested_attack_height == want_height;
        }
        every_mode_holds &= bench.height_changes() >= 3 && bench.attacks_released() >= 1;

        // **The asymmetric case**: press the low key and release the high one. The swing goes out
        // at the height that is set; releasing the same key cannot tell the two readings apart.
        let low = maps::the_shipped_control(&shell, imap.0, keys_of[0]);
        let (low_down, _) = d.press_release(&mut shell, &low);
        let mut fresh = keys::Bench::new(mode);
        now += 1.0;
        fresh.drive(low_down, now);
        every_mode_holds &= fresh.world().combat.requested_attack_height == AttackHeight::Low;
        now += 1.0;
        fresh.drive(vec![keys::released_in(keys_of[1], imap)], now);
        let swung = fresh.what_would_go_out();
        every_swing_carries_the_set_height &= !swung.is_empty()
            && swung
                .iter()
                .filter_map(keys::height_of)
                .all(|h| h == AttackHeight::Low as u32);
    }

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.attack.every-height-key-moves-the-height-in-both-modes-and-the-swing-carries-it",
        move |_| calibrated && every_mode_holds && every_swing_carries_the_set_height,
    );
}

pub fn the_gauge_keys_step_by_a_sixth_in_both_modes() {
    use dereth_client::interaction::action as ia;
    use dereth_client_model::combat::CombatMode;

    let sixth = 1.0_f32 / 6.0;
    let mut both_modes_hold = true;
    for (mode, imap, down_a, up_a) in [
        (
            CombatMode::Melee,
            dereth_input::combat::MELEE_COMBAT_MAP,
            ia::COMBAT_DECREASE_ATTACK_POWER,
            ia::COMBAT_INCREASE_ATTACK_POWER,
        ),
        (
            CombatMode::Missile,
            dereth_input::combat::MISSILE_COMBAT_MAP,
            ia::COMBAT_DECREASE_MISSILE_ACCURACY,
            ia::COMBAT_INCREASE_MISSILE_ACCURACY,
        ),
    ] {
        let mut shell = maps::shell();
        shell.set_combat_input_maps(mode.raw());
        let mut d = maps::Driver::new();
        let mut bench = keys::Bench::new(mode);
        let mut now = 100.0;
        both_modes_hold &= (bench.world().combat.ui_requested_power - 0.5).abs() < 1e-6;

        let mut step = |a: u32,
                        bench: &mut keys::Bench,
                        shell: &mut dereth_client::input::InputShell,
                        d: &mut maps::Driver,
                        now: &mut f64| {
            let qc = maps::the_shipped_control(shell, imap.0, a);
            let (down, _) = d.press_release(shell, &qc);
            assert!(
                down.iter().map(|e| e.id.0).eq([a]),
                "the key resolves to its own action"
            );
            *now += 1.0;
            bench.drive(down, *now);
            bench.world().combat.ui_requested_power
        };

        // Up from the half: one notch at a time, then it stops at full.
        let mut up = vec![0.5_f32];
        for _ in 0..5 {
            up.push(step(up_a, &mut bench, &mut shell, &mut d, &mut now));
        }
        both_modes_hold &= (up[1] - 4.0 * sixth).abs() < 1e-6
            && (up[2] - 5.0 * sixth).abs() < 1e-6
            && (up[3] - 1.0).abs() < 1e-6
            && (up[4] - 1.0).abs() < 1e-6
            && (up[5] - 1.0).abs() < 1e-6;

        // ...and all the way down, stopping at empty.
        let mut down = Vec::new();
        for _ in 0..8 {
            down.push(step(down_a, &mut bench, &mut shell, &mut d, &mut now));
        }
        let want: Vec<f32> = [5.0_f32, 4.0, 3.0, 2.0, 1.0, 0.0, 0.0, 0.0]
            .into_iter()
            .map(|n| (n * sixth).min(1.0))
            .collect();
        both_modes_hold &= down
            .iter()
            .zip(want.iter())
            .all(|(got, w)| (got - w).abs() < 1e-6);
        // Every press reached the gauge arm, which is what stops a silence reading as a step.
        both_modes_hold &= bench.desired_power_changes() == 13;
    }

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.power-bar.the-gauge-keys-step-by-a-sixth-in-both-modes-and-saturate",
        move |_| both_modes_hold,
    );
}

pub fn a_value_between_notches_rounds_to_the_nearest_before_stepping() {
    let mut g = dereth_client_model::combat::CombatState::begin();
    let six = dereth_client_model::combat::POWER_NOTCHES == 6;

    // Between notch 3 and notch 4: stepping up lands on 5/6 and not on a sixth further along.
    g.ui_requested_power = 0.60;
    let up = (g.adjust_ui_requested_power(true) - 5.0 / 6.0).abs() < 1e-6;
    g.ui_requested_power = 0.60;
    let down = (g.adjust_ui_requested_power(false) - 3.0 / 6.0).abs() < 1e-6;

    // The stops are applied after the scale, which is what makes empty and full the ends.
    g.ui_requested_power = 0.0;
    let at_the_bottom = g.adjust_ui_requested_power(false).abs() < 1e-9;
    g.ui_requested_power = 1.0;
    let at_the_top = (g.adjust_ui_requested_power(true) - 1.0).abs() < 1e-9;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.power-bar.a-value-between-notches-rounds-to-the-nearest-before-stepping",
        move |_| six && up && down && at_the_bottom && at_the_top,
    );
}

pub fn holding_a_height_key_refuses_once_and_not_once_per_repeat() {
    use dereth_client_model::combat::{AttackHeight, CombatMode};
    use dereth_primitives::LocalTime;

    // Nothing to swing at, so the refusal arm is the one that runs.
    let mut w = keys::a_world(CombatMode::Melee);
    w.set_selected_object(None, false, &mut dereth_client_model::NullSink);
    let nothing_to_hit = w.get_attack_target().is_none();

    let t = LocalTime(100.0);
    let first_refuses = w
        .set_requested_attack_height(AttackHeight::High, true, t)
        .is_err();
    // Ten repeats: nothing has latched, because the refusal arm never latches it, so every one
    // refuses. That is the discriminating case for the second half of the guard.
    let mut refusals = 0;
    for i in 0..10 {
        if w.set_requested_attack_height(AttackHeight::High, true, LocalTime(100.0 + f64::from(i)))
            .is_err()
        {
            refusals += 1;
        }
    }
    let every_one_refuses = refusals == 10;

    // Now with something to hit: the first press latches, and every later repeat of the same
    // height is swallowed.
    let mut w = keys::a_world(CombatMode::Melee);
    w.set_requested_attack_height(AttackHeight::High, true, t)
        .expect("a real target");
    let latched = w.combat.attack_request_in_progress;
    let start = w.combat.build_start_time;
    // Something the whole arm would overwrite if it ran.
    w.combat.requested_attack_power = 0.25;
    for i in 1..=10 {
        w.set_requested_attack_height(AttackHeight::High, true, LocalTime(100.0 + f64::from(i)))
            .expect("still valid");
    }
    let swallowed = (w.combat.requested_attack_power - 0.25).abs() < 1e-9
        && (w.combat.build_start_time - start).abs() < 1e-9;

    // A different height runs the whole thing again -- and the charge already running is still
    // not restarted, which is a different guard and is named rather than conflated.
    w.set_requested_attack_height(AttackHeight::Low, true, LocalTime(111.0))
        .expect("valid");
    let ran_again = (w.combat.requested_attack_power - 1.0).abs() < 1e-9
        && (w.combat.build_start_time - start).abs() < 1e-9;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.attack.holding-a-height-key-refuses-once-and-not-once-per-repeat",
        move |_| {
            nothing_to_hit
                && first_refuses
                && every_one_refuses
                && latched
                && swallowed
                && ran_again
        },
    );
}

pub fn at_peace_no_combat_key_reaches_an_arm() {
    use dereth_client::interaction::action as ia;
    use dereth_client_model::combat::{AttackHeight, CombatMode};

    let mut shell = maps::shell();
    shell.set_combat_input_maps(dereth_input::combat::mode::MELEE);
    let mut d = maps::Driver::new();
    let qc = maps::the_shipped_control(
        &shell,
        dereth_input::combat::MELEE_COMBAT_MAP.0,
        ia::COMBAT_HIGH_ATTACK,
    );
    let (down, _) = d.press_release(&mut shell, &qc);
    let the_event_exists = down.len() == 1;

    let mut readings = Vec::new();
    for mode in [CombatMode::NonCombat, CombatMode::Melee] {
        let mut bench = keys::Bench::new(mode);
        bench.drive(down.clone(), 100.0);
        readings.push((
            bench.world().combat.requested_attack_height == AttackHeight::High,
            bench.height_changes(),
        ));
    }

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.mode.at-peace-no-combat-key-reaches-an-arm",
        move |_| the_event_exists && readings == vec![(false, 0), (true, 1)],
    );
}

pub fn every_control_in_the_combat_window_does_what_the_client_does() {
    use dereth_client_model::combat::{AttackHeight, CombatMode};
    use dereth_ui_screens::hud::combat_window as cw;
    use dereth_ui_screens::view::{PlayerOption, UiRequest};

    let mut w = keys::WindowBench::new(CombatMode::Melee);
    let bound = w.the_window_is_bound();

    // The three attack-height buttons: a press sets the height and starts the charge, and the
    // click that follows releases it.
    let mut every_button_holds = true;
    for (id, want) in [
        (cw::ATTACK_HEIGHT_BUTTONS[0].0, AttackHeight::High),
        (cw::ATTACK_HEIGHT_BUTTONS[2].0, AttackHeight::Low),
        (cw::ATTACK_HEIGHT_BUTTONS[1].0, AttackHeight::Medium),
    ] {
        let asked = w.press(id, dereth_ui::msg::element::id::MOUSE_PRESS, 0);
        every_button_holds &= asked
            == vec![UiRequest::CombatSetAttackHeight {
                height: want as u32,
            }];
        w.run(asked, 200.0);
        every_button_holds &=
            w.world().combat.requested_attack_height == want && w.world().combat.build_in_progress;

        let asked = w.press(id, dereth_ui::msg::element::id::BUTTON_CLICKED, 0);
        every_button_holds &= asked
            == vec![UiRequest::CombatEndAttack {
                height: want as u32,
            }];
        w.run(asked, 201.0);
        every_button_holds &= !w.world().combat.attack_request_in_progress;
    }
    let swung = w.attacks_released() >= 1;

    // The gauge: thousandths, clamped.
    let mut every_drag_holds = true;
    for (position, want) in [(0_u32, 0.0_f32), (250, 0.25), (1000, 1.0), (4000, 1.0)] {
        let asked = w.press(
            cw::DESIRED_POWER,
            dereth_ui::msg::element::id::SCROLL_POSITION,
            position,
        );
        every_drag_holds &= asked == vec![UiRequest::CombatSetDesiredPower { position }];
        w.run(asked, 202.0);
        every_drag_holds &= (w.world().combat.ui_requested_power - want).abs() < 1e-6;
    }
    // ...and a drag on something outside the window does not move it.
    let outside = w.drag_something_else(900);
    let declined = !outside
        .iter()
        .any(|r| matches!(r, UiRequest::CombatSetDesiredPower { .. }));

    // The three option boxes: each names its own option and its new value.
    let mut every_box_holds = true;
    for (i, option) in [
        PlayerOption::AutoRepeatAttack,
        PlayerOption::AutoTarget,
        PlayerOption::ViewCombatTarget,
    ]
    .into_iter()
    .enumerate()
    {
        let asked = w.tick_the_option_box(i);
        every_box_holds &= asked == vec![UiRequest::SetPlayerOption(option, true)];
    }

    // And the bit reaches the character's options and is queued for the shard. The one whose
    // default is off, so that setting it is a change and not a no-op.
    let ordinal = dereth_client::hud::option_ordinal(PlayerOption::ViewCombatTarget);
    let starts_off = !w.world().player_system.options.get(ordinal);
    let queued_before = w.option_changes_queued();
    w.run(
        vec![UiRequest::SetPlayerOption(
            PlayerOption::ViewCombatTarget,
            true,
        )],
        203.0,
    );
    let reached =
        w.world().player_system.options.get(ordinal) && w.option_changes_queued() > queued_before;
    // The two things the option is supposed to make the game do are counted as not yet done,
    // which is how a reader can see they were decided and not applied.
    let counted = w.side_effects_unapplied() > 0;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.window.every-control-does-what-the-client-does",
        move |_| {
            bound
                && every_button_holds
                && swung
                && every_drag_holds
                && declined
                && every_box_holds
                && starts_off
                && reached
                && counted
        },
    );
}

pub fn the_combat_window_reflects_the_height_the_power_and_the_notch() {
    use dereth_ui_screens::bind::{attr, attr_enum, attr_float};
    use dereth_ui_screens::hud::combat_window as cw;
    use dereth_ui_screens::hud::powerbar::PowerBarMode;

    let (mut ui, screen) = panels::gameplay();
    let root = screen.root().expect("the gameplay root");
    let mut p = dereth_ui_screens::panels::remaining::RemainingPanels::default();
    p.post_init(&mut ui, root);
    let w = &mut p.combat_window;
    let group = w.height_group.expect("the shipped height group");

    // Two stations for the height, because a handler that writes one state and never changes it
    // is otherwise green.
    let low = w.on_attack_height_changed(&mut ui, cw::height::LOW)
        && attr_enum(&ui, group, attr::MEDIA_STATE) == Some(cw::ATTACK_HEIGHT_BUTTONS[2].0 .0);
    let high = w.on_attack_height_changed(&mut ui, cw::height::HIGH)
        && attr_enum(&ui, group, attr::MEDIA_STATE) == Some(cw::ATTACK_HEIGHT_BUTTONS[0].0 .0);
    let again = !w.on_attack_height_changed(&mut ui, cw::height::HIGH);
    let no_height = !w.on_attack_height_changed(&mut ui, cw::height::UNDEF)
        && attr_enum(&ui, group, attr::MEDIA_STATE) == Some(cw::ATTACK_HEIGHT_BUTTONS[0].0 .0);

    // The meter takes the charge that belongs to it and refuses one that does not.
    let meter = w.actual_power.expect("the shipped meter");
    let refused = !w.on_set_powerbar_level(&mut ui, PowerBarMode::Jump, 0.9);
    let taken = w.on_set_powerbar_level(&mut ui, PowerBarMode::Combat, 0.4)
        && attr_float(&ui, meter, attr::METER_LEVEL).is_some_and(|v| (v - 0.4).abs() < 1e-6);

    // The gauge's own mark, which is a different attribute on a different element.
    let sb = w.desired_power.expect("the shipped gauge");
    let notch = w.on_desired_attack_power_changed(&mut ui, 0.5)
        && attr_float(&ui, sb, attr::MARKER).is_some_and(|v| (v - 0.5).abs() < 1e-6);
    let notch_again = !w.on_desired_attack_power_changed(&mut ui, 0.5);

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.window.it-reflects-the-height-the-power-and-the-notch",
        move |_| low && high && again && no_height && refused && taken && notch && notch_again,
    );
}

pub fn the_frames_own_pass_drives_the_combat_windows_read_backs() {
    use dereth_ui_screens::bind::{attr, attr_enum, attr_float};
    use dereth_ui_screens::hud::combat_window as cw;
    use dereth_ui_screens::view::CombatBar;

    let (mut ui, screen) = panels::gameplay();
    let root = screen.root().expect("the gameplay root");
    let mut p = dereth_ui_screens::panels::remaining::RemainingPanels::default();
    p.post_init(&mut ui, root);
    let group = p
        .combat_window
        .height_group
        .expect("the shipped height group");

    // A fresh character: the middle height, no charge, the gauge at its half.
    let a = keys::StubView {
        bar: Some(CombatBar::default()),
        ..keys::StubView::default()
    };
    let first = p.update(&mut ui, &a) > 0
        && attr_enum(&ui, group, attr::MEDIA_STATE) == Some(cw::ATTACK_HEIGHT_BUTTONS[1].0 .0);

    // Part way through a charge at the low height, with the gauge near the top.
    let b = keys::StubView {
        bar: Some(CombatBar {
            requested_attack_height: cw::height::LOW,
            power_bar_mode: 1,
            level: 0.5,
            desired_power: 5.0 / 6.0,
        }),
        ..keys::StubView::default()
    };
    let wrote = p.update(&mut ui, &b)
        + dereth_client::hud::deliver_power_bar_notices(
            &mut ui,
            &mut p,
            [dereth_client_model::combat::PowerBarNotice::SetLevel {
                mode: dereth_client_model::combat::PowerBarMode::Combat,
                level: 0.5,
            }],
        );
    let meter = p.combat_window.actual_power.expect("the shipped meter");
    let sb = p.combat_window.desired_power.expect("the shipped gauge");
    let second = wrote == 3
        && attr_enum(&ui, group, attr::MEDIA_STATE) == Some(cw::ATTACK_HEIGHT_BUTTONS[2].0 .0)
        && attr_float(&ui, meter, attr::METER_LEVEL).is_some_and(|v| (v - 0.5).abs() < 1e-6)
        && attr_float(&ui, sb, attr::MARKER).is_some_and(|v| (v - 5.0 / 6.0).abs() < 1e-6);

    // The same frame again writes nothing.
    let idle = p.update(&mut ui, &b) == 0;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.window.the-frames-own-pass-drives-the-read-backs",
        move |_| first && second && idle,
    );
}

pub fn the_recklessness_meter_appears_at_exactly_the_trained_class() {
    use dereth_ui_screens::hud::combat_window as cw;

    let (mut ui, screen) = panels::gameplay();
    let root = screen.root().expect("the gameplay root");
    let mut p = dereth_ui_screens::panels::remaining::RemainingPanels::default();
    p.post_init(&mut ui, root);
    let field = p
        .combat_window
        .recklessness_field
        .expect("the shipped meter");
    let sb = ui
        .get_child_recursive(root, cw::DESIRED_POWER)
        .expect("the shipped gauge");

    let mut every_class_holds = true;
    for (class, want) in [(0_u32, false), (1, false), (2, true), (3, true)] {
        let view = keys::StubView {
            recklessness: class,
            ..keys::StubView::default()
        };
        let mut m = keys::element_message(
            sb,
            cw::DESIRED_POWER,
            dereth_ui::msg::element::id::SCROLL_POSITION,
        );
        m.p1 = 500;
        ui.requests.clear();
        every_class_holds &= p.on_element_message(&mut ui, &m, &view)
            && ui.node(field).expect("alive").region.flags.visible == want;
    }
    let trained_is_two = cw::RECKLESSNESS_TRAINED == 2;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.window.the-recklessness-meter-appears-at-exactly-the-trained-class",
        move |_| every_class_holds && trained_is_two,
    );
}

#[test]
fn scenario_one_set_of_keys_means_three_different_things() {
    scenario("one_set_of_keys_means_three_different_things");
}

#[test]
fn scenario_every_height_key_moves_the_height_in_both_modes() {
    scenario("every_height_key_moves_the_height_in_both_modes");
}

#[test]
fn scenario_the_gauge_keys_step_by_a_sixth_in_both_modes() {
    scenario("the_gauge_keys_step_by_a_sixth_in_both_modes");
}

#[test]
fn scenario_a_value_between_notches_rounds_to_the_nearest_before_stepping() {
    scenario("a_value_between_notches_rounds_to_the_nearest_before_stepping");
}

#[test]
fn scenario_holding_a_height_key_refuses_once_and_not_once_per_repeat() {
    scenario("holding_a_height_key_refuses_once_and_not_once_per_repeat");
}

#[test]
fn scenario_at_peace_no_combat_key_reaches_an_arm() {
    scenario("at_peace_no_combat_key_reaches_an_arm");
}

#[test]
fn scenario_every_control_in_the_combat_window_does_what_the_client_does() {
    scenario("every_control_in_the_combat_window_does_what_the_client_does");
}

#[test]
fn scenario_the_combat_window_reflects_the_height_the_power_and_the_notch() {
    scenario("the_combat_window_reflects_the_height_the_power_and_the_notch");
}

#[test]
fn scenario_the_frames_own_pass_drives_the_combat_windows_read_backs() {
    scenario("the_frames_own_pass_drives_the_combat_windows_read_backs");
}

#[test]
fn scenario_the_recklessness_meter_appears_at_exactly_the_trained_class() {
    scenario("the_recklessness_meter_appears_at_exactly_the_trained_class");
}

/// One key press into the client's own interaction slot, and the combat window's own controls.
pub mod keys {
    use std::collections::BTreeMap;

    use dereth_client::input::InputShell;
    use dereth_client::interaction::Interaction;
    use dereth_client_model::combat::CombatMode;
    use dereth_input::spec::ControlChord;
    use dereth_input::{ActionId, InputMapId};
    use dereth_primitives::{DataId, LocalTime, ObjectId, ServerTime};
    use dereth_ui::{ElementId, MessageId, UiSystem};
    use dereth_ui_screens::panels::remaining::RemainingPanels;
    use dereth_ui_screens::view::{CombatBar, GameView, PlayerOption, SpellEntry, UiRequest};

    use super::{maps, panels};

    const PLAYER: ObjectId = ObjectId(0x5410_0002);
    const MONSTER: ObjectId = ObjectId(0x8410_0777);

    /// The controls all three combat sections bind, discovered from the shipped keymap.
    pub fn shared_controls(shell: &InputShell) -> Vec<ControlChord> {
        let section = |m: InputMapId| {
            shell
                .manager
                .keymap
                .section(m)
                .map(|s| s.bindings().to_vec())
                .unwrap_or_else(|| panic!("{m:?} has a shipped section"))
        };
        let melee = section(dereth_input::combat::MELEE_COMBAT_MAP);
        let missile = section(dereth_input::combat::MISSILE_COMBAT_MAP);
        let magic = section(dereth_input::combat::MAGIC_COMBAT_MAP);
        let mut out: Vec<ControlChord> = melee
            .iter()
            .filter(|(qa, _)| {
                missile.iter().any(|(qb, _)| qa.is_conflicting(qb))
                    && magic.iter().any(|(qb, _)| qa.is_conflicting(qb))
            })
            .map(|(q, _)| *q)
            .collect();
        out.sort_by_key(|q| q.control.offset());
        out.dedup_by_key(|q| q.control.offset());
        assert!(
            !out.is_empty(),
            "the three shipped sections share something"
        );
        out
    }

    /// **The calibration**, which every silence below rests on: the driver can produce an action
    /// and the arm can move the world. The known positive is the key that toggles combat itself.
    pub fn the_driver_and_the_arm_both_work() -> bool {
        use dereth_client::interaction::action as ia;

        let mut shell = maps::shell();
        let mut d = maps::Driver::new();
        let qc = maps::the_shipped_control(
            &shell,
            dereth_input::combat::COMBAT_MAP.0,
            ia::COMBAT_TOGGLE_COMBAT,
        );
        let (down, _) = d.press_release(&mut shell, &qc);
        if !down.iter().map(|e| e.id.0).eq([ia::COMBAT_TOGGLE_COMBAT]) {
            return false;
        }
        // ...and it reaches the world. This bench has no body, so the client queues the change
        // rather than taking it -- which names the branch the action reached, and is a stronger
        // reading than the mode simply moving.
        let mut bench = Bench::new(CombatMode::NonCombat);
        bench.drive(down, 100.0);
        bench.world().combat.pending_combat_mode == CombatMode::Melee
            && bench.world().combat.combat_mode == CombatMode::NonCombat
    }

    /// A player in `mode` with something attackable selected and the table the ready arm reads.
    pub fn a_world(mode: CombatMode) -> dereth_client_model::World {
        let mut w = dereth_client_model::World::new();
        w.player = Some(PLAYER);
        w.tables
            .weenies
            .insert(PLAYER, dereth_client_model::Weenie::new(PLAYER));
        w.tables
            .weenies
            .get_mut(PLAYER)
            .expect("just inserted")
            .pwd
            .name = "Aldis".into();
        w.tables.inventories.insert(
            PLAYER,
            dereth_client_model::objects::ObjectInventory::new(PLAYER),
        );
        let mut m = dereth_client_model::Weenie::new(MONSTER);
        m.pwd.name = "Mosswart".into();
        m.pwd.obj_type = dereth_client_model::weenie::item_type::CREATURE;
        m.pwd.bitfield |= dereth_client_model::weenie::bitfield::ATTACKABLE;
        w.tables.weenies.insert(MONSTER, m);
        w.set_selected_object(Some(MONSTER), false, &mut dereth_client_model::NullSink);
        w.combat.combat_mode = mode;
        w.tables
            .weenies
            .get_mut(PLAYER)
            .expect("the player")
            .qualities
            .get_or_insert_with(dereth_client_model::Qualities::new)
            .set(
                dereth_client_model::qualities::StatKey::new(
                    dereth_client_model::qualities::StatType::Did,
                    dereth_client_model::combat::COMBAT_TABLE_DID,
                ),
                dereth_client_model::qualities::StatValue::Did(DataId(0x3000_0000)),
            );
        if mode == CombatMode::Missile {
            w.combat.current_style = dereth_client_model::combat::MISSILE_READY_STYLES[0];
        }
        assert!(
            w.player_in_ready_position(true, Some(false)),
            "the body must be able to attack at all, or every reading is a refusal it did not \
             intend"
        );
        w
    }

    /// The release event the input layer would build. The shipped combat bindings are one-shots
    /// and produce none, so the arm is asked the question directly with the event it would have
    /// been handed.
    pub fn released_in(
        action: u32,
        _input_map: InputMapId,
    ) -> dereth_client_runtime::actions::Action {
        dereth_client_runtime::actions::Action {
            id: ActionId(action),
            phase: dereth_client_runtime::actions::ActionPhase::End,
            extent: 1.0,
            repeats: 1,
        }
    }

    /// The height a swing carries, whichever kind of swing it is.
    pub fn height_of(r: &dereth_client_model::Request) -> Option<u32> {
        match r {
            dereth_client_model::Request::TargetedMeleeAttack(m) => Some(m.attack_height),
            dereth_client_model::Request::TargetedMissileAttack(m) => Some(m.attack_height),
            _ => None,
        }
    }

    /// The client's own interaction slot over a world, with no scene and no link: the requests are
    /// counted, never sent.
    pub struct Bench {
        inter: Interaction,
        objects: dereth_client::objects::ObjectStream,
        store: std::sync::Arc<dereth_dat::RetailDatStore>,
    }

    impl Bench {
        pub fn new(mode: CombatMode) -> Self {
            let mut objects = dereth_client::objects::ObjectStream::default();
            objects.world = a_world(mode);
            Self {
                inter: Interaction::new(),
                objects,
                store: std::sync::Arc::new(dereth_dat::testing::open_store_or_fail()),
            }
        }

        pub fn drive(&mut self, actions: Vec<dereth_client_runtime::actions::Action>, now: f64) {
            let _ = dereth_client::interaction::use_time(
                &mut self.inter,
                &self.store,
                None,
                &mut self.objects,
                None,
                actions,
                false,
                (800, 600),
                LocalTime(now),
            );
        }

        pub const fn world(&self) -> &dereth_client_model::World {
            &self.objects.world
        }

        pub const fn height_changes(&self) -> u64 {
            self.inter.stats.attack_height_changes
        }

        pub const fn attacks_released(&self) -> u64 {
            self.inter.stats.attacks_released
        }

        pub const fn desired_power_changes(&self) -> u64 {
            self.inter.stats.desired_power_changes
        }

        /// What a frame with no link leaves behind: the requests that would have gone out.
        pub fn what_would_go_out(&self) -> Vec<dereth_client_model::Request> {
            self.inter.last_sent.clone()
        }
    }

    /// One element message, as the shipped tree delivers one.
    pub fn element_message(
        source: dereth_ui::ElemHandle,
        source_id: ElementId,
        id: MessageId,
    ) -> dereth_ui::ElementMessage {
        dereth_ui::ElementMessage {
            source_id,
            source,
            id,
            p1: 0,
            p2: 0,
            point: dereth_ui::msg::element::MessagePoint::default(),
            serial: 0,
        }
    }

    /// The combat window on the shipped tree, with the client's own request seam under it.
    pub struct WindowBench {
        ui: UiSystem,
        root: dereth_ui::ElemHandle,
        p: RemainingPanels,
        view: StubView,
        inter: Interaction,
        objects: dereth_client::objects::ObjectStream,
    }

    impl WindowBench {
        pub fn new(mode: CombatMode) -> Self {
            let (mut ui, screen) = panels::gameplay();
            let root = screen.root().expect("the gameplay root");
            let mut p = RemainingPanels::default();
            p.post_init(&mut ui, root);
            let mut objects = dereth_client::objects::ObjectStream::default();
            objects.world = a_world(mode);
            Self {
                ui,
                root,
                p,
                view: StubView::default(),
                inter: Interaction::new(),
                objects,
            }
        }

        /// The window bound off the shipped tree, with everything it looks up found.
        pub fn the_window_is_bound(&self) -> bool {
            let w = &self.p.combat_window;
            w.bound()
                && w.failures == 0
                && w.height_buttons.len() == 3
                && w.options.len() == 3
                && w.options.iter().map(|o| o.option).eq([
                    PlayerOption::AutoRepeatAttack,
                    PlayerOption::AutoTarget,
                    PlayerOption::ViewCombatTarget,
                ])
                && w.desired_power.is_some()
                && w.actual_power.is_some()
                && w.recklessness_field.is_some_and(|h| {
                    // Binding hides it, which is what the boundary scenario measures against.
                    !self.ui.node(h).expect("alive").region.flags.visible
                })
        }

        /// One gesture on a control of the window, and what it asked the client for.
        pub fn press(&mut self, id: ElementId, message: MessageId, p1: u32) -> Vec<UiRequest> {
            let h = self
                .ui
                .get_child_recursive(self.root, id)
                .unwrap_or_else(|| panic!("{id:?} is in the shipped tree"));
            let mut m = element_message(h, id, message);
            m.p1 = p1;
            self.ui.requests.clear();
            assert!(
                self.p.on_element_message(&mut self.ui, &m, &self.view),
                "{id:?} is the window's own control and it consumed the gesture"
            );
            self.ui.requests.take()
        }

        /// Tick one of the three option boxes and click it, as a player does: the control carries
        /// its own new value and the arm reads it back off the control.
        pub fn tick_the_option_box(&mut self, i: usize) -> Vec<UiRequest> {
            use dereth_ui_screens::hud::combat_window as cw;
            let element = self.p.combat_window.options[i].element;
            let id = cw::OPTION_CHECKBOXES[i].0;
            self.ui.set_attribute_bool(element, cw::ATTR_CHECKED, true);
            self.ui.requests.clear();
            assert!(
                self.p.on_element_message(
                    &mut self.ui,
                    &element_message(element, id, dereth_ui::msg::element::id::BUTTON_CLICKED),
                    &self.view,
                ),
                "the option box consumed its own click"
            );
            self.ui.requests.take()
        }

        /// A drag on something that is not the window's gauge.
        pub fn drag_something_else(&mut self, position: u32) -> Vec<UiRequest> {
            let id = ElementId(0x1000_0034);
            let Some(h) = self.ui.get_child_recursive(self.root, id) else {
                return Vec::new();
            };
            let mut m = element_message(h, id, dereth_ui::msg::element::id::SCROLL_POSITION);
            m.p1 = position;
            self.ui.requests.clear();
            let _ = self.p.on_element_message(&mut self.ui, &m, &self.view);
            self.ui.requests.take()
        }

        /// Run what a gesture asked for, through the client's own request seam.
        pub fn run(&mut self, requests: Vec<UiRequest>, now: f64) {
            self.inter.queue(Vec::new(), requests);
            self.inter
                .run_ui_requests(&mut self.objects.world, false, ServerTime(now));
        }

        pub const fn world(&self) -> &dereth_client_model::World {
            &self.objects.world
        }

        pub const fn attacks_released(&self) -> u64 {
            self.inter.stats.attacks_released
        }

        pub const fn option_changes_queued(&self) -> u64 {
            self.inter.stats.option_changes_deferred + self.inter.stats.option_changes_unsendable
        }

        pub const fn side_effects_unapplied(&self) -> u64 {
            self.inter.stats.option_side_effects_unapplied
        }
    }

    /// A view that answers only what these scenarios read; everything else takes the trait's own
    /// default, which is what makes those answers the only inputs.
    #[derive(Debug, Default)]
    pub struct StubView {
        pub spells: Vec<u32>,
        pub spell_entries: Vec<SpellEntry>,
        pub options: BTreeMap<u32, bool>,
        pub recklessness: u32,
        pub bar: Option<CombatBar>,
    }

    impl StubView {
        pub fn with_spells(spells: Vec<u32>) -> Self {
            let spell_entries = spells
                .iter()
                .map(|id| SpellEntry {
                    id: *id,
                    name: format!("Spell {id}"),
                    icon: Some(DataId(0x0600_13A5)),
                    school: 4,
                    level: 1,
                    icon_power: 1,
                    display_order: i32::try_from(*id).expect("a small id"),
                    bitfield: 0,
                })
                .collect();
            Self {
                spells,
                spell_entries,
                ..Self::default()
            }
        }
    }

    impl GameView for StubView {
        fn spellbook(&self) -> &[SpellEntry] {
            &self.spell_entries
        }
        fn spell_tab(&self, _tab: usize) -> &[u32] {
            &self.spells
        }
        fn player_option(&self, o: PlayerOption) -> bool {
            self.options.get(&(o as u32)).copied().unwrap_or(false)
        }
        fn recklessness_advancement_class(&self) -> u32 {
            self.recklessness
        }
        fn combat_bar(&self) -> CombatBar {
            self.bar.unwrap_or_default()
        }
    }
}
