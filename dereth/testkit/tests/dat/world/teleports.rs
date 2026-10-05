use super::*;

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

/// A recorded teleport moves the player's own body, and everything the client says about where
/// it is afterwards names the destination.
///
/// The three moments the report asks for: the arrival, the portal space the client stands still
/// through, and the walk away from where it landed -- which is what makes the second report, the
/// one that would have carried a stale position, exist at all.
pub fn a_recorded_teleport_moves_the_body_and_the_reports_follow() {
    use dereth_primitives::LocalTime;
    use {dereth_client_runtime::app::body_motion, dereth_client_runtime::app::player_timestamps};

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
            app.probe().player_teleport_use_times() == 3
                && app.probe().player_teleport_use_times() == app.frames_drawn()
                && app.probe().position_use_times() == app.frames_drawn()
                // With no shard and no body there is nothing to teleport, and nothing may have
                // been invented.
                && app.probe().player_teleports_applied() == 0
                && app.probe().player_teleports_before_a_body() == 0
        },
    );
    c.shutdown();
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
    let region = dereth_client_runtime::landblock::load_region(&store).expect("the region decodes");
    let mut out = dereth_client_runtime::character::Character::new(
        &store,
        &region,
        dereth_client_runtime::landblock::DEFAULT_LANDBLOCK,
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
