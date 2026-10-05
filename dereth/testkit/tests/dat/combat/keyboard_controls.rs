use super::*;

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

pub(super) fn one_set_of_keys_means_three_different_things() {
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

pub(super) fn every_height_key_moves_the_height_in_both_modes() {
    use dereth_client_model::combat::{AttackHeight, CombatMode};
    use dereth_client_runtime::interaction::action as ia;

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

pub(super) fn the_gauge_keys_step_by_a_sixth_in_both_modes() {
    use dereth_client_model::combat::CombatMode;
    use dereth_client_runtime::interaction::action as ia;

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
                        shell: &mut dereth_client_shell::input::InputShell,
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

pub(super) fn a_value_between_notches_rounds_to_the_nearest_before_stepping() {
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

pub(super) fn holding_a_height_key_refuses_once_and_not_once_per_repeat() {
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

pub(super) fn at_peace_no_combat_key_reaches_an_arm() {
    use dereth_client_model::combat::{AttackHeight, CombatMode};
    use dereth_client_runtime::interaction::action as ia;

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

pub(super) fn every_control_in_the_combat_window_does_what_the_client_does() {
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
    let ordinal = dereth_client_runtime::hud::option_ordinal(PlayerOption::ViewCombatTarget);
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

pub(super) fn the_combat_window_reflects_the_height_the_power_and_the_notch() {
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

pub(super) fn the_frames_own_pass_drives_the_combat_windows_read_backs() {
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
        + dereth_client_shell::hud_drive::deliver_power_bar_notices(
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

pub(super) fn the_recklessness_meter_appears_at_exactly_the_trained_class() {
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
