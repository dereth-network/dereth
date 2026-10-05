//! The local body's motion style sets the combat power bar's charge rate: a full charge takes
//! 0.800 s in the dual-wield style (`DUAL_WIELD_COMBAT_STYLE` `0x80000046`) and 1.000 s in every
//! other, and the body's interpreted `current_style` and forward command are carried into
//! `World.combat` every frame, so the rate follows the style there and back. Asserted as the
//! charge time, not the field.
//! Fixture: `CombatState` alone for the rate; a headless `App` in gameplay with a real
//! `WorldScene` and `Character` for the bridge. No desktop, no shard.

#![cfg(gpu)]

use crate::common::client_dir;

use dereth_animation::MotionCommand;
use dereth_client::app::App;
use dereth_client_model::combat::{CombatState, DUAL_WIELD_COMBAT_STYLE};
use dereth_client_runtime::config::Config;
use dereth_primitives::LocalTime;

/// The rate, on its own. `POWER_BAR_SECONDS` is 1.000 and `_DUAL_WIELD` 0.800, and the only
/// thing that chooses between them is `current_style`. Asserted either side of each boundary, so
/// a constant that is merely *close* fails.
#[test]
fn the_dual_wield_charge_time_is_0_800_seconds_and_everything_else_is_1_000() {
    let bar = |style: u32, held: f64| {
        let mut c = CombatState::begin();
        c.current_style = style;
        c.start_power_bar_build(LocalTime(10.0));
        c.power_bar_level(LocalTime(10.0 + held))
    };
    // Non-combat, hand, bow: all 1.000 s.
    for style in [0x8000_003D, 0x8000_0000, 0x8000_003F] {
        assert!(
            bar(style, 0.999) < 1.0,
            "{style:#010X} is not charged at 0.999 s"
        );
        assert!(
            (bar(style, 1.000) - 1.0).abs() < 1e-6,
            "{style:#010X} is charged at 1.000 s"
        );
        assert!(
            (bar(style, 0.800) - 0.8).abs() < 1e-6,
            "{style:#010X} is 80% at 0.800 s"
        );
    }
    // Dual wield: 0.800 s, and it is **80% of the way charged at 0.800 s in every other style**,
    // which is the difference a player feels.
    assert!(
        bar(DUAL_WIELD_COMBAT_STYLE, 0.799) < 1.0,
        "not charged at 0.799 s"
    );
    assert!(
        (bar(DUAL_WIELD_COMBAT_STYLE, 0.800) - 1.0).abs() < 1e-6,
        "charged at 0.800 s"
    );
    assert_eq!(DUAL_WIELD_COMBAT_STYLE, 0x8000_0046);
    assert_eq!(MotionCommand(DUAL_WIELD_COMBAT_STYLE).0, 0x8000_0046);
}

