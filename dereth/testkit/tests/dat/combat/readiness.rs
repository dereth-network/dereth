use super::*;

// ---------------------------------------------------------------------------------------------
// combat.readiness.* and combat.mode.a-change-the-body-is-not-ready-for-…
//
// The body is one real `Character` on the retail terrain, settled and then jumped, asked the same
// question twice.
// ---------------------------------------------------------------------------------------------

/// Readiness is the body's own motion queue, in both directions, and it is not ground contact.
pub(super) fn ready_is_the_bodys_own_motion_queue() {
    use dereth_client_runtime::character::CharacterInput;
    use dereth_client_runtime::interaction::Interaction;
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
pub(super) fn a_stance_change_the_body_refuses_is_queued() {
    use dereth_client_model::combat::CombatMode;
    use dereth_primitives::LocalTime;
    use {dereth_client_runtime::interaction, dereth_client_runtime::interaction::Interaction};

    let store = store();
    let (body, _) = settled(&store);
    let mut inter = Interaction::new();
    let mut objects = dereth_client_runtime::objects::ObjectStream::default();
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
pub(super) fn the_two_flavours_disagree_on_one_body() {
    use dereth_client_model::combat::CombatMode;
    use dereth_client_runtime::character::CharacterInput;
    use dereth_client_runtime::interaction::Interaction;
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
pub(super) fn a_melee_swing_needs_the_weapons_combat_table() {
    use dereth_client_model::combat::CombatMode;
    use dereth_client_runtime::interaction::Interaction;

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
pub(super) fn a_missile_swing_needs_the_stance_and_the_ready_command() {
    use dereth_client_model::combat::{CombatMode, MISSILE_READY_STYLES};
    use dereth_client_runtime::interaction::Interaction;

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
pub(super) fn all_three_attack_arms_consume_the_produced_flavour() {
    use dereth_client_model::combat::{CombatMode, MISSILE_READY_STYLES};
    use dereth_primitives::{LocalTime, ServerTime};
    use dereth_ui_screens::view::UiRequest;
    use {dereth_client_runtime::interaction, dereth_client_runtime::interaction::Interaction};

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
        let mut objects = dereth_client_runtime::objects::ObjectStream::default();
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
    let mut objects = dereth_client_runtime::objects::ObjectStream::default();
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
