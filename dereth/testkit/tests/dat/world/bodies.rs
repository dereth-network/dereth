use super::*;

// -------------------------------------------------------------------------------------------
// 19. movement.run.reports-walk-with-the-run-hold-key
// -------------------------------------------------------------------------------------------

/// Running moves the body at run speed and is reported as walking with the run key held.
pub fn running_reports_walk_with_the_run_hold_key() {
    use dereth_primitives::LocalTime;
    use dereth_world_data::landblock::DEFAULT_LANDBLOCK;
    use {
        dereth_client_runtime::character::Character,
        dereth_client_runtime::character::CharacterInput,
    };

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
        let region = dereth_world_data::landblock::load_region(store).expect("the region decodes");
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
        let s = dereth_client_runtime::app::raw_motion_state_to_wire(
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

// -------------------------------------------------------------------------------------------
// 20. physics.enter-world.refuses-a-body-inside-a-building
// -------------------------------------------------------------------------------------------

/// A body may only enter the world where it fits.
#[cfg(windows)]
pub fn enter_world_refuses_a_body_inside_a_building() {
    use std::sync::Arc;

    use dereth_physics::source::SetupGeometry;
    use dereth_physics::{LandSource, PhysicsWorld};
    use dereth_primitives::{CellId, Frame, LandblockId, ObjectId, Position, Quat, Vec3};
    use dereth_world_data::land_source::DatLandSource;

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
            let region =
                dereth_world_data::landblock::load_region(&store).expect("the region decodes");
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

// -------------------------------------------------------------------------------------------
// movement.teleport.arrives-exactly-on-another-player-and-is-pushed-aside-only-by-a-creature
// -------------------------------------------------------------------------------------------

/// Teleporting onto another player's exact spot lands there; a creature overlapping it does not.
///
/// The whole measurement is one arrival: settle a body, note the ground-true destination, stand the
/// scene's bodies on it, move the local body away, then run the production teleport back onto it.
pub fn a_teleport_onto_another_player_is_deflected_only_by_a_creature() {
    use dereth_physics::math::V3 as _;
    use dereth_primitives::{LocalTime, ObjectId, Position};
    use {
        dereth_client_runtime::character::CharacterInput,
        dereth_client_runtime::character::MovementCommands,
    };

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

        let mut stream = dereth_client_runtime::objects::ObjectStream::new();
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
        dereth_client_runtime::app::complete_player_teleport_at(
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
    dereth_client_runtime::character::Character,
    dereth_client_runtime::objects::ObjectStream,
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

    let mut stream = dereth_client_runtime::objects::ObjectStream::new();
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

    let mut stream = dereth_client_runtime::objects::ObjectStream::new();
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
    let mut stream2 = dereth_client_runtime::objects::ObjectStream::new();
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

// -------------------------------------------------------------------------------------------
// movement.remote-body.a-hook-a-storage-chest-or-a-corpse-is-put-down-exactly-where-sent
// -------------------------------------------------------------------------------------------

/// A house hook, a storage chest and a corpse, with the state words ACE sends them, stand exactly
/// where they were sent and stay there through a second and a half of physics: sent a hand's
/// breadth into the ground, and (the chest, the one of the three that is solid) onto a spot a
/// creature already fills. The same descriptions without the one fact that makes each that kind
/// are put on top of the ground, and the chest beside the creature.
pub fn a_hook_a_storage_chest_or_a_corpse_is_put_down_exactly_where_sent() {
    use dereth_physics::math::V3 as _;
    use dereth_primitives::{LocalTime, ObjectId};
    use dereth_protocol::types::PublicWeenieDesc;

    const ARRIVAL: ObjectId = ObjectId(0x8000_114E);
    const MONSTER: ObjectId = ObjectId(0x8000_114F);
    /// The public description's corpse bit.
    const CORPSE: u32 = 0x0000_2000;
    /// A wall hook's hook type, and the item types it takes: all of them.
    const WALL: u16 = 2;
    const ANY_ITEM: u32 = u32::MAX;
    /// The weenie classes ACE gives a wall hook and a decorative Lugian corpse.
    const WALL_HOOK_CLASS: u32 = 9686;
    const CORPSE_CLASS: u32 = 25457;
    /// The state words ACE sends: a wall hook `IGNORE_COLLISIONS | ETHEREAL`, a storage chest
    /// `GRAVITY | IGNORE_COLLISIONS | REPORT_COLLISIONS`, a corpse
    /// `GRAVITY | IGNORE_COLLISIONS | ETHEREAL`.
    const HOOK_STATE: u32 = 0x14;
    const CHEST_STATE: u32 = 0x418;
    const CORPSE_STATE: u32 = 0x414;
    /// Their setups.
    const HOOK_SETUP: u32 = 0x0200_0A8E;
    const CHEST_SETUP: u32 = 0x0200_0A97;
    const CORPSE_SETUP: u32 = 0x0200_0F9C;
    /// How far into the ground they are sent, and how long physics then runs.
    const INTO: f32 = 0.1;
    const TICKS: usize = 30;

    let store = store();
    let storage = dereth_client_runtime::object_physics::storage_class(&*store)
        .expect("the portal dat names the storage chest's class");
    // The two hook fields go over the wire only when the header names them.
    let described =
        |name: &str, wcid: u32, bitfield: u32, hook: Option<(u16, u32)>| PublicWeenieDesc {
            header: hook.map_or(0, |_| {
                dereth_protocol::types::weeniedesc::header::HOOK_TYPE
                    | dereth_protocol::types::weeniedesc::header::HOOK_ITEM_TYPES
            }),
            name: name.to_owned(),
            wcid,
            bitfield,
            hook_type: hook.map(|h| h.0),
            hook_item_types: hook.map(|h| h.1),
            ..Default::default()
        };
    // Each kind with its setup and state, and the same description without the one fact that
    // makes it that kind: a hook that takes no item types, the class after the storage chest's,
    // a corpse without the bit.
    let kinds = [
        (
            "a wall hook",
            (HOOK_SETUP, HOOK_STATE),
            described("Wall Hook", WALL_HOOK_CLASS, 0, Some((WALL, ANY_ITEM))),
            described("Wall Hook", WALL_HOOK_CLASS, 0, Some((WALL, 0))),
        ),
        (
            "a storage chest",
            (CHEST_SETUP, CHEST_STATE),
            described("Storage", storage, 0, None),
            described("Storage", storage + 1, 0, None),
        ),
        (
            "a corpse",
            (CORPSE_SETUP, CORPSE_STATE),
            described("Lugian Corpse", CORPSE_CLASS, CORPSE, None),
            described("Lugian Corpse", CORPSE_CLASS, 0, None),
        ),
    ];

    // Sent `dz` from the ground the local body stood on, with a creature a quarter of a metre
    // along x when asked: how far the arrival ever stands from where it was sent over the ticks,
    // and where it ends, from where it was sent.
    let lands = |shape: (u32, u32), wdesc: PublicWeenieDesc, dz: f32, creature: bool| {
        let mut c = fresh_local_body(&store);
        let mut sent = c.position();
        sent.frame.origin.z += dz;
        let mut away = sent;
        away.frame.origin.x -= 12.0;
        away.frame.origin.z -= dz;
        c.teleport(away);
        let mut stream = dereth_client_runtime::objects::ObjectStream::new();
        if creature {
            let mut monster_at = sent;
            monster_at.frame.origin.x += 0.25;
            stream.apply_event(
                &create_event(MONSTER, Some(monster_at), 0, "Monster"),
                LocalTime(9.0),
            );
            stream.sync_physics(&store, &mut c.world);
            assert_eq!(
                body_origin(&c, MONSTER),
                Some(monster_at.frame.origin),
                "the creature's own create was moved, so the overlap under test is not the one \
                 asked for"
            );
        }
        stream.apply_event(
            &create_event_shaped(ARRIVAL, Some(sent), 0, shape, wdesc),
            LocalTime(9.0),
        );
        stream.sync_physics(&store, &mut c.world);
        let off = |c: &dereth_client_runtime::character::Character| {
            body_origin(c, ARRIVAL)
                .expect("the arrival is in a cell")
                .sub(sent.frame.origin)
        };
        let mut worst = off(&c).mag2().sqrt();
        let mut t = 9.0;
        for _ in 0..TICKS {
            t += DT;
            c.update(LocalTime(t));
            worst = worst.max(off(&c).mag2().sqrt());
        }
        (worst, off(&c))
    };

    let mut exact = true;
    let mut put_down = true;
    for (kind, shape, it, without) in kinds {
        let (sent, _) = lands(shape, it.clone(), -INTO, false);
        let (_, raised) = lands(shape, without.clone(), -INTO, false);
        println!(
            "placement: {kind} sent into the ground is at most {sent:.6} m from where it was \
             sent; without what makes it one it ends {:+.6} m from there",
            raised.z
        );
        exact &= sent < 1e-4;
        put_down &= raised.z > 0.05 && raised.z < 0.2;
        // The one solid kind, onto a creature.
        if shape.1 == CHEST_STATE {
            let (onto, _) = lands(shape, it, 0.0, true);
            let (_, beside) = lands(shape, without, 0.0, true);
            let beside = beside.mag2().sqrt();
            println!(
                "placement: {kind} sent onto a creature is at most {onto:.6} m from where it was \
                 sent; without what makes it one, {beside:.6} m"
            );
            exact &= onto < 1e-4;
            put_down &= beside > 0.1 && beside <= 4.0;
        }
    }

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.remote-body.a-hook-a-storage-chest-or-a-corpse-is-put-down-exactly-where-sent",
        move |_| exact && put_down,
    );
}

// -------------------------------------------------------------------------------------------
// movement.remote-body.a-re-create-of-an-object-the-client-holds-moves-it-as-a-position-update-does
// -------------------------------------------------------------------------------------------

/// The shard sending again an object the client already holds moves it as a position update
/// does: a body nearby glides to the new spot over the following frames rather than appearing
/// there, and a re-send whose position is not newer moves nothing.
pub fn a_re_create_of_a_held_object_moves_it_as_a_position_update_does() {
    use dereth_physics::math::V3 as _;
    use dereth_physics::pmanager::CLOSE_ENOUGH;
    use dereth_primitives::{LocalTime, ObjectId};
    use dereth_protocol::types::PublicWeenieDesc;

    const VICTIM: ObjectId = ObjectId(0x8000_114C);

    let store = store();
    let (mut c, mut stream, mut t, destination) = glide_scene(&store, VICTIM, true);
    let before = body_origin(&c, VICTIM).expect("the body is in a cell");
    let distance = before.sub(destination.frame.origin).mag2().sqrt();
    assert!(
        distance > 2.0,
        "the move under test is only {distance:.3} m long"
    );
    // The body's own description sent again, at the same instance as its first create.
    let victim = || PublicWeenieDesc {
        name: "Victim".to_owned(),
        obj_type: CREATURE,
        bitfield: PLAYER,
        ..Default::default()
    };
    let arms = stream.physics.stats.interpolate_arm;
    let merges = stream.stats.merges;

    stream.apply_event(
        &create_event_described(VICTIM, Some(destination), 1, victim()),
        LocalTime(t),
    );
    stream.sync_physics(&store, &mut c.world);
    let h = c.world.by_object_id(VICTIM).expect("the body exists");
    let merged = stream.stats.merges == merges + 1;
    let queued_not_moved = stream.physics.stats.interpolate_arm == arms + 1
        && body_origin(&c, VICTIM) == Some(before)
        && c.world.is_interpolating(h);

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let steps = (distance / STEP).ceil() as usize + 1;
    for _ in 0..steps {
        t += DT;
        c.update(LocalTime(t));
    }
    let settled = body_origin(&c, VICTIM).expect("the body is in a cell");
    let offset = settled.sub(destination.frame.origin).mag2().sqrt();
    println!("re-create: after {steps} sub-steps the body is {offset:.6} m from the new spot");
    let finished = offset < CLOSE_ENOUGH && !c.world.is_interpolating(h);

    // The same position stamp again, naming the spot it started from: nothing moves.
    let mut back = destination;
    back.frame.origin = before;
    stream.apply_event(
        &create_event_described(VICTIM, Some(back), 1, victim()),
        LocalTime(t),
    );
    stream.sync_physics(&store, &mut c.world);
    for _ in 0..10 {
        t += DT;
        c.update(LocalTime(t));
    }
    let stale_moves_nothing = body_origin(&c, VICTIM)
        .is_some_and(|o| o.sub(settled).mag2().sqrt() < 1e-4)
        && !c.world.is_interpolating(h);

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.remote-body.a-re-create-of-an-object-the-client-holds-moves-it-as-a-position-update-does",
        move |_| merged && queued_not_moved && finished && stale_moves_nothing,
    );
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
    dereth_client_runtime::character::Character,
    dereth_client_runtime::objects::ObjectStream,
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

    let mut stream = dereth_client_runtime::objects::ObjectStream::new();
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
    const HOME: u16 = dereth_world_data::landblock::DEFAULT_LANDBLOCK;
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

    fn ground(c: &dereth_client_runtime::character::Character, block: u16, x: f32, y: f32) -> f32 {
        c.land()
            .ground_height(LandblockId(block), x, y)
            .unwrap_or_else(|| panic!("that block has no terrain under ({x}, {y})"))
    }

    /// Park the body at a landblock-relative point and let it settle.
    fn settle(
        c: &mut dereth_client_runtime::character::Character,
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
        dereth_client_runtime::character::Character,
        dereth_client_runtime::objects::ObjectStream,
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

        let mut stream = dereth_client_runtime::objects::ObjectStream::new();
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
        c: &dereth_client_runtime::character::Character,
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

/// Walking forward with running off reports a walk and no modifier, and releasing reports nothing.
pub fn walking_forward_reports_a_walk_with_no_hold_key() {
    use dereth_client_runtime::character::CharacterInput;
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

/// A live running body's report is byte for byte a recorded one, and it really goes out.
pub fn a_running_bodys_report_is_the_retail_clients_own() {
    use dereth_client_net::client_session::testing::MockTransport;
    use dereth_client_net::client_session::{
        ContactPlane, PlayerMotion, PositionReporter, Session,
    };
    use dereth_client_runtime::character::CharacterInput;
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
            &dereth_client_runtime::app::body_motion(&c, MoveTimestamps::default()),
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
            &dereth_client_runtime::app::body_motion(&c, MoveTimestamps::default()),
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

/// Turning running on part way through a walk speeds the body up and changes the report.
pub fn turning_run_on_mid_walk_speeds_the_body_and_changes_the_report() {
    use dereth_client_runtime::character::CharacterInput;
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