/// Behaviour: combat.power-bar.the-bodys-motion-style-sets-the-charge-rate
///
/// The bridge, in a running client: non-combat, dual wield, and back, with the charge time as
/// the observable; then the forward command follows the body from `Ready` to walking.
#[test]
fn the_local_bodys_motion_style_reaches_the_combat_system_and_changes_the_charge_time() {
    let cfg = Config {
        ui: true,
        headless: true,
        sound: false,
        dat_dir: client_dir(),
        ..Config::default()
    };
    assert!(
        dereth_dat::testing::have_dats(),
        "the retail dats must be at {} -- set DERETH_TEST_DAT_DIR",
        client_dir().display()
    );
    let mut app = App::new(cfg).unwrap_or_else(|e| panic!("the application must start: {e}"));
    app.start_shell()
        .unwrap_or_else(|e| panic!("the shell must come up: {e}"));
    let s = dereth_client_runtime::scene::SceneConfig {
        landblock: app.config().landblock,
        land_radius: app.config().land_radius,
        scenery_radius: app.config().scenery_radius,
        ..dereth_client_runtime::scene::SceneConfig::default()
    };
    app.load_static_scene(s)
        .unwrap_or_else(|e| panic!("the static scene must load: {e}"));
    app.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    for _ in 0..4 {
        app.frame();
    }

    // The denominator: there **is** a local body. Without one this whole test asserts nothing, and
    // "the bridge did not run" and "there was nothing to bridge" would be the same silence.
    assert!(
        app.world_state()
            .and_then(|w| w.character.as_ref())
            .is_some(),
        "a local body is stood up -- otherwise the bridge has no input"
    );

    let charge_time = |app: &App| {
        let mut c = app.objects().world.combat.clone();
        c.start_power_bar_build(LocalTime(0.0));
        // The first instant at which the bar reads full, to a millisecond.
        (1..=2000)
            .map(|ms| f64::from(ms) / 1000.0)
            .find(|t| c.power_bar_level(LocalTime(*t)) >= 1.0)
            .expect("the bar charges within two seconds")
    };

    // Station A: starts at `NON_COMBAT`, so a full charge takes 1.000 s.
    assert_eq!(
        app.objects().world.combat.current_style,
        MotionCommand::NON_COMBAT.0
    );
    assert!(
        (charge_time(&app) - 1.000).abs() < 1e-9,
        "A: 1.000 s at NonCombat"
    );
    let bridges_a = app.interaction().stats.combat_style_bridges;

    // Station B: the motion layer takes the dual-wield style, by the same call
    // `MotionDriver::unpack_movement`'s pre-switch word makes (`apply_movement_style`).
    // Nothing else is touched: the combat state is not written by hand anywhere in this test.
    {
        let ch = app
            .probe_mut()
            .world_state_mut()
            .expect("scene")
            .character
            .as_mut()
            .expect("body");
        ch.driver_mut()
            .apply_movement_style(MotionCommand(DUAL_WIELD_COMBAT_STYLE));
    }
    app.frame();
    assert_eq!(
        app.objects().world.combat.current_style,
        DUAL_WIELD_COMBAT_STYLE,
        "B: the bridge carried the style across the seam"
    );
    assert!(
        app.interaction().stats.combat_style_bridges > bridges_a,
        "B: and it ran, rather than the field happening to hold the value"
    );
    assert!(
        (charge_time(&app) - 0.800).abs() < 1e-9,
        "B: 0.800 s while dual-wielding"
    );

    // Station C: back to a one-handed stance. A bridge that copies once passes B and fails here.
    {
        let ch = app
            .probe_mut()
            .world_state_mut()
            .expect("scene")
            .character
            .as_mut()
            .expect("body");
        ch.driver_mut()
            .apply_movement_style(MotionCommand::NON_COMBAT);
    }
    app.frame();
    assert_eq!(
        app.objects().world.combat.current_style,
        MotionCommand::NON_COMBAT.0,
        "C"
    );
    assert!(
        (charge_time(&app) - 1.000).abs() < 1e-9,
        "C: back to 1.000 s"
    );

    // ---- the second field of the same struct crosses the same seam ---------------------------
    //
    // The missile-ready predicate reads the interpreted forward command alongside
    // the current style, and refuses anything but `Ready (0x41000003)`. Without this bridge, the
    // forward-command field would be initialized by `CombatState::begin` but never refreshed from
    // the body's interpreted movement state. The station drives idle -> forward -> idle to prove
    // that propagation.
    //
    // The station is the body's own idle value first (so "the bridge ran" is not confused with
    // "the default happened to match"), then a real forward command, then back.
    assert_eq!(
        app.objects().world.combat.forward_command,
        MotionCommand::READY.0,
        "D: the settled body is in `Ready`, and the bridge carried it"
    );
    let bridges_d = app.interaction().stats.combat_style_bridges;
    {
        let ch = app
            .probe_mut()
            .world_state_mut()
            .expect("scene")
            .character
            .as_mut()
            .expect("body");
        ch.driver_mut()
            .movement
            .interp
            .interpreted_state
            .forward_command = MotionCommand::WALK_FORWARD;
    }
    app.frame();
    assert_eq!(
        app.objects().world.combat.forward_command,
        MotionCommand::WALK_FORWARD.0,
        "E: and it follows the motion layer rather than latching"
    );
    assert!(
        app.interaction().stats.combat_style_bridges > bridges_d,
        "E: the bridge ran, rather than the field happening to hold the value"
    );
}
