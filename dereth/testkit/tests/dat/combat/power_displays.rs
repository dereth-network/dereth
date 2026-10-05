use super::*;

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

pub(super) fn a_terminal_answer_empties_the_classic_meter_and_the_next_charge_works() {
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

pub(super) fn the_standalone_bar_and_the_classic_meter_are_separate_routes() {
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

pub(super) fn a_jump_raises_the_standalone_bar_and_not_the_classic_one() {
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

pub(super) fn a_notice_with_no_panel_to_hear_it_is_dropped() {
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

pub(super) fn the_power_bar_follows_the_clock_at_one_full_charge_a_second() {
    use dereth_client_contract::actions::mapped as ia;
    use dereth_client_model::combat::PowerBarMode;

    let readable = bars::the_readback_can_produce_a_non_zero();
    let mut b = bars::Bench::new();
    b.enter_advanced_combat();

    b.frame(vec![bars::key(ia::COMBAT_MEDIUM_ATTACK.0, true)], 100.0);
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

pub(super) fn a_two_handed_style_charges_a_quarter_faster() {
    use dereth_client_contract::actions::mapped as ia;

    let mut b = bars::Bench::new();
    b.enter_advanced_combat();
    b.set_style(dereth_client_model::combat::DUAL_WIELD_COMBAT_STYLE);

    b.frame(vec![bars::key(ia::COMBAT_MEDIUM_ATTACK.0, true)], 200.0);
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

pub(super) fn exactly_one_of_the_two_power_displays_is_live() {
    use dereth_client_contract::actions::mapped as ia;
    use dereth_client_model::combat::PowerBarMode;
    use dereth_ui_screens::hud::powerbar as pb;

    let readable = bars::the_readback_can_produce_a_non_zero();
    // Ordinary combat: the window's meter carries it and the standalone bars are never written.
    let mut classic = bars::Bench::new();
    classic.frame(vec![bars::key(ia::COMBAT_MEDIUM_ATTACK.0, true)], 300.0);
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
    advanced.frame(vec![bars::key(ia::COMBAT_MEDIUM_ATTACK.0, true)], 400.0);
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

pub(super) fn the_power_bar_is_shown_when_a_charge_begins_and_emptied_when_it_ends() {
    use dereth_client_contract::actions::mapped as ia;
    use dereth_ui_screens::hud::powerbar as pb;

    let mut b = bars::Bench::new();
    b.enter_advanced_combat();
    let down_at_first = b.bars_visible() == vec![false, false];

    b.frame(vec![bars::key(ia::COMBAT_MEDIUM_ATTACK.0, true)], 500.0);
    b.frame(vec![], 500.4);
    let sub = b.subscriber();
    let shown = b.bars_visible() == vec![false, true]
        && b.bar_modes()[sub] == pb::PowerBarMode::AdvancedCombat
        && b.bar_modes()[1 - sub] == pb::PowerBarMode::Undef;

    b.frame(vec![], 500.5);
    let charged = b.drawn_level() > 0.0;

    b.frame(vec![bars::key(ia::COMBAT_MEDIUM_ATTACK.0, false)], 500.6);
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

pub(super) fn the_keyboard_gauge_has_seven_notches_and_starts_half_way() {
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

pub(super) fn the_power_drag_is_continuous_and_lands_between_the_notches() {
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

pub(super) fn the_notch_and_the_fill_are_different_things() {
    use dereth_client_contract::actions::mapped as ia;

    let mut b = bars::Bench::new();
    b.set_requested_power_from_a_drag(833);
    b.frame(vec![bars::key(ia::COMBAT_MEDIUM_ATTACK.0, true)], 600.0);
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

pub(super) fn the_display_holds_the_level_the_swing_went_out_at() {
    use dereth_client_contract::actions::mapped as ia;
    use dereth_primitives::LocalTime;

    let mut b = bars::Bench::new();
    b.frame(vec![bars::key(ia::COMBAT_MEDIUM_ATTACK.0, true)], 700.0);
    b.frame(vec![], 700.5);
    // While the charge is building the clock and what was last sent agree, which is why a display
    // that recomputed looked right until the swing.
    let agree = (b.combat().power_bar_level(LocalTime(700.5)) - b.combat().latest_power_bar_level)
        .abs()
        < 1e-5;

    b.frame(vec![bars::key(ia::COMBAT_MEDIUM_ATTACK.0, false)], 700.55);
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
