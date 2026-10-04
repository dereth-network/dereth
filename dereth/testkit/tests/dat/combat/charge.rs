use super::*;

// ---------------------------------------------------------------------------------------------
// combat.attack.*
//
// The bench's calibration -- that it reaches the wire at all -- is folded into each scenario as
// the known positive beside the silence.
// ---------------------------------------------------------------------------------------------

/// A click starts a charge; the swing comes on the frame the bar reaches the gauge setting.
pub(super) fn one_click_charges_the_bar_and_swings_when_it_fills() {
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
pub(super) fn a_held_control_charges_to_full_and_swings_on_release() {
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
pub(super) fn the_button_and_the_key_start_the_same_charge() {
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
pub(super) fn a_clean_acknowledgement_rearms_the_loop() {
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
pub(super) fn any_other_acknowledgement_cancels_the_repeat() {
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
pub(super) fn a_moved_slider_refires_at_its_new_power() {
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
pub(super) fn the_acknowledgement_writes_no_line() {
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
pub(super) fn walking_backwards_breaks_a_repeating_swing() {
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
pub(super) fn a_release_a_turn_and_an_idle_walk_cancel_nothing() {
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
