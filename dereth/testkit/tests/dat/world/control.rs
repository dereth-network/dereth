use super::*;

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
