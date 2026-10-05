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
        victim.pwd.obj_type = dereth_rules::weenie::item_type::CREATURE;
        victim.pwd.bitfield = dereth_rules::weenie::bitfield::ATTACKABLE;
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
