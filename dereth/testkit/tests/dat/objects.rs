//! Objects: the object stream over real setups and real cells, and what the panels read off it.
//!
//! Fixture: the retail dats under `$DERETH_TEST_DAT_DIR`, and the recordings
//! `dereth_client_net::client_session::testing::session_index()` names, replayed through the real
//! session. Most scenarios open the data files directly and book the claim at the end; no whole
//! `App` is built. No count is pinned: every denominator is read off the recordings.
//!
//! **This binary must run serially**: two headless clients in one process share the UI request
//! globals. `ALL` is concatenated with the other subjects' lists in `census.rs`.

use std::collections::BTreeMap;
use std::sync::Arc;

use dereth_client_net::client_session::SessionEvent;
use dereth_client_runtime::objects::ObjectStream;
use dereth_dat::RetailDatStore;
use dereth_physics::{LandSource, PhysicsWorld};
use dereth_primitives::{CellId, LocalTime, ObjectId, ServerTime};
use dereth_protocol::objects::{ItemCreateObject, ItemSetState, ObjectCreatePayload};
use dereth_protocol::types::physicsdesc::{flags, PhysicsDesc};
use dereth_protocol::types::PositionWire;
use dereth_protocol::{Message as _, Opcode};
use dereth_testkit::HeadlessClient;

/// The retail data files. An `expect`, never a skip: a scenario that quietly passes on a machine
/// with no data files is a test that passes by skipping.
fn store() -> Arc<RetailDatStore> {
    Arc::new(dereth_dat::testing::open_store().unwrap_or_else(|| {
        panic!(
            "the retail data files are this scenario's oracle and there are none under {} -- set \
             DERETH_TEST_DAT_DIR",
            dereth_dat::testing::dat_dir().display()
        )
    }))
}

/// The human setup every attachment scenario here uses: its parts really do carry a
/// holding location, which is what makes a refusal of another one a result.
const PERSON_SETUP: u32 = 0x0200_0001;
/// A holding location that setup's own parts carry.
const RIGHT_HAND: u32 = 1;
/// One that no setup carries at all.
const NOT_A_PLACE: u32 = 4242;
/// The door setup whose parts carry a collision tree and no spheres at all, so it is intangible
/// unless the tree is read.
const DOOR_SETUP: u32 = 0x0200_024F;
/// The bit the client sets when an object's parts carry that collision tree.
const HAS_PHYSICS_BSP_PS: u32 = 0x0001_0000;
/// The bit that stops a body being seen at all.
const HIDDEN_PS: u32 = 0x0000_4000;

fn create_body(
    id: ObjectId,
    setup: Option<u32>,
    cell: Option<CellId>,
    parent: Option<(ObjectId, u32)>,
) -> Vec<u8> {
    let mut physicsdesc = PhysicsDesc::default();
    if let Some(s) = setup {
        physicsdesc.bitfield |= flags::SETUP;
        physicsdesc.setup_id = Some(s);
    }
    if let Some(c) = cell {
        physicsdesc.bitfield |= flags::POSITION;
        physicsdesc.position = Some(PositionWire {
            objcell_id: c.0,
            frame: dereth_protocol::types::Frame::default(),
        });
    }
    if parent.is_some() {
        physicsdesc.bitfield |= flags::PARENT;
        physicsdesc.parent = parent;
    }
    dereth_protocol::write_body(&ItemCreateObject(ObjectCreatePayload {
        id,
        objdesc: dereth_protocol::types::ObjDesc::default(),
        physicsdesc,
        wdesc: dereth_protocol::types::PublicWeenieDesc::default(),
    }))
    .expect("the encoder writes what the decoder reads")
}

fn feed(s: &mut ObjectStream, opcode: Opcode, body: Vec<u8>, now: f64) {
    s.apply_event(&SessionEvent::WorldObject { opcode, body }, LocalTime(now));
}

use dereth_testkit::replay::recorded_endpoint as endpoint;
/// The recording, and the endpoint it replays into, both shared from `dereth_testkit::replay`.
use dereth_testkit::replay::records as recording;

// =============================================================================================
// objects.create.the-shards-state-word-wins-over-the-geometry-scan
// =============================================================================================

/// The descriptor's word is the word the body ends up with, in both directions.
pub fn the_descriptors_word_wins_over_the_scan() {
    use dereth_client_runtime::object_physics::ObjectPhysics;
    use dereth_client_runtime::objects::Presence;
    use dereth_primitives::{DataId, Frame, Position};

    /// Spawn one object through the real synchronisation and report the state its body ends with,
    /// alongside what the client's own scan of the setup's parts thinks.
    fn spawn(store: &RetailDatStore, setup: u32, wire: u32) -> (u32, bool) {
        let id = ObjectId(0x8000_0007);
        let at = Position::new(CellId(0x0086_0100), Frame::default());
        let mut presences = BTreeMap::new();
        presences.insert(
            id,
            Presence {
                setup_id: Some(DataId(setup)),
                scale: 1.0,
                // The achieved pose and the last pose the wire named are two fields; the
                // synchronisation reads the second, so a fixture that filled only the first would
                // create no body at all.
                position: Some(at),
                server_position: Some(at),
                ..Presence::default()
            },
        );
        let mut game = dereth_client_model::World::new();
        game.tables.physics.insert(
            id,
            dereth_client_model::objects::PhysicsPresence {
                state: wire,
                ..Default::default()
            },
        );
        let land: Arc<dyn LandSource> =
            Arc::new(dereth_physics::source::StaticLandSource::default());
        let mut world = PhysicsWorld::new(land);
        let mut phys = ObjectPhysics::new();
        phys.sync(store, &mut world, &presences, &game, None);
        let h = phys.handle(id).expect("the object got a body");
        let o = world.get(h).expect("live");
        (o.state().0, o.geometry.caches_physics_bsp())
    }

    let store = store();

    // A word the recordings really carry for this setup: a door, shut.
    let (with, scan_says) = spawn(&store, DOOR_SETUP, 0x0001_0018);
    // The premise: the client's own scan of this setup's parts says the bit belongs on, so the
    // arm below is a result and not an instrument that never sets anything.
    let the_scan_would_set_it = scan_says;
    let descriptor_kept = with & HAS_PHYSICS_BSP_PS != 0 && with == 0x0001_0018;

    // The same object with the bit taken out of the word. The body loses it, because a state
    // change assigns the whole word rather than merging it -- a build that re-ran the scan
    // afterwards would keep it.
    let (without, _) = spawn(&store, DOOR_SETUP, 0x0001_0018 & !HAS_PHYSICS_BSP_PS);
    let descriptor_wins_the_other_way = without == 0x0000_0018;

    // And the bits nothing reacts to are assigned just the same.
    let (whole, _) = spawn(&store, DOOR_SETUP, 0x0001_0418);
    let assigned_whole = whole == 0x0001_0418;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "objects.create.the-shards-state-word-wins-over-the-geometry-scan",
        move |_| {
            the_scan_would_set_it
                && descriptor_kept
                && descriptor_wins_the_other_way
                && assigned_whole
        },
    );
}

dereth_testkit::scenarios! {
    scenario_the_descriptors_word_wins_over_the_scan => the_descriptors_word_wins_over_the_scan ["objects.create.the-shards-state-word-wins-over-the-geometry-scan"],
    scenario_every_recorded_state_change_reaches_its_body => every_recorded_state_change_reaches_its_body ["object.set-state.every-recorded-change-reaches-the-body-it-names"],
    scenario_a_hidden_body_stops_being_a_target => a_hidden_body_stops_being_a_target ["object.set-state.a-body-the-shard-hides-stops-being-a-target-and-comes-back-in-place"],
    scenario_a_new_animation_table_alone_reaches_the_body => a_new_animation_table_alone_reaches_the_body ["objects.description.a-new-animation-table-alone-reaches-the-body"],
    scenario_a_holder_holds_only_where_its_body_has_a_place => a_holder_holds_only_where_its_body_has_a_place ["objects.parent.a-holder-holds-only-at-a-place-its-own-body-has"],
    scenario_a_held_object_has_no_body_of_its_own => a_held_object_has_no_body_of_its_own ["objects.held-object.has-no-body-and-leaves-the-cell-with-its-holder"],
    scenario_an_older_teleport_puts_the_position_stamp_back => an_older_teleport_puts_the_position_stamp_back ["objects.position.an-older-teleport-generation-puts-the-position-stamp-back"],
    scenario_an_unloaded_body_is_rescued_by_a_position => an_unloaded_body_is_rescued_by_a_position ["objects.unloaded-cell.a-body-is-rescued-by-the-next-position-the-shard-sends"],
    scenario_the_split_box_is_not_resurrected_by_a_rebuild => the_split_box_is_not_resurrected_by_a_rebuild ["selection.split-box.is-not-resurrected-by-a-screen-rebuild"],
    scenario_the_selection_read_out_follows_one_change_once => the_selection_read_out_follows_one_change_once ["selection.read-out.follows-one-selection-change-once"],
    scenario_the_shards_first_word_opens_a_box_the_player_can_dismiss => the_shards_first_word_opens_a_box_the_player_can_dismiss ["shard-message.pop-up.the-first-thing-the-shard-says-opens-a-box-the-player-can-dismiss"],
    scenario_three_prompts_in_a_burst_are_three_boxes_at_once => three_prompts_in_a_burst_are_three_boxes_at_once ["shard-message.pop-up.three-in-one-burst-are-three-boxes-at-once"],
    scenario_the_shards_house_answer_redraws_the_house_tab => the_shards_house_answer_redraws_the_house_tab ["shard-message.house-status.the-shards-answer-redraws-the-house-tab-every-time"],
    scenario_the_fellowship_done_marker_is_consumed_and_changes_nothing => the_fellowship_done_marker_is_consumed_and_changes_nothing ["shard-message.fellowship-update-done.is-consumed-and-changes-nothing-else"],
    scenario_none_of_the_three_reaches_no_receiver => none_of_the_three_reaches_no_receiver ["shard-message.the-three-the-shard-sends-on-every-login-all-reach-a-receiver"],
    scenario_selecting_a_creature_asks_about_its_health_once => selecting_a_creature_asks_about_its_health_once ["selection.query.selecting-a-creature-asks-the-shard-about-its-health-once"],
    scenario_selecting_a_stack_asks_nothing => selecting_a_stack_asks_nothing ["selection.query.selecting-a-pile-of-things-asks-the-shard-nothing"],
    scenario_what_is_asked_has_four_outcomes_and_not_two => what_is_asked_has_four_outcomes_and_not_two ["selection.query.what-the-client-asks-about-a-selected-thing-has-four-outcomes"],
    scenario_a_pile_that_shrinks_to_one_asks_again => a_pile_that_shrinks_to_one_asks_again ["selection.query.a-pile-that-shrinks-to-one-asks-again-with-no-selection-change"],
    scenario_the_shards_answer_fills_the_selected_things_health_bar => the_shards_answer_fills_the_selected_things_health_bar ["selection.meters.the-shards-answer-fills-the-selected-things-health-bar"],
    scenario_a_magic_answer_fills_the_bar_only_when_it_succeeded => a_magic_answer_fills_the_bar_only_when_it_succeeded ["selection.meters.an-answer-about-a-things-magic-fills-its-bar-only-when-it-succeeded"],
    scenario_a_new_selection_clears_only_the_bar_that_was_showing => a_new_selection_clears_only_the_bar_that_was_showing ["selection.query.a-new-selection-clears-only-the-bar-that-was-showing"],
    scenario_an_out_of_range_holding_part_uses_the_holders_frame => an_out_of_range_holding_part_uses_the_holders_frame ["objects.held-frame.an-out-of-range-part-uses-the-holders-own-frame"],
    scenario_a_new_parent_event_attaches_the_child_in_wire_order => a_new_parent_event_attaches_the_child_in_wire_order ["objects.parent.a-new-parent-event-attaches-the-child-in-wire-order"],
    scenario_old_and_incomplete_parent_events_change_nothing => old_and_incomplete_parent_events_change_nothing ["objects.parent.an-old-or-incomplete-parent-event-changes-nothing"],
    scenario_a_position_report_releases_an_object_from_its_parent => a_position_report_releases_an_object_from_its_parent ["objects.position.a-position-report-releases-an-object-from-its-parent"],
    scenario_a_server_named_placement_draws_its_own_frames => a_server_named_placement_draws_its_own_frames ["objects.placement.a-server-named-placement-draws-its-own-frames"],
    scenario_a_pick_sweep_uses_the_placement_the_shard_named => a_pick_sweep_uses_the_placement_the_shard_named ["objects.pick.a-sweep-uses-the-placement-the-shard-named"],
    scenario_a_held_object_is_pickable_and_a_contained_object_is_not => a_held_object_is_pickable_and_a_contained_object_is_not ["objects.held-object.is-a-pick-candidate-while-a-contained-object-is-not"],
}

// =============================================================================================
// object.set-state.every-recorded-change-reaches-the-body-it-names
// =============================================================================================

/// What one recording's state changes did to the bodies they name.
#[derive(Default)]
struct Verdict {
    delivered: usize,
    on_a_live_body: usize,
    on_the_player: usize,
    no_body: usize,
    wrong_word: usize,
    written_to_a_body: usize,
    applied: usize,
}

/// Every recorded state change reaches the body it names, with the word that was on the wire.
///
/// The per-frame synchronisation is the point: the client runs it once a frame, so it sees every
/// intermediate word. A replay that synchronised only at the end would see the last word each
/// object was given and would score a door that opened and shut as a door that only ever shut.
pub fn every_recorded_state_change_reaches_its_body() {
    let store = store();
    let mut total = Verdict::default();

    for (id, slug) in dereth_client_net::client_session::testing::session_index() {
        let records = recording(id);
        let mut net = endpoint(&records);
        let mut objects = ObjectStream::new();
        let land: Arc<dyn LandSource> =
            Arc::new(dereth_physics::source::StaticLandSource::default());
        let mut world = PhysicsWorld::new(land);
        let mut entered = false;
        let mut v = Verdict::default();
        for r in &records {
            let now = LocalTime(r.t);
            if !r.c2s {
                net.feed(&r.raw, r.peer(), now);
            }
            net.tick(now);
            let _ = net.take_outgoing();
            let mut arrived: Vec<ItemSetState> = Vec::new();
            // Snapshotted **before** the pump, because the pump is what applies the message: a
            // counter read afterwards would measure nothing and report a confident zero.
            let before = objects.stats.state_events;
            for e in objects.pump(&mut net, now) {
                match &e {
                    SessionEvent::CharacterSet(set) if !entered => {
                        if let Some(ch) = set.characters.first() {
                            let account = set.account.clone();
                            net.enter_world(ch.gid, &account);
                            entered = true;
                        }
                    }
                    SessionEvent::WorldObject { opcode, body }
                        if *opcode == Opcode::ITEM_SET_STATE =>
                    {
                        let mut rd = dereth_protocol::Reader::body(body);
                        arrived.push(ItemSetState::read(&mut rd).expect("a state change decodes"));
                    }
                    _ => {}
                }
            }
            let applied_before = objects.physics.stats.state_applied;
            objects.sync_physics(&store, &mut world);
            let applied_here = objects.physics.stats.state_applied - applied_before;
            v.delivered += arrived.len();
            v.applied +=
                usize::try_from(objects.stats.state_events - before).expect("a small count");
            v.written_to_a_body += usize::try_from(applied_here).expect("a small count");
            for m in &arrived {
                // The word is read from its one home, which is the table both of the client's own
                // readers of it read.
                if objects.physics_state(m.id) != Some(m.state) {
                    continue;
                }
                if Some(m.id) == objects.player() {
                    v.on_the_player += 1;
                    continue;
                }
                // Three answers, not two: "it landed on a body", "the object is the player's own"
                // and "the object has no body" are different, and only the first is the claim.
                match objects.physics.handle(m.id).and_then(|h| world.get(h)) {
                    None => v.no_body += 1,
                    Some(o) if o.state().0 == m.state => v.on_a_live_body += 1,
                    Some(_) => v.wrong_word += 1,
                }
            }
        }
        if v.delivered > 0 {
            println!(
                "set-state: {slug} -- {} delivered, {} accepted, {} on a live body, {} on \
                 the player, {} with no body, {} with the wrong word, {} written by the physics \
                 side",
                v.delivered,
                v.applied,
                v.on_a_live_body,
                v.on_the_player,
                v.no_body,
                v.wrong_word,
                v.written_to_a_body
            );
        }
        total.delivered += v.delivered;
        total.applied += v.applied;
        total.on_a_live_body += v.on_a_live_body;
        total.on_the_player += v.on_the_player;
        total.no_body += v.no_body;
        total.wrong_word += v.wrong_word;
        total.written_to_a_body += v.written_to_a_body;
    }

    // Every message the gate accepted reached its body with the word that was on the wire: the
    // three answers account for all of them and none carries the wrong word.
    let nothing_carries_the_wrong_word = total.wrong_word == 0;
    let all_accounted_for =
        total.on_a_live_body + total.on_the_player + total.no_body == total.applied;
    // And the physics side counted the ones that really moved a word, which cannot be more than
    // the ones the gate accepted.
    let the_physics_side_agrees =
        total.written_to_a_body > 0 && total.written_to_a_body <= total.applied;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "object.set-state.every-recorded-change-reaches-the-body-it-names",
        move |_| {
            total.delivered > 0
                && total.on_a_live_body > 0
                && nothing_carries_the_wrong_word
                && all_accounted_for
                && the_physics_side_agrees
        },
    );
}

// =============================================================================================
// object.set-state.a-body-the-shard-hides-stops-being-a-target-and-comes-back-in-place
// =============================================================================================

/// A body the shard hides is not something a swing can reach, and it comes back in place.
pub fn a_hidden_body_stops_being_a_target() {
    use dereth_physics::source::StaticLandSource;
    use dereth_primitives::{Frame, LandblockId, Position, Quat, Vec3};

    /// A word the recordings carry for an ordinary visible object.
    const VISIBLE: u32 = 0x0040_0408;
    const HIDDEN: u32 = VISIBLE | HIDDEN_PS;
    const REMOTE: ObjectId = ObjectId(0x7000_0252);
    const ATTACKER: ObjectId = ObjectId(0x7000_0253);

    let store = store();
    let target_at = Vec3::new(90.0, 91.0, 20.0);
    let attacker_at = Vec3::new(90.0, 90.0, 20.0);
    let cell = LandblockId(0xA9B4).cell(1);

    let placed_create = |id: ObjectId, state: u32, event: u16| -> Vec<u8> {
        dereth_protocol::write_body(&ItemCreateObject(ObjectCreatePayload {
            id,
            objdesc: dereth_protocol::types::ObjDesc::default(),
            physicsdesc: PhysicsDesc {
                bitfield: flags::SETUP | flags::POSITION | flags::OBJSCALE,
                state,
                setup_id: Some(PERSON_SETUP),
                object_scale: Some(1.0),
                position: Some(PositionWire {
                    objcell_id: cell.0,
                    frame: dereth_protocol::types::Frame {
                        origin: target_at.into(),
                        orientation: Quat::IDENTITY.into(),
                    },
                }),
                timestamps: dereth_protocol::types::PhysicsTimestamps {
                    state: event,
                    ..dereth_protocol::types::PhysicsTimestamps::default()
                },
                ..PhysicsDesc::default()
            },
            wdesc: dereth_protocol::types::PublicWeenieDesc::default(),
        }))
        .expect("encode")
    };
    let change = |id: ObjectId, state: u32, event: u16| -> Vec<u8> {
        dereth_protocol::write_body(&ItemSetState {
            id,
            state,
            timestamps: dereth_protocol::types::PhysicsEventStamp { instance: 0, event },
        })
        .expect("encode")
    };
    let cone = dereth_physics::detect::AttackCone {
        part_index: -1,
        left: (-0.5, 0.866_025_4),
        right: (0.5, 0.866_025_4),
        radius: 2.0,
        height: 1.0,
    };

    // The body the shard hides after it was created.
    let mut land = StaticLandSource::linear();
    land.add_flat_block(LandblockId(0xA9B4), 10);
    let mut world = PhysicsWorld::new(Arc::new(land));
    let mut stream = ObjectStream::new();
    feed(
        &mut stream,
        Opcode::ITEM_CREATE_OBJECT,
        placed_create(REMOTE, VISIBLE, 0),
        0.0,
    );
    stream.sync_physics(&store, &mut world);
    let remote = stream
        .physics
        .handle(REMOTE)
        .expect("the create made a body");
    let standing = world.get(remote).expect("live").position;
    let attacker_geometry = Arc::clone(&world.get(remote).expect("live").geometry);
    let attacker = world.create(ATTACKER, attacker_geometry, true);
    world.force_into_cell(
        attacker,
        &Position::new(cell, Frame::new(attacker_at, Quat::IDENTITY)),
    );
    let reaches = |w: &mut PhysicsWorld| w.attack(attacker, &cone).iter().any(|p| p.id == REMOTE);
    let starts_reachable = reaches(&mut world);

    feed(
        &mut stream,
        Opcode::ITEM_SET_STATE,
        change(REMOTE, HIDDEN, 1),
        1.0,
    );
    stream.sync_physics(&store, &mut world);
    let hidden_in_place = stream.physics.handle(REMOTE) == Some(remote)
        && world.get(remote).expect("still live").position == standing
        && world
            .get(remote)
            .expect("still live")
            .state()
            .ignores_collisions()
        && !world
            .get(remote)
            .expect("still live")
            .state()
            .reports_collisions();
    let hidden_is_out_of_reach = !reaches(&mut world);

    // An unchanged word is no change at all.
    let applied = stream.physics.stats.state_applied;
    feed(
        &mut stream,
        Opcode::ITEM_SET_STATE,
        change(REMOTE, HIDDEN, 2),
        2.0,
    );
    stream.sync_physics(&store, &mut world);
    let no_edge = stream.physics.stats.state_applied == applied;

    feed(
        &mut stream,
        Opcode::ITEM_SET_STATE,
        change(REMOTE, VISIBLE, 3),
        3.0,
    );
    stream.sync_physics(&store, &mut world);
    let back_in_place = stream.physics.handle(REMOTE) == Some(remote)
        && world.get(remote).expect("still live").position == standing
        && !world
            .get(remote)
            .expect("still live")
            .state()
            .ignores_collisions()
        && world
            .get(remote)
            .expect("still live")
            .state()
            .reports_collisions();
    let reachable_again = reaches(&mut world);

    // And a body that was hidden from the moment it was created is out of reach too, then comes
    // back without moving: an initially set bit is a real change and not constructor state.
    let mut land = StaticLandSource::linear();
    land.add_flat_block(LandblockId(0xA9B4), 10);
    let mut world2 = PhysicsWorld::new(Arc::new(land));
    let mut stream2 = ObjectStream::new();
    feed(
        &mut stream2,
        Opcode::ITEM_CREATE_OBJECT,
        placed_create(REMOTE, HIDDEN, 0),
        0.0,
    );
    stream2.sync_physics(&store, &mut world2);
    let remote2 = stream2
        .physics
        .handle(REMOTE)
        .expect("the create made a body");
    let where_it_stands = world2.get(remote2).expect("live").position;
    let geometry2 = Arc::clone(&world2.get(remote2).expect("live").geometry);
    let attacker2 = world2.create(ATTACKER, geometry2, true);
    world2.force_into_cell(
        attacker2,
        &Position::new(cell, Frame::new(attacker_at, Quat::IDENTITY)),
    );
    let reaches2 = |w: &mut PhysicsWorld| w.attack(attacker2, &cone).iter().any(|p| p.id == REMOTE);
    let born_hidden = !reaches2(&mut world2)
        && world2
            .get(remote2)
            .expect("live")
            .state()
            .ignores_collisions();
    feed(
        &mut stream2,
        Opcode::ITEM_SET_STATE,
        change(REMOTE, VISIBLE, 1),
        1.0,
    );
    stream2.sync_physics(&store, &mut world2);
    let unhidden_in_place = stream2.physics.handle(REMOTE) == Some(remote2)
        && world2.get(remote2).expect("live").position == where_it_stands
        && reaches2(&mut world2);

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "object.set-state.a-body-the-shard-hides-stops-being-a-target-and-comes-back-in-place",
        move |_| {
            starts_reachable
                && hidden_in_place
                && hidden_is_out_of_reach
                && no_edge
                && back_in_place
                && reachable_again
                && born_hidden
                && unhidden_in_place
        },
    );
}

// =============================================================================================
// objects.description.a-new-animation-table-alone-reaches-the-body
// =============================================================================================

/// A description that moves only the animation table reaches the body and rebuilds nothing else.
pub fn a_new_animation_table_alone_reaches_the_body() {
    use dereth_client_runtime::character::Character;
    use dereth_dat::DbType;
    use dereth_primitives::DataId;
    use {
        dereth_world_data::landblock::load_region, dereth_world_data::landblock::DEFAULT_LANDBLOCK,
    };

    /// The middle of the body's own block, where every movement scenario stands one up.
    const MID: f32 = dereth_physics::globals::BLOCK_LENGTH * 0.5;

    let store = store();
    let region = load_region(&store).expect("the region decodes");
    let body = |store: &Arc<RetailDatStore>| {
        Character::new(store, &region, DEFAULT_LANDBLOCK, (MID, MID)).expect("the body stands up")
    };
    // The table's own default style, which is the table's row and not anything this scenario
    // chose: an animation-level observable rather than the id this scenario wrote.
    let style = |c: &Character| c.driver().motion_table.table().map(|t| t.default_style());

    let mut c0 = body(&store);
    let own_setup = c0.setup_id();
    let own_table = c0.motion_table_id();
    // The premise: the body has a table before anything changes, or "the table changed" is
    // satisfied by a body that never had one.
    let before = style(&c0).expect("the body is created with an animation table installed");
    let parts_before = c0.driver().part_array.parts.len();
    let has_parts = parts_before > 0;

    // A table the data files hold whose default style differs from this body's. Found by asking
    // the data files, so the assertion below can discriminate: a second table that answered the
    // same style would make it pass for a body that ignored the change.
    let mut other: Option<(DataId, _)> = None;
    for id in store.ids_of(DbType::MTable) {
        if id == own_table {
            continue;
        }
        let mut probe = body(&store);
        if probe.set_setup_id(own_setup, Some(id)).is_ok() {
            let after = style(&probe);
            if after.is_some() && after != Some(before) {
                other = Some((id, after.expect("just checked")));
                break;
            }
        }
    }
    let (other_id, other_style) =
        other.expect("no animation table in the data files answers a different default style");

    // The change under test: the same setup the body already carries, a different table.
    let rebuilt = c0
        .set_setup_id(own_setup, Some(other_id))
        .expect("the table loads");
    let the_table_moved = c0.motion_table_id() == other_id
        && style(&c0) == Some(other_style)
        && c0.stats.motion_table_changes == 1;
    // …and the parts are untouched, which is what a body in the middle of an animation needs.
    let the_parts_did_not = !rebuilt
        && c0.setup_id() == own_setup
        && c0.stats.setup_changes == 0
        && c0.driver().part_array.parts.len() == parts_before;

    // Offering the same pair again does nothing at all -- rebuilding the manager would restart
    // the idle animation in the middle of its life.
    let mut c1 = body(&store);
    let idempotent = !c1.set_setup_id(own_setup, Some(own_table)).expect("the body's own pair")
        && c1.stats.motion_table_changes == 0
        && c1.stats.setup_changes == 0
        // The wire's "the description names none" is not a table either, and nor is no field.
        && !c1.set_setup_id(own_setup, Some(DataId(0))).expect("no table named")
        && !c1.set_setup_id(own_setup, None).expect("no table named")
        && c1.stats.motion_table_changes == 0
        && c1.motion_table_id() == own_table;
    let _ = c1
        .set_setup_id(own_setup, Some(other_id))
        .expect("the table loads");
    let the_new_arm_is_idempotent_too = !c1
        .set_setup_id(own_setup, Some(other_id))
        .expect("the table loads")
        && c1.stats.motion_table_changes == 1;

    // A table the data files do not hold is refused without disturbing the body.
    let mut c2 = body(&store);
    let missing = DataId(0x0900_FFFF);
    let really_missing = store.read_typed(DbType::MTable, missing).is_err();
    let was = style(&c2);
    let err = c2
        .set_setup_id(own_setup, Some(missing))
        .expect_err("a missing table must not build");
    let refused_cleanly = c2.motion_table_id() == own_table
        && style(&c2) == was
        && c2.stats.motion_table_changes == 0
        && matches!(err, dereth_client_runtime::character::CharacterError::NoMotionTable(id) if id == missing);

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "objects.description.a-new-animation-table-alone-reaches-the-body",
        move |_| {
            has_parts
                && the_table_moved
                && the_parts_did_not
                && idempotent
                && the_new_arm_is_idempotent_too
                && really_missing
                && refused_cleanly
        },
    );
}

// =============================================================================================
// objects.parent.a-holder-holds-only-where-its-body-has-a-place
// =============================================================================================

/// The holding place is the gate, and it is the holder's own parts that answer.
pub fn a_holder_holds_only_where_its_body_has_a_place() {
    const HOLDER: ObjectId = ObjectId(0x7011_0001);
    const CHILD: ObjectId = ObjectId(0x7011_0002);

    let store = store();
    let attaches_at = |location: u32| -> bool {
        let mut s = ObjectStream::with_store(Arc::clone(&store));
        feed(
            &mut s,
            Opcode::ITEM_CREATE_OBJECT,
            create_body(HOLDER, Some(PERSON_SETUP), None, None),
            0.0,
        );
        feed(
            &mut s,
            Opcode::ITEM_CREATE_OBJECT,
            create_body(CHILD, Some(PERSON_SETUP), None, Some((HOLDER, location))),
            0.0,
        );
        assert!(
            s.presence(CHILD).is_some(),
            "the premise: the object itself still arrives"
        );
        s.world.physics_parent(CHILD) == Some((HOLDER, location))
    };
    // Three links against the same pair, so the only thing that varies is the place asked for.
    let a_real_place = attaches_at(RIGHT_HAND);
    let not_a_place = !attaches_at(NOT_A_PLACE);
    // The place a check that only ever asked for one could not tell from no gate at all.
    let a_hole_in_the_table = !attaches_at(7);

    // With no data files open the client cannot answer the question at all, so it refuses every
    // attachment -- the identical wire bytes, and no link.
    let mut bare = ObjectStream::new();
    feed(
        &mut bare,
        Opcode::ITEM_CREATE_OBJECT,
        create_body(HOLDER, Some(PERSON_SETUP), None, None),
        0.0,
    );
    feed(
        &mut bare,
        Opcode::ITEM_CREATE_OBJECT,
        create_body(CHILD, Some(PERSON_SETUP), None, Some((HOLDER, RIGHT_HAND))),
        0.0,
    );
    let assetless_refuses =
        bare.presence(CHILD).is_some() && bare.world.physics_parent(CHILD).is_none();

    // And a holder that really does hold a child keeps that child's placeholder alive past the
    // deadline it would otherwise have died at, while a refused attachment leaves the timer
    // running.
    let lifetime = |location: u32| -> (Option<ServerTime>, bool) {
        let mut s = ObjectStream::with_store(Arc::clone(&store));
        let mut holder = ObjectCreatePayload {
            id: HOLDER,
            objdesc: dereth_protocol::types::ObjDesc::default(),
            physicsdesc: PhysicsDesc {
                bitfield: flags::SETUP | flags::CHILDREN,
                setup_id: Some(PERSON_SETUP),
                children: Some(vec![dereth_protocol::types::physicsdesc::ChildLink {
                    child_id: CHILD,
                    location_id: location,
                }]),
                ..PhysicsDesc::default()
            },
            wdesc: dereth_protocol::types::PublicWeenieDesc::default(),
        };
        holder.physicsdesc.timestamps.instance = 1;
        feed(
            &mut s,
            Opcode::ITEM_CREATE_OBJECT,
            dereth_protocol::write_body(&ItemCreateObject(holder)).expect("encode"),
            1000.0,
        );
        assert!(
            s.world.tables.null_physics.contains_key(CHILD),
            "the child is a placeholder"
        );
        let deadline = s.world.tables.doomed.get(CHILD).copied();
        s.use_time::<dereth_client_net::client_session::testing::MockTransport>(
            ServerTime(1_025.001),
            None,
        );
        (deadline, s.world.tables.null_physics.contains_key(CHILD))
    };
    let (accepted_deadline, accepted_survives) = lifetime(RIGHT_HAND);
    let (refused_deadline, refused_survives) = lifetime(NOT_A_PLACE);
    let an_accepted_child_outlives_it = accepted_deadline.is_none() && accepted_survives;
    let a_refused_one_does_not = refused_deadline == Some(ServerTime(1_025.0)) && !refused_survives;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "objects.parent.a-holder-holds-only-at-a-place-its-own-body-has",
        move |_| {
            a_real_place
                && not_a_place
                && a_hole_in_the_table
                && assetless_refuses
                && an_accepted_child_outlives_it
                && a_refused_one_does_not
        },
    );
}

// =============================================================================================
// objects.held-object.has-no-body-and-leaves-the-cell-with-its-holder
// =============================================================================================

/// A held object has no body of its own, and it leaves the room with whatever is holding it.
pub fn a_held_object_has_no_body_of_its_own() {
    use dereth_primitives::{LandblockId, Vec3};
    use dereth_world_data::land_source::DatLandSource;

    const HOLDER: ObjectId = ObjectId(0x7011_0001);
    const CHILD: ObjectId = ObjectId(0x7011_0002);

    let store = store();

    // 1. The room the holder is released from takes the child with it: the child is not released
    //    in its own right, and nothing on that path detaches it.
    let cell = CellId(0xA9B4_0100);
    let mut s = ObjectStream::with_store(Arc::clone(&store));
    feed(
        &mut s,
        Opcode::ITEM_CREATE_OBJECT,
        create_body(HOLDER, Some(PERSON_SETUP), Some(cell), None),
        0.0,
    );
    s.world.publish_physics_cell(HOLDER, Some(cell));
    feed(
        &mut s,
        Opcode::ITEM_CREATE_OBJECT,
        create_body(
            CHILD,
            Some(PERSON_SETUP),
            Some(cell),
            Some((HOLDER, RIGHT_HAND)),
        ),
        0.0,
    );
    let premise = s.world.physics(CHILD).and_then(|p| p.cell) == Some(cell)
        && s.world.physics_parent(CHILD) == Some((HOLDER, RIGHT_HAND));
    let left = s.world.release_obj_cell(cell, ServerTime(10.0));
    let only_the_holder_is_released = left == vec![HOLDER];
    let the_child_went_with_it = s.world.physics(CHILD).and_then(|p| p.cell).is_none()
        && s.world.tables.doomed.contains_key(CHILD)
        && s.world.physics_parent(CHILD) == Some((HOLDER, RIGHT_HAND));

    // 2. And it has no body at all: the synchronisation skips a held object, and destroys the body
    //    of one that becomes held. Measured from the arena rather than asserted from the source.
    let region = dereth_world_data::landblock::load_region(&store).expect("the region decodes");
    let land =
        Arc::new(DatLandSource::new(Arc::clone(&store), &region).expect("the land source opens"));
    let block = LandblockId(0x7F03);
    let room = CellId(0x7F03_0100);
    land.load_block_cells(block);
    let geometry = land.env_cell(room).expect("the installed academy interior");
    let mut low = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
    let mut high = Vec3::new(f32::MIN, f32::MIN, f32::MIN);
    for v in geometry.physics_polygons.iter().flat_map(|p| &p.vertices) {
        low = Vec3::new(low.x.min(v.x), low.y.min(v.y), low.z.min(v.z));
        high = Vec3::new(high.x.max(v.x), high.y.max(v.y), high.z.max(v.z));
    }
    let centre = Vec3::new((low.x + high.x) * 0.5, (low.y + high.y) * 0.5, low.z + 1.0);
    let origin = dereth_physics::math::localtoglobal(&geometry.frame, centre);
    let mut physics = PhysicsWorld::new(Arc::clone(&land) as Arc<dyn LandSource>);
    let mut s = ObjectStream::with_store(Arc::clone(&store));
    let holder = ObjectCreatePayload {
        id: HOLDER,
        objdesc: dereth_protocol::types::ObjDesc::default(),
        physicsdesc: PhysicsDesc {
            bitfield: flags::SETUP | flags::POSITION,
            setup_id: Some(PERSON_SETUP),
            position: Some(PositionWire {
                objcell_id: room.0,
                frame: dereth_protocol::types::Frame {
                    origin: origin.into(),
                    orientation: geometry.frame.rotation.into(),
                },
            }),
            ..PhysicsDesc::default()
        },
        wdesc: dereth_protocol::types::PublicWeenieDesc::default(),
    };
    feed(
        &mut s,
        Opcode::ITEM_CREATE_OBJECT,
        dereth_protocol::write_body(&ItemCreateObject(holder)).expect("encode"),
        1000.0,
    );
    s.sync_physics_at(&store, &mut physics, LocalTime(1000.0));
    // The premise: an unheld object with a position does get a body, so "no body" below is a
    // result and not the instrument failing.
    let the_holder_has_one =
        s.physics.handle(HOLDER).is_some() && physics.by_object_id(HOLDER).is_some();
    feed(
        &mut s,
        Opcode::ITEM_CREATE_OBJECT,
        create_body(CHILD, Some(PERSON_SETUP), None, Some((HOLDER, RIGHT_HAND))),
        1000.0,
    );
    let attached = s.world.physics_parent(CHILD) == Some((HOLDER, RIGHT_HAND));
    s.sync_physics_at(&store, &mut physics, LocalTime(1001.0));
    let the_child_has_none =
        s.physics.handle(CHILD).is_none() && physics.by_object_id(CHILD).is_none();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "objects.held-object.has-no-body-and-leaves-the-cell-with-its-holder",
        move |_| {
            premise
                && only_the_holder_is_released
                && the_child_went_with_it
                && the_holder_has_one
                && attached
                && the_child_has_none
        },
    );
}

// =============================================================================================
// objects.position.an-older-teleport-generation-puts-the-position-stamp-back
// =============================================================================================

/// A position report from an older teleport generation is undone completely.
#[allow(clippy::too_many_lines)]
pub fn an_older_teleport_puts_the_position_stamp_back() {
    use dereth_client_runtime::app::player_timestamps;
    use dereth_client_runtime::objects::PlayerMotionDispatch;
    use dereth_primitives::{Frame, Position};
    use dereth_protocol::movement::{position_flags, MovementPositionEvent, PositionPack};
    use dereth_protocol::types::{Origin, PhysicsTimestamps};

    const ID: ObjectId = ObjectId(0x7000_0021);
    const HOLDER: ObjectId = ObjectId(0x7000_0022);
    const CELL: u32 = 0x7F03_01B0;

    let store = store();
    // The attachment really is validated against the data files, which is why this needs them.
    let seed = |player: bool, position: u16, teleport: u16, parent: bool| -> ObjectStream {
        let mut objects = ObjectStream::with_store(Arc::clone(&store));
        if parent {
            feed(
                &mut objects,
                Opcode::ITEM_CREATE_OBJECT,
                create_body(HOLDER, Some(PERSON_SETUP), None, None),
                0.0,
            );
        }
        if player {
            objects.apply_event(&SessionEvent::PlayerCreated(ID), LocalTime(0.0));
        }
        let mut desc = PhysicsDesc {
            bitfield: flags::POSITION | flags::ANIMFRAME | flags::SETUP,
            setup_id: Some(PERSON_SETUP),
            position: Some(PositionWire {
                objcell_id: CELL,
                frame: dereth_protocol::types::Frame {
                    origin: dereth_protocol::types::Vec3 {
                        x: 1.0,
                        y: 2.0,
                        z: 3.0,
                    },
                    ..Default::default()
                },
            }),
            animframe_id: Some(52),
            timestamps: PhysicsTimestamps {
                position,
                teleport,
                force_position: 20,
                server_controlled_move: 7,
                instance: 3,
                ..PhysicsTimestamps::default()
            },
            ..PhysicsDesc::default()
        };
        if parent {
            desc.bitfield |= flags::PARENT;
            desc.parent = Some((HOLDER, RIGHT_HAND));
        }
        feed(
            &mut objects,
            Opcode::ITEM_CREATE_OBJECT,
            dereth_protocol::write_body(&ItemCreateObject(ObjectCreatePayload {
                id: ID,
                objdesc: dereth_protocol::types::ObjDesc::default(),
                physicsdesc: desc,
                wdesc: dereth_protocol::types::PublicWeenieDesc::default(),
            }))
            .expect("encode"),
            0.0,
        );
        assert!(
            objects.take_player_motion_dispatches().is_empty(),
            "a create that carries no motion is no teleport completion"
        );
        objects
    };
    let incoming = |position: u16, teleport: u16, force: u16, x: f32| MovementPositionEvent {
        id: ID,
        position: PositionPack {
            flags: position_flags::HAS_PLACEMENT_ID,
            origin: Origin {
                objcell_id: CELL,
                origin: dereth_protocol::types::Vec3 { x, y: 4.0, z: 5.0 },
            },
            placement_id: Some(3),
            instance_timestamp: 3,
            position_timestamp: position,
            teleport_timestamp: teleport,
            force_position_timestamp: force,
            ..Default::default()
        },
    };
    let apply = |objects: &mut ObjectStream, m: &MovementPositionEvent| {
        feed(
            objects,
            Opcode::MOVEMENT_POSITION_EVENT,
            dereth_protocol::write_body(m).expect("encode"),
            1.0,
        );
    };
    let destination = |m: &MovementPositionEvent| {
        Position::new(
            CellId(CELL),
            Frame::new(
                m.position.origin.origin.into(),
                m.position.orientation.into(),
            ),
        )
    };

    // 1. The rollback is complete, and it happens before anything else the message would have
    //    done -- the position, what is holding the object, and how it is posed.
    let mut rollback = true;
    for player in [false, true] {
        for parent in [false, true] {
            let mut objects = seed(player, 100, 10, parent);
            let before = objects.presence(ID).expect("created").clone();
            apply(&mut objects, &incoming(101, 9, 20, 99.0));
            let after = objects.presence(ID).expect("still here");
            rollback &= after.position_ts == 100
                && after.position == before.position
                && after.parent == before.parent
                && after.placement == 52
                && after.pending_placement.is_none()
                && player_timestamps(Some(after)) == player_timestamps(Some(&before))
                && objects.stats.position_updates == 0
                && objects.stats.position_teleport_rollbacks == 1
                && objects.stats.stale_positions == 0
                && objects.stats.teleport_stamps == 0
                && objects.take_player_motion_dispatches().is_empty();
            // The very stamp that was refused is usable again: merely keeping the position while
            // consuming the stamp would reject this one.
            let ordinary = incoming(101, 10, 20, 6.0);
            apply(&mut objects, &ordinary);
            let p = objects.presence(ID).expect("still here");
            rollback &= p.position_ts == 101
                && p.position == Some(destination(&ordinary))
                && p.parent.is_none()
                && p.pending_placement == Some(3)
                && p.teleport_ts == 10
                && objects.stats.position_updates == 1
                && objects.take_player_motion_dispatches().is_empty();
        }
    }

    // 2. A teleport is completed exactly once, and a newer teleport cannot carry a stale position
    //    past the gate.
    let mut objects = seed(true, 100, 10, false);
    let standing = objects.presence(ID).expect("created").position;
    let mut exactly_once = true;
    for pos in [100, 99] {
        apply(&mut objects, &incoming(pos, 11, 20, 99.0));
        let p = objects.presence(ID).expect("still here");
        exactly_once &= p.position == standing
            && p.teleport_ts == 10
            && objects.take_player_motion_dispatches().is_empty();
    }
    let first = incoming(101, 11, 20, 6.0);
    apply(&mut objects, &first);
    apply(&mut objects, &first);
    apply(&mut objects, &incoming(102, 10, 20, 99.0));
    exactly_once &= objects.presence(ID).expect("here").position == Some(destination(&first))
        && objects.presence(ID).expect("here").position_ts == 101;
    let second = incoming(102, 12, 20, 7.0);
    apply(&mut objects, &second);
    let ordinary = incoming(103, 12, 20, 8.0);
    apply(&mut objects, &ordinary);
    exactly_once &= objects.presence(ID).expect("here").position == Some(destination(&ordinary))
        && objects.presence(ID).expect("here").position_ts == 103
        && objects.stats.position_updates == 3
        && objects.stats.position_teleport_rollbacks == 1
        && objects.stats.stale_positions == 3
        && objects.stats.teleport_stamps == 2
        && objects.stats.player_teleports == 2;
    let journal = objects.take_player_motion_dispatches();
    exactly_once &= journal.len() == 2;
    for (entry, source) in journal.iter().zip([first, second]) {
        let PlayerMotionDispatch::Teleport {
            position,
            timestamps,
        } = entry
        else {
            panic!("a teleport completion is the only thing this drive produces")
        };
        // The completion carries the position the teleport named, not the last one the body
        // happened to reach.
        exactly_once &= *position == destination(&source)
            && timestamps.teleport == source.position.teleport_timestamp
            && (
                timestamps.instance,
                timestamps.server_control,
                timestamps.force_position,
            ) == (3, 7, 20);
    }
    exactly_once &= objects.take_player_motion_dispatches().is_empty()
        && objects.take_player_teleport().is_none();

    // 3. The forced-position generation belongs to the player's own body and moves before either
    //    rejection path, and a stale one does not block an otherwise acceptable report.
    let mut force_is_the_players = true;
    for player in [false, true] {
        let mut objects = seed(player, 100, 10, false);
        let standing = objects.presence(ID).expect("created").position;
        for (position, force) in [(100, 21), (101, 22)] {
            apply(&mut objects, &incoming(position, 9, force, 99.0));
            let p = objects.presence(ID).expect("still here");
            force_is_the_players &= p.position_ts == 100
                && p.position == standing
                && p.teleport_ts == 10
                && p.force_position_ts == if player { force } else { 20 }
                && player_timestamps(Some(p)).force_position == if player { force } else { 20 }
                && objects.take_player_motion_dispatches().is_empty();
        }
        force_is_the_players &= objects.stats.force_position_stamps == u64::from(player) * 2
            && objects.stats.position_teleport_rollbacks == 1
            && objects.stats.stale_positions == 1;
        let ordinary = incoming(101, 10, 19, 6.0);
        apply(&mut objects, &ordinary);
        force_is_the_players &= objects.presence(ID).expect("here").position
            == Some(destination(&ordinary))
            && objects.presence(ID).expect("here").force_position_ts
                == if player { 22 } else { 20 };
    }

    // 4. The comparison treats a difference of half the stamp range or more as a wrap.
    let mut wraps = true;
    for (stored, arriving, accepted, completion) in [
        (0u16, 0xFFFFu16, false, false),
        (0xFFFF, 0, true, true),
        (0, 0x8000, false, false),
        (0x8000, 0, true, true),
        (5, 5, true, false),
    ] {
        let mut objects = seed(true, 0xFFFF, stored, false);
        let standing = objects.presence(ID).expect("created").position;
        let next = incoming(0, arriving, 20, 6.0);
        apply(&mut objects, &next);
        let p = objects.presence(ID).expect("still here");
        wraps &= p.position_ts == if accepted { 0 } else { 0xFFFF }
            && p.position
                == if accepted {
                    Some(destination(&next))
                } else {
                    standing
                }
            && p.teleport_ts == if completion { arriving } else { stored }
            && objects.take_player_motion_dispatches().len() == usize::from(completion)
            && objects.take_player_motion_dispatches().is_empty();
    }

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "objects.position.an-older-teleport-generation-puts-the-position-stamp-back",
        move |_| rollback && exactly_once && force_is_the_players && wraps,
    );
}

// =============================================================================================
// objects.unloaded-cell.a-body-is-rescued-by-the-next-position-the-shard-sends
// =============================================================================================

/// A body whose room the client unloaded is rescued by the next position the shard sends for it.
#[allow(clippy::too_many_lines)]
pub fn an_unloaded_body_is_rescued_by_a_position() {
    use dereth_client_net::client_session::testing::MockTransport;
    use dereth_primitives::{Frame, Position, Vec3};
    use dereth_protocol::movement::{position_flags, MovementPositionEvent, PositionPack};
    use dereth_protocol::objects::ItemParentEvent;
    use dereth_world_data::land_source::DatLandSource;

    const PARENT: ObjectId = ObjectId(0x7000_6011);
    const CHILD: ObjectId = ObjectId(0x8000_6011);
    const OTHER: ObjectId = ObjectId(0x7000_6012);
    /// The flag the shard sets when a report says the body is on the ground; a report without it
    /// moves nothing, by the client's own rule.
    const GROUNDED: u32 = position_flags::IS_GROUNDED;

    let store = store();
    let region = dereth_world_data::landblock::load_region(&store).expect("the region decodes");
    let land =
        Arc::new(DatLandSource::new(Arc::clone(&store), &region).expect("the land source opens"));
    // Two real interiors, so "into another room that is loaded" is a real room.
    let mut places = Vec::new();
    for cell in [CellId(0x7F03_0100), CellId(0x8602_0100)] {
        land.load_block_cells(cell.landblock());
        let geom = land.env_cell(cell).expect("the installed academy interior");
        let mut low = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
        let mut high = Vec3::new(f32::MIN, f32::MIN, f32::MIN);
        for v in geom.physics_polygons.iter().flat_map(|p| &p.vertices) {
            low = Vec3::new(low.x.min(v.x), low.y.min(v.y), low.z.min(v.z));
            high = Vec3::new(high.x.max(v.x), high.y.max(v.y), high.z.max(v.z));
        }
        assert!(
            high.x > low.x && high.y > low.y && high.z > low.z,
            "the room has an extent"
        );
        places.push(Position::new(
            cell,
            Frame::new(
                dereth_physics::math::localtoglobal(
                    &geom.frame,
                    Vec3::new((low.x + high.x) * 0.5, (low.y + high.y) * 0.5, low.z + 1.0),
                ),
                geom.frame.rotation,
            ),
        ));
    }
    let (old, fresh) = (places[0], places[1]);

    let placed = |id: ObjectId, pos: Option<Position>, children: bool| -> Vec<u8> {
        let mut desc = PhysicsDesc {
            bitfield: flags::SETUP,
            setup_id: Some(PERSON_SETUP),
            ..PhysicsDesc::default()
        };
        if let Some(p) = pos {
            desc.bitfield |= flags::POSITION;
            desc.position = Some(PositionWire {
                objcell_id: p.cell.0,
                frame: dereth_protocol::types::Frame {
                    origin: p.frame.origin.into(),
                    orientation: p.frame.rotation.into(),
                },
            });
        }
        if children {
            desc.bitfield |= flags::CHILDREN;
            desc.children = Some(vec![dereth_protocol::types::physicsdesc::ChildLink {
                child_id: CHILD,
                location_id: RIGHT_HAND,
            }]);
        }
        dereth_protocol::write_body(&ItemCreateObject(ObjectCreatePayload {
            id,
            objdesc: dereth_protocol::types::ObjDesc::default(),
            physicsdesc: desc,
            wdesc: dereth_protocol::types::PublicWeenieDesc::default(),
        }))
        .expect("encode")
    };
    let report = |to: Position, stamp: u16, teleport: u16, flags_word: u32| -> Vec<u8> {
        let event = MovementPositionEvent {
            id: PARENT,
            position: PositionPack {
                flags: flags_word,
                origin: dereth_protocol::types::Origin {
                    objcell_id: to.cell.0,
                    origin: to.frame.origin.into(),
                },
                orientation: to.frame.rotation.into(),
                position_timestamp: stamp,
                teleport_timestamp: teleport,
                ..Default::default()
            },
        };
        dereth_protocol::write_body(&event).expect("encode")
    };

    /// A body standing in `old`, holding a child, whose room has then been unloaded.
    struct Station {
        physical: PhysicsWorld,
        stream: ObjectStream,
        handle: dereth_physics::PhysHandle,
    }

    let build = || -> Station {
        // Both rooms are loaded again at the start of each run: this scenario's whole subject is
        // a room being unloaded, and the land source is shared between its three runs.
        land.load_block_cells(old.cell.landblock());
        land.load_block_cells(fresh.cell.landblock());
        let mut physical = PhysicsWorld::new(Arc::clone(&land) as Arc<dyn LandSource>);
        let mut stream = ObjectStream::with_store(Arc::clone(&store));
        feed(
            &mut stream,
            Opcode::ITEM_CREATE_OBJECT,
            placed(CHILD, None, false),
            1000.0,
        );
        // A second holder, so the attachment that is refused below is refused for the place it
        // names and not because the client has never heard of the thing doing the holding.
        feed(
            &mut stream,
            Opcode::ITEM_CREATE_OBJECT,
            placed(OTHER, None, false),
            1000.0,
        );
        feed(
            &mut stream,
            Opcode::ITEM_CREATE_OBJECT,
            placed(PARENT, Some(old), true),
            1000.0,
        );
        stream.sync_physics_at(&store, &mut physical, LocalTime(1000.0));
        let handle = stream
            .physics
            .handle(PARENT)
            .expect("the create made a body");
        assert_eq!(physical.get(handle).expect("live").cell, Some(old.cell));
        assert_eq!(
            stream.world.physics(CHILD).expect("held").cell,
            Some(old.cell)
        );
        assert_eq!(
            stream.presence(CHILD).expect("held").parent,
            Some((PARENT, RIGHT_HAND))
        );
        // …and then the room goes.
        assert_eq!(
            stream.release_block_obj_cells_with_physics(
                old.cell.landblock(),
                LocalTime(1001.0),
                &mut physical
            ),
            1
        );
        assert!(land.release_visible_cells(old.cell.landblock()).0 > 0);
        stream.sync_physics_at(&store, &mut physical, LocalTime(1001.1));
        assert_eq!(physical.get(handle).expect("live").cell, None);
        for id in [PARENT, CHILD] {
            assert_eq!(
                stream.world.tables.doomed.get(id),
                Some(&ServerTime(1026.0))
            );
        }
        Station {
            physical,
            stream,
            handle,
        }
    };

    // 1. A fresh grounded report into the room that is still loaded rescues it, and the room it
    //    left is not reloaded to do it.
    let mut s = build();
    feed(
        &mut s.stream,
        Opcode::MOVEMENT_POSITION_EVENT,
        report(fresh, 1, 0, GROUNDED),
        1002.0,
    );
    s.stream
        .sync_physics_at(&store, &mut s.physical, LocalTime(1002.0));
    let entered = s.physical.get(s.handle).expect("live").cell == Some(fresh.cell)
        && s.stream.world.tables.doomed.get(PARENT).is_none()
        && s.stream.world.tables.doomed.get(CHILD).is_none()
        && s.stream
            .world
            .tables
            .lost_cells
            .iter()
            .all(|(_, lost)| !lost.objects.contains(&PARENT))
        && land.env_cell(old.cell).is_none()
        && s.stream.world.physics(CHILD).and_then(|p| p.cell) == Some(fresh.cell)
        && s.stream
            .world
            .weenie(CHILD)
            .is_some_and(|w| w.phys_has_cell);
    // …and it survives past the deadline it was on, without depending on the periodic refresh.
    s.stream.world.tables.visible.clear();
    s.stream
        .use_time::<MockTransport>(ServerTime(1_026.001), None);
    let survived =
        s.stream.world.physics(PARENT).is_some() && s.stream.world.physics(CHILD).is_some();

    // 2. A report that is not grounded moves no body, although the shard's word is still taken.
    let mut s = build();
    feed(
        &mut s.stream,
        Opcode::MOVEMENT_POSITION_EVENT,
        report(fresh, 1, 0, GROUNDED),
        1002.0,
    );
    s.stream
        .sync_physics_at(&store, &mut s.physical, LocalTime(1002.0));
    let standing = s.physical.get(s.handle).expect("live").position;
    let mut nudged = fresh;
    nudged.frame.origin.x += 0.125;
    let arms = |s: &Station| {
        let st = &s.stream.physics.stats;
        (
            st.teleport_arm,
            st.no_contact_arm,
            st.interpolate_arm,
            st.snap_arm,
        )
    };
    let before = arms(&s);
    feed(
        &mut s.stream,
        Opcode::MOVEMENT_POSITION_EVENT,
        report(nudged, 2, 0, 0),
        1003.0,
    );
    s.stream
        .sync_physics_at(&store, &mut s.physical, LocalTime(1003.0));
    let ungrounded_moves_nothing = arms(&s) == (before.0, before.1 + 1, before.2, before.3)
        && s.physical.get(s.handle).expect("live").position == standing
        && s.stream.presence(PARENT).expect("here").position_ts == 2
        && s.stream.presence(PARENT).expect("here").server_position == Some(nudged);

    // 3. A report the client refuses rescues nothing, and a genuine one afterwards still does.
    let mut s = build();
    let clock = s.physical.get(s.handle).expect("live").update_time;
    // An attachment the holder's parts cannot answer still advances the position stamp.
    let parent_event = ItemParentEvent {
        creature: OTHER,
        item: PARENT,
        location: NOT_A_PLACE,
        placement_frame: s.stream.presence(PARENT).expect("here").placement,
        timestamps: dereth_protocol::types::PhysicsEventStamp {
            instance: 0,
            event: 1,
        },
    };
    feed(
        &mut s.stream,
        Opcode::ITEM_PARENT_EVENT,
        dereth_protocol::write_body(&parent_event).expect("encode"),
        1002.0,
    );
    s.stream
        .sync_physics_at(&store, &mut s.physical, LocalTime(1002.0));
    let mut refused_rescues_nothing = s.stream.presence(PARENT).expect("here").position_ts == 1
        && s.stream.presence(PARENT).expect("here").parent.is_none()
        && s.physical.get(s.handle).expect("live").update_time == clock;
    for (stamp, teleport) in [(1u16, 0u16), (0, 0), (2, u16::MAX)] {
        feed(
            &mut s.stream,
            Opcode::MOVEMENT_POSITION_EVENT,
            report(fresh, stamp, teleport, GROUNDED),
            1003.0,
        );
        s.stream
            .sync_physics_at(&store, &mut s.physical, LocalTime(1003.0));
        refused_rescues_nothing &= s.stream.presence(PARENT).expect("here").position_ts == 1
            && s.physical.get(s.handle).expect("live").cell.is_none()
            && s.physical.get(s.handle).expect("live").update_time == clock
            && s.stream.world.tables.doomed.get(PARENT) == Some(&ServerTime(1026.0));
    }
    feed(
        &mut s.stream,
        Opcode::MOVEMENT_POSITION_EVENT,
        report(fresh, 2, 0, GROUNDED),
        1004.0,
    );
    s.stream
        .sync_physics_at(&store, &mut s.physical, LocalTime(1004.0));
    let a_genuine_one_still_does = s.stream.world.tables.doomed.get(PARENT).is_none()
        && s.physical.get(s.handle).expect("live").update_time == 1004.0;

    // 4. And the rescue is spent when it is used: a body that has entered a room is placed by a
    //    later report without going through the no-room tail again, so an unload afterwards is a
    //    fresh one and needs its own rescue.
    let mut s = build();
    feed(
        &mut s.stream,
        Opcode::MOVEMENT_POSITION_EVENT,
        report(fresh, 1, 1, GROUNDED),
        1002.0,
    );
    s.stream
        .sync_physics_at(&store, &mut s.physical, LocalTime(1002.0));
    let rescued_at = s.physical.get(s.handle).expect("live").update_time;
    let mut nudged = fresh;
    nudged.frame.origin.x += 0.125;
    feed(
        &mut s.stream,
        Opcode::MOVEMENT_POSITION_EVENT,
        report(nudged, 2, 1, GROUNDED),
        1003.0,
    );
    s.stream
        .sync_physics_at(&store, &mut s.physical, LocalTime(1003.0));
    s.stream
        .sync_physics_at(&store, &mut s.physical, LocalTime(1004.0));
    let placed_without_the_tail =
        rescued_at == 1002.0 && s.physical.get(s.handle).expect("live").update_time == 1002.0;
    // The room it is in now goes too.
    let released = s.stream.release_block_obj_cells_with_physics(
        fresh.cell.landblock(),
        LocalTime(1005.0),
        &mut s.physical,
    ) == 1
        && land.release_visible_cells(fresh.cell.landblock()).0 > 0;
    s.stream
        .sync_physics_at(&store, &mut s.physical, LocalTime(1006.0));
    let mut a_second_unload_needs_its_own_rescue = released
        && s.physical.get(s.handle).expect("live").cell.is_none()
        && s.physical.get(s.handle).expect("live").update_time == 1002.0;
    for id in [PARENT, CHILD] {
        a_second_unload_needs_its_own_rescue &=
            s.stream.world.tables.doomed.get(id) == Some(&ServerTime(1030.0));
    }
    // …and a later genuine report into the first room, loaded again, rescues it once more.
    land.load_block_cells(old.cell.landblock());
    feed(
        &mut s.stream,
        Opcode::MOVEMENT_POSITION_EVENT,
        report(old, 3, 2, GROUNDED),
        1007.0,
    );
    s.stream
        .sync_physics_at(&store, &mut s.physical, LocalTime(1007.0));
    let rescued_again = s.physical.get(s.handle).expect("live").cell == Some(old.cell)
        && s.physical.get(s.handle).expect("live").update_time == 1007.0
        && s.stream.world.tables.doomed.get(PARENT).is_none()
        && s.stream.world.tables.doomed.get(CHILD).is_none();
    s.stream.world.tables.visible.clear();
    s.stream
        .use_time::<MockTransport>(ServerTime(1_030.001), None);
    let and_survives =
        s.stream.world.physics(PARENT).is_some() && s.stream.world.physics(CHILD).is_some();

    for (what, ok) in [
        ("the body enters the loaded room", entered),
        ("and survives past the timer it was on", survived),
        (
            "an ungrounded report moves nothing",
            ungrounded_moves_nothing,
        ),
        ("a refused report rescues nothing", refused_rescues_nothing),
        (
            "a genuine one afterwards still does",
            a_genuine_one_still_does,
        ),
        (
            "a placed body is not placed through the no-room tail",
            placed_without_the_tail,
        ),
        (
            "a second unload needs its own rescue",
            a_second_unload_needs_its_own_rescue,
        ),
        ("and gets one", rescued_again && and_survives),
    ] {
        println!("rescue: {what}: {ok}");
    }

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "objects.unloaded-cell.a-body-is-rescued-by-the-next-position-the-shard-sends",
        move |_| {
            entered
                && survived
                && ungrounded_moves_nothing
                && refused_rescues_nothing
                && a_genuine_one_still_does
                && placed_without_the_tail
                && a_second_unload_needs_its_own_rescue
                && rescued_again
                && and_survives
        },
    );
}

// =============================================================================================
// The toolbar's selection: one selection change is one edge
// =============================================================================================
//
// The shipped game screen, built from the retail data files, with the heads-up display driving it
// by hand. **The display outlives the screen on purpose**: a change of screen destroys and rebuilds
// the game screen while the host-side display survives, and that is the state in which one
// remembered selection and two of them tell each other apart.
//
// The subjects are made here rather than replayed. A creature and a stack picked out of a
// recording would do, but the *type* is emphatically not the gate -- what makes a creature the
// right subject is that it carries no stack size at all, so nothing can re-seed a stale edge for
// it. That property is what is built.

/// The selected-object field, the strip the stack splitter lives in.
const SEL_OBJECT_FIELD: dereth_ui::ElementId = dereth_ui::ElementId(0x1000_019E);

/// The shipped game screen and the display that drives it.
struct Toolbar {
    shell: dereth_client_shell::ui::UiShell,
    hud: dereth_client_shell::hud::Hud,
    objects: ObjectStream,
    serial: u64,
    field: Option<dereth_ui::ElemHandle>,
    entry: Option<dereth_ui::ElemHandle>,
    slider: Option<dereth_ui::ElemHandle>,
    now: f64,
}

impl Toolbar {
    fn new(objects: ObjectStream) -> Self {
        let store = store();
        let shell = dereth_client_shell::ui::UiShell::new(&store, (800, 600))
            .expect("the shell comes up on the retail data files");
        let mut hud = dereth_client_shell::hud::Hud::new();
        hud.sync(&objects, None);
        let mut t = Toolbar {
            shell,
            hud,
            objects,
            serial: 1,
            field: None,
            entry: None,
            slider: None,
            now: 0.0,
        };
        t.settle_on_the_game_screen();
        t
    }

    fn settle_on_the_game_screen(&mut self) {
        use dereth_ui::framework::mode;
        let host = dereth_client_contract::pregame::PregameView::default();
        self.shell.queue(mode::GAME_PLAY);
        for _ in 0..16 {
            self.now += 1.0;
            self.shell
                .frame(LocalTime(self.now), &host, &mut dereth_ui::NullInputPump);
            if self.shell.flow.current_mode() == Some(mode::GAME_PLAY) {
                self.bind();
                return;
            }
        }
        panic!("the shell never reached the game screen")
    }

    fn bind(&mut self) {
        use dereth_ui_screens::toolbar::splitter::{ENTRY_BOX, SLIDER};
        let root = {
            let screen = self.shell.flow.current_mut().expect("the game screen");
            *screen.roots().first().expect("the game screen has a root")
        };
        let find = |ui: &dereth_ui::UiSystem, id: dereth_ui::ElementId| {
            ui.get_child_recursive(root, id)
                .unwrap_or_else(|| panic!("{id:?} is in the shipped layout"))
        };
        self.field = Some(find(&self.shell.ui, SEL_OBJECT_FIELD));
        self.entry = Some(find(&self.shell.ui, ENTRY_BOX));
        self.slider = Some(find(&self.shell.ui, SLIDER));
    }

    /// A change of screen: the game screen is destroyed and built again from the shipped layout,
    /// and the host-side display survives -- which is why the display's own driver is handed a
    /// serial at all.
    fn rebuild_screen(&mut self) {
        use dereth_ui::framework::mode;
        let host = dereth_client_contract::pregame::PregameView::default();
        self.shell.queue(mode::CHARACTER_MANAGEMENT);
        for _ in 0..16 {
            self.now += 1.0;
            self.shell
                .frame(LocalTime(self.now), &host, &mut dereth_ui::NullInputPump);
            if self.shell.flow.current_mode() == Some(mode::CHARACTER_MANAGEMENT) {
                break;
            }
        }
        self.settle_on_the_game_screen();
        self.serial += 1;
    }

    fn gameplay(&mut self) -> &mut dereth_ui_screens::screens::gameplay::GamePlayScreen {
        let screen = self.shell.flow.current_mut().expect("the game screen");
        let any: &mut dyn std::any::Any = &mut **screen;
        any.downcast_mut::<dereth_ui_screens::screens::gameplay::GamePlayScreen>()
            .expect("the game screen")
    }

    fn drive(&mut self) {
        // The frame seeds the shared stack quantity from the selection before any interface
        // draws, as the client's own frame does.
        self.objects.world.refresh_stack_split();
        let serial = self.serial;
        let screen = self.shell.flow.current_mut().expect("the game screen");
        let any: &mut dyn std::any::Any = &mut **screen;
        let gameplay = any
            .downcast_mut::<dereth_ui_screens::screens::gameplay::GamePlayScreen>()
            .expect("the game screen");
        self.hud
            .drive(&mut self.shell.ui, gameplay, serial, &self.objects);
    }

    fn select(&mut self, id: Option<ObjectId>) {
        self.objects.world.selected = id;
        self.drive();
    }

    /// `(the strip, the entry box, the slider)`, so one reading can name all three.
    fn shown(&self) -> (bool, bool, bool) {
        let v = |h: Option<dereth_ui::ElemHandle>| {
            self.shell
                .ui
                .node(h.expect("the shipped layout was bound"))
                .expect("the element is alive")
                .region
                .flags
                .visible
        };
        (v(self.field), v(self.entry), v(self.slider))
    }

    fn counters(&self) -> (u64, u64) {
        (
            self.hud.stats.toolbar_written,
            self.hud.stats.split_gate_runs,
        )
    }
}

/// A creature with no stack size at all, and a stack of `n`.
fn a_creature_and_a_stack(n: u16) -> (ObjectStream, ObjectId, ObjectId) {
    const CREATURE: ObjectId = ObjectId(0x5000_0101);
    const STACK: ObjectId = ObjectId(0x5000_0102);

    let mut objects = ObjectStream::new();
    let mut creature = dereth_client_model::Weenie::new(CREATURE);
    creature.valid = true;
    creature.pwd.name = "a selection subject".to_owned();
    creature.pwd.obj_type |= dereth_client_runtime::hud::ITEM_TYPE_CREATURE;
    // **The property the subject is chosen for**, not the type: with no stack size at all nothing
    // can re-seed a stale edge for it, which is the only condition under which one remembered
    // selection and two of them can disagree.
    creature.pwd.stack_size = None;
    objects.world.tables.weenies.insert(CREATURE, creature);

    let mut stack = dereth_client_model::Weenie::new(STACK);
    stack.valid = true;
    stack.pwd.name = "a stack of things".to_owned();
    stack.pwd.stack_size = Some(n);
    objects.world.tables.weenies.insert(STACK, stack);

    (objects, CREATURE, STACK)
}

// =============================================================================================
// selection.split-box.is-not-resurrected-by-a-screen-rebuild
// =============================================================================================

/// The split box does not come back beside a creature, and does come back beside a stack.
///
/// **The pair is the test.** A build that hid the box always would pass the first half on its own
/// and would silently undo the stack splitting the second half is about.
pub fn the_split_box_is_not_resurrected_by_a_rebuild() {
    const STACK_SIZE: u16 = 7;

    // A creature, selected, across a change of screen.
    let (objects, creature, _) = a_creature_and_a_stack(STACK_SIZE);
    let mut t = Toolbar::new(objects);
    let carries_no_stack = t
        .objects
        .world
        .weenie(creature)
        .expect("the subject is in the world")
        .pwd
        .stack_size
        .is_none();
    t.select(Some(creature));
    let hidden_for_a_creature = t.shown() == (true, false, false);

    let runs_before = t.hud.stats.split_gate_runs;
    t.rebuild_screen();
    // The freshly built screen brings the strip up and the splitter down before any selection has
    // been read -- which is the toolbar's own start-up state and not the edge under test.
    let fresh_screen_starts_down = t.shown() == (true, false, false);
    t.drive();
    let still_hidden = t.shown() == (true, false, false);
    let the_splitter_half_ran_once = t.hud.stats.split_gate_runs == runs_before + 1;
    let at_the_not_a_stack_value = t.gameplay().splitter
        == dereth_ui_screens::toolbar::splitter::Splitter {
            split_size: 1,
            max_split_size: 1,
        };

    // And a stack, selected, across the same change of screen.
    let (objects, _, stack) = a_creature_and_a_stack(STACK_SIZE);
    let mut t = Toolbar::new(objects);
    t.select(Some(stack));
    let shown_for_a_stack = t.shown() == (true, true, true)
        && t.gameplay().splitter.max_split_size == u32::from(STACK_SIZE);
    t.rebuild_screen();
    // The rebuilt screen's splitter starts at the default: the state is the screen's, and the
    // screen is new.
    let the_state_is_the_screens = t.gameplay().splitter.max_split_size == 1;
    t.drive();
    let shown_again = t.shown() == (true, true, true);
    let reseeded = t.gameplay().splitter.max_split_size == u32::from(STACK_SIZE);

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "selection.split-box.is-not-resurrected-by-a-screen-rebuild",
        move |_| {
            carries_no_stack
                && hidden_for_a_creature
                && fresh_screen_starts_down
                && still_hidden
                && the_splitter_half_ran_once
                && at_the_not_a_stack_value
                && shown_for_a_stack
                && the_state_is_the_screens
                && shown_again
                && reseeded
        },
    );
}

// =============================================================================================
// selection.read-out.follows-one-selection-change-once
// =============================================================================================

/// One change of selection, one run of each half; an idle frame, neither.
pub fn the_selection_read_out_follows_one_change_once() {
    use dereth_client_model::qualities::{StatKey, StatType, StatValue};

    const STACK_SIZE: u16 = 7;

    let (objects, creature, stack) = a_creature_and_a_stack(STACK_SIZE);
    let mut t = Toolbar::new(objects);

    // The first frame with nothing selected: the read-out runs and the splitter does not. That
    // asymmetry is the client's own -- the toolbar's remembered selection and the world's are both
    // nothing, so there is no edge, while the screen has never been asked at all.
    t.drive();
    let first_frame = t.counters() == (1, 0);
    for _ in 0..30 {
        t.drive();
    }
    let idle_frames_move_neither = t.counters() == (1, 0);

    // One change: exactly one of each, and then it settles.
    t.select(Some(creature));
    let one_change_one_edge = t.counters() == (2, 1);
    for _ in 0..30 {
        t.drive();
    }
    let settles_on_a_creature = t.counters() == (2, 1);

    // A second change, to a stack: once each again, and the pair is seeded to the whole stack.
    t.select(Some(stack));
    let second_change =
        t.counters() == (3, 2) && t.gameplay().splitter.max_split_size == u32::from(STACK_SIZE);
    for _ in 0..30 {
        t.drive();
    }
    let settles_on_a_stack = t.counters() == (3, 2);

    // Deselecting is an edge in both halves, and the pair is deliberately left where it was.
    t.select(None);
    let deselect_is_an_edge = t.counters() == (4, 3)
        && t.shown() == (true, false, false)
        && t.gameplay().splitter.max_split_size == u32::from(STACK_SIZE);

    // And the other caller, in the direction nothing had asserted: a selected stack whose size the
    // shard changes re-seeds the splitter without any selection having changed -- and the read-out
    // half does not re-run, because the selection did not.
    let (objects, _, stack) = a_creature_and_a_stack(2);
    let mut t = Toolbar::new(objects);
    t.select(Some(stack));
    let seeded = t.gameplay().splitter.max_split_size == 2;
    for _ in 0..30 {
        t.drive();
    }
    let (wrote, runs) = t.counters();
    // The shard's own producer of that notice, rather than a poke at the field.
    let effect = dereth_client_model::weenie::mirror_stat_update(
        t.objects
            .world
            .weenie_mut(stack)
            .expect("the subject is in the world"),
        StatKey::new(StatType::Int, 12),
        &StatValue::Int(5),
        false,
    );
    let the_stack_really_grew =
        effect.changed && t.objects.world.weenie(stack).expect("row").pwd.stack_size == Some(5);
    t.drive();
    let reseeded_without_a_selection_change = t.counters() == (wrote, runs + 1)
        && t.gameplay().splitter.max_split_size == 5
        && t.shown() == (true, true, true);
    for _ in 0..30 {
        t.drive();
    }
    let and_settles_again = t.counters() == (wrote, runs + 1);

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "selection.read-out.follows-one-selection-change-once",
        move |_| {
            first_frame
                && idle_frames_move_neither
                && one_change_one_edge
                && settles_on_a_creature
                && second_change
                && settles_on_a_stack
                && deselect_is_an_edge
                && seeded
                && the_stack_really_grew
                && reseeded_without_a_selection_change
                && and_settles_again
        },
    );
}

// ===========================================================================================
// The three the shard sends on every login
// ===========================================================================================
//
// Three server-to-client messages, each of which must reach a receiver, every one replayed here
// out of the recording that carries it, framed the way the shard framed it. Two of them belong
// under a panel subject and one under the social one.

/// Every scenario of this section, appended to [`ALL`] above.
// -------------------------------------------------------------------------------------------
// shard-message.pop-up.*
// -------------------------------------------------------------------------------------------

/// The first thing the shard says to a character opens the box it opens in retail, saying what
/// the shard sent, and its one button takes it away again.
pub fn the_shards_first_word_opens_a_box_the_player_can_dismiss() {
    let mut s = shell_support::Station::new();
    // The before-state, so a scenario that passes because the thing was already true cannot
    // exist.
    let nothing_yet = shell_support::popup_root(s.client.view().expect_app()).is_none()
        && s.client
            .view()
            .expect_app()
            .interaction()
            .stats
            .pop_up_strings
            == 0
        && s.client
            .view()
            .expect_app()
            .interaction()
            .stats
            .pop_ups_shown
            == 0;

    let (blob, said) = shell_support::a_recorded_pop_up();
    s.replay(blob);
    s.settle();

    let received = {
        let app = s.client.view().expect_app();
        app.interaction().stats.pop_up_strings == 1
            && app.interaction().stats.pop_up_strings_undecodable == 0
            // A non-zero here is "received and never shown".
            && app.interaction().pop_up_strings_pending() == 0
            && app.interaction().stats.pop_ups_shown == 1
    };
    let root =
        shell_support::popup_root(s.client.view().expect_app()).expect("the box is on screen");
    // What the box says is read off its own glyphs, not off the property that was put in: a
    // receiver that sets a value and never reaches an element fails here. It is compared with
    // what the recorded blob itself decoded to, so no capture text is written down.
    let shown = shell_support::popup_text(s.client.app_mut()) == said;
    let one_button = {
        let app = s.client.view().expect_app();
        shell_support::popups(app)[0].0 == dereth_ui::dialog::base::DialogKind::Message
            // ...and it is on the all-at-once list, not in the one-per-queue map.
            && app.ui().expect("the shell").ui.dialogs.open_on(shell_support::NON_QUEUED).is_none()
    };

    let button = s
        .client
        .view()
        .expect_app()
        .ui()
        .expect("the shell")
        .ui
        .get_child_recursive(root, shell_support::MESSAGE_BUTTON)
        .expect("the shipped box carries its one button");
    s.click(button, 1_000);
    let dismissed = shell_support::popup_root(s.client.view().expect_app()).is_none()
        && s.client.view().expect_app().interaction().stats.pop_ups_dismissed == 1
        // ...and the element is deleted, not merely unbound.
        && s.client.view().expect_app().ui().expect("the shell").ui.node(root).is_none();

    s.client.assert_behaviour(
        "shard-message.pop-up.the-first-thing-the-shard-says-opens-a-box-the-player-can-dismiss",
        move |_| nothing_yet && received && shown && one_button && dismissed,
    );
    s.client.shutdown();
}

/// Three prompts in one burst are three boxes on screen at once, not one with two waiting.
pub fn three_prompts_in_a_burst_are_three_boxes_at_once() {
    let mut s = shell_support::Station::new();
    let mut sent = 0usize;
    for blob in shell_support::three_recorded_pop_ups() {
        s.replay(blob);
        sent += 1;
    }
    s.settle();

    let app = s.client.view().expect_app();
    let arrived = app.interaction().stats.pop_up_strings == sent as u64
        && app.interaction().stats.pop_ups_shown == sent as u64;
    let all = shell_support::popups(app);
    let together = all.len() == sent
        && all.iter().all(|(_, e)| e.is_some())
        && app
            .ui()
            .expect("the shell")
            .ui
            .dialogs
            .waiting_on(shell_support::NON_QUEUED)
            == 0;

    s.client.assert_behaviour(
        "shard-message.pop-up.three-in-one-burst-are-three-boxes-at-once",
        move |_| sent == 3 && arrived && together,
    );
    s.client.shutdown();
}

// -------------------------------------------------------------------------------------------
// shard-message.house-status.*
// -------------------------------------------------------------------------------------------

/// The shard's answer about the player's house redraws the House tab, every time it arrives.
///
/// The text cannot tell the answer from the stand-in the panel draws at login -- for a character
/// with no house it is the same line either way -- so the redraw is what is measured, which is
/// all the panel's own update does.
pub fn the_shards_house_answer_redraws_the_house_tab() {
    let mut s = shell_support::Station::new();

    let before = {
        let app = s.client.view().expect_app();
        assert!(
            app.hud().panels.house.fully_bound(),
            "the House tab is bound off the screen"
        );
        (
            app.hud().stats.house_status_notices,
            app.hud().panels.house.displays,
        )
    };
    let lines_before = s.client.view().expect_app().hud().panels.house.text();
    // One write, from the stand-in the panel draws for itself at login.
    let stand_in = before == (0, 1);

    let (first, second) = shell_support::two_recorded_house_answers();
    s.replay(first);
    s.settle();
    let once = {
        let app = s.client.view().expect_app();
        app.hud().stats.house_status_notices == 1
            && app.hud().panels.house.displays == before.1 + 1
            // The redraw flushes and rewrites; it does not append.
            && app.hud().panels.house.text() == lines_before
            // This is the no-house answer.
            && !app.hud().panels.house.owns_house
    };

    // A second answer is a second redraw: the panel's own update has no "already drawn" guard
    // and the pull that stands in for it must not grow one.
    s.replay(second);
    s.settle();
    let twice = {
        let app = s.client.view().expect_app();
        app.hud().stats.house_status_notices == 2 && app.hud().panels.house.displays == before.1 + 2
    };

    s.client.assert_behaviour(
        "shard-message.house-status.the-shards-answer-redraws-the-house-tab-every-time",
        move |_| stand_in && once && twice,
    );
    s.client.shutdown();
}

// -------------------------------------------------------------------------------------------
// shard-message.fellowship-update-done.*
// -------------------------------------------------------------------------------------------

/// The marker that ends a run of fellowship updates is consumed, and consuming it is the whole
/// of what it does.
///
/// This is deliberately the weakest of the three, because the client it rebuilds does nothing
/// with the message at all. What it asserts is that the message reaches a receiver, plus the two
/// things that must **not** happen: the roster must not move, and no line must appear for the
/// player to read.
pub fn the_fellowship_done_marker_is_consumed_and_changes_nothing() {
    let mut s = shell_support::Station::new();
    let before = {
        let app = s.client.view().expect_app();
        (
            app.hud().stats.fellow_update_done,
            app.hud().stats.fellowship_lines_composed,
            app.hud().stats.fellowship_members,
        )
    };

    let blobs = shell_support::recorded_fellowship_done_markers();
    let n = blobs.len();
    for blob in blobs {
        s.replay(blob);
        s.settle();
    }

    let app = s.client.view().expect_app();
    let consumed = app.hud().stats.fellow_update_done == before.0 + n as u64
        && !dereth_client_runtime::dropped::unreceived(
            dereth_protocol::Opcode::FELLOWSHIP_FELLOW_UPDATE_DONE,
        );
    let changed_nothing = app.hud().stats.fellowship_lines_composed == before.1
        && app.hud().stats.fellowship_members == before.2
        && app.objects().world.fellowship.is_none();

    s.client.assert_behaviour(
        "shard-message.fellowship-update-done.is-consumed-and-changes-nothing-else",
        move |_| n >= 2 && consumed && changed_nothing,
    );
    s.client.shutdown();
}

// -------------------------------------------------------------------------------------------
// shard-message.every-one-of-the-three-reaches-a-receiver
// -------------------------------------------------------------------------------------------

/// All three, through the whole client, against its own ledger of what it threw away.
///
/// The three counters are what makes the verdict a result rather than an absence: the ledger
/// reads the same for a message with a receiver and for one that never arrived, so the arrival
/// of each is asserted first.
pub fn none_of_the_three_reaches_no_receiver() {
    let mut s = shell_support::Station::new();
    dereth_client_runtime::dropped::clear();

    let (pop, _) = shell_support::a_recorded_pop_up();
    s.replay(pop);
    let (house, _) = shell_support::two_recorded_house_answers();
    s.replay(house);
    let markers = shell_support::recorded_fellowship_done_markers();
    let markers_sent = markers.len();
    for blob in markers {
        s.replay(blob);
    }
    s.settle();

    let app = s.client.view().expect_app();
    let arrived = app.interaction().stats.pop_up_strings == 1
        && app.hud().stats.house_status_notices == 1
        && app.hud().stats.fellow_update_done == markers_sent as u64;
    let received = [
        dereth_protocol::Opcode::COMMUNICATION_POP_UP_STRING,
        dereth_protocol::Opcode::HOUSE_HOUSE_STATUS,
        dereth_protocol::Opcode::FELLOWSHIP_FELLOW_UPDATE_DONE,
    ]
    .into_iter()
    .all(|o| !dereth_client_runtime::dropped::unreceived(o));
    let unannounced = dereth_client_runtime::dropped::announcements()
        .iter()
        .all(|l| !l.contains("0x0004") && !l.contains("0x0226") && !l.contains("0x01C9"));

    s.client.assert_behaviour(
        "shard-message.the-three-the-shard-sends-on-every-login-all-reach-a-receiver",
        move |_| arrived && received && unannounced,
    );
    s.client.shutdown();
}

// -------------------------------------------------------------------------------------------
// The fixtures the five above share
// -------------------------------------------------------------------------------------------

/// A whole client with its shell up, and a way of putting a recorded blob into it the way the
/// shard framed it.
///
/// It is a module of its own so that it stays apart from the object-stream fixtures above.
mod shell_support {
    use dereth_client_net::client_session::testing::{Corpus, Direction};
    use dereth_primitives::ObjectId;
    use dereth_protocol::Opcode;
    use dereth_testkit::{ClientSpec, HeadlessClient};
    use dereth_ui::{ElemHandle, ElementId};

    /// The player this bench's own create names.
    const PLAYER: ObjectId = ObjectId(0x5000_0001);
    /// The dialog list the client shows all at once, rather than one at a time.
    pub const NON_QUEUED: u64 = dereth_ui::dialog::factory::NON_QUEUED;
    /// The one button a message box carries.
    pub const MESSAGE_BUTTON: ElementId = dereth_ui::dialog::base::child::MESSAGE_BUTTON;
    /// The queue the client receives all three of these on.
    const UI_QUEUE: u16 = 9;
    /// ...and the one an object's create arrives on.
    const OBJECT_QUEUE: u16 = 10;

    /// Every server-to-client blob of `scenario` whose **game-event** opcode is `want`.
    ///
    /// The outer word of a game event is the wrapper's, and the opcode is the word twelve bytes
    /// in; counting by the outer word reads zero for forty of the forty-five messages this
    /// question was first asked about, which is the shape of an instrument that cannot look
    /// reporting absence.
    fn recorded(scenario: &str, want: Opcode) -> Vec<Vec<u8>> {
        let corpus = Corpus::load(scenario)
            .expect("the corpus parses")
            .unwrap_or_else(|| panic!("{scenario} is not in the corpus"));
        corpus
            .blobs
            .into_iter()
            .filter(|b| b.dir == Direction::ServerToClient)
            .filter(|b| {
                b.opcode == 0xF7B0
                    && b.payload.len() >= 16
                    && u32::from_le_bytes(b.payload[12..16].try_into().expect("four bytes"))
                        == want.0
            })
            .map(|b| b.payload)
            .collect()
    }

    /// The recordings searched for these messages, in the corpus's own order.
    fn sessions() -> &'static [&'static str] {
        dereth_client_net::client_session::testing::session_names()
    }

    /// One recorded prompt, and what its body decodes to -- so a scenario can compare what the
    /// box says with what the shard sent without any capture text being written down here.
    pub fn a_recorded_pop_up() -> (Vec<u8>, String) {
        for s in sessions() {
            if let Some(blob) = recorded(s, Opcode::COMMUNICATION_POP_UP_STRING).pop() {
                let said = decode_pop_up(&blob);
                return (blob, said);
            }
        }
        panic!("no recording carries a prompt from the shard")
    }

    /// Three of them, from three different recordings.
    pub fn three_recorded_pop_ups() -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        for s in sessions() {
            if let Some(blob) = recorded(s, Opcode::COMMUNICATION_POP_UP_STRING).pop() {
                out.push(blob);
                if out.len() == 3 {
                    return out;
                }
            }
        }
        panic!("fewer than three recordings carry a prompt from the shard")
    }

    fn decode_pop_up(blob: &[u8]) -> String {
        let mut r = dereth_protocol::archive::Reader::new(&blob[16..]);
        let m =
            <dereth_protocol::comms::CommunicationPopUpString as dereth_protocol::Message>::read(
                &mut r,
            )
            .expect("the recorded body decodes");
        assert!(!m.message.is_empty(), "the shard's prompt is not empty");
        m.message
    }

    /// Two recorded answers about the player's house, from two recordings.
    pub fn two_recorded_house_answers() -> (Vec<u8>, Vec<u8>) {
        let mut out = Vec::new();
        for s in sessions() {
            if let Some(blob) = recorded(s, Opcode::HOUSE_HOUSE_STATUS).pop() {
                // The whole payload is the wrapper and one word -- the word every receiver in
                // retail discards.
                assert_eq!(blob.len(), 20, "a house answer is a wrapper and one word");
                out.push(blob);
                if out.len() == 2 {
                    return (out.remove(0), out.remove(0));
                }
            }
        }
        panic!("fewer than two recordings carry the shard's answer about a house")
    }

    /// Every recorded marker that ends a run of fellowship updates.
    pub fn recorded_fellowship_done_markers() -> Vec<Vec<u8>> {
        for s in sessions() {
            let blobs = recorded(s, Opcode::FELLOWSHIP_FELLOW_UPDATE_DONE);
            if blobs.len() >= 2 {
                for b in &blobs {
                    // The wrapper alone, with no body at all.
                    assert_eq!(b.len(), 16, "the marker carries nothing");
                }
                return blobs;
            }
        }
        panic!("no recording carries two of the fellowship markers")
    }

    /// The shard, as far as the client can tell: a connection it will accept fragments on.
    /// It is the harness's `dereth_testkit::replay::Peer`.
    use dereth_testkit::Peer;

    /// One client with its shell up and a peer feeding it recorded bytes.
    pub struct Station {
        pub client: HeadlessClient,
        peer: Peer,
        pump: dereth_desktop::pump::Pump,
    }

    impl Station {
        pub fn new() -> Self {
            dereth_client_runtime::dropped::clear();
            let mut client = HeadlessClient::new(ClientSpec::gameplay_in_world(4));

            let peer = Peer::attach(&mut client, PLAYER);
            let mut s = Self {
                client,
                peer,
                pump: dereth_desktop::pump::Pump::new(),
            };
            s.pump.state.is_ready = true;
            s.pump.state.is_active_app = true;

            // A body for the player, so the client has someone for the shard to be talking to.
            let mut p = dereth_protocol::objects::ObjectCreatePayload {
                id: PLAYER,
                ..Default::default()
            };
            p.physicsdesc.bitfield |= dereth_protocol::types::physicsdesc::flags::SETUP;
            p.physicsdesc.setup_id = Some(0x0200_0001);
            p.physicsdesc.timestamps.instance = 1;
            let blob = dereth_protocol::write_blob(&dereth_protocol::objects::ItemCreateObject(p))
                .expect("the create encodes");
            s.send(OBJECT_QUEUE, blob);
            s.client.tick(1);
            s.client.world_mut().player = Some(PLAYER);
            s.client
                .world_mut()
                .weenie_mut(PLAYER)
                .expect("the player was created")
                .pwd
                .name = "Larktest".to_owned();
            s.settle();
            s
        }

        /// Let the boundary deliver, the box get made, and its element get built.
        pub fn settle(&mut self) {
            self.client.tick(6);
        }

        /// Replay one recorded game event, re-addressed to this session.
        ///
        /// The opcode and the whole body are the recording's own bytes, padding included. The
        /// wrapper in front of them is bookkeeping and **must** be rewritten: it names the
        /// recorded character rather than this one, and its stamp is the one that session had
        /// reached -- replaying that verbatim into a session whose counter starts at zero stalls
        /// every blob behind a stamp that never arrives, which is how a replay can
        /// pass while delivering nothing at all.
        pub fn replay(&mut self, mut blob: Vec<u8>) {
            assert!(blob.len() >= 16, "a game event is at least a wrapper");
            assert_eq!(
                u32::from_le_bytes(blob[0..4].try_into().expect("four bytes")),
                0xF7B0,
                "this helper re-stamps ordered game events only"
            );
            // The two assertions above are this helper's own; the re-addressing and the
            // re-stamping are the harness's. The queue is pinned here rather than passed,
            // because the harness sends an ordered blob on its own -- so this says the two
            // agree instead of assuming it.
            assert_eq!(
                UI_QUEUE,
                dereth_testkit::replay::ORDERED_QUEUE,
                "one ordered queue"
            );
            self.peer.replay_blob(&mut self.client, blob);
        }

        /// Feed one blob as the shard framed it: one fragment on `queue`.
        fn send(&mut self, queue: u16, bytes: Vec<u8>) {
            self.peer.send(&mut self.client, queue, bytes);
        }

        /// Click a control, through the client's own pump and input manager.
        pub fn click(&mut self, h: ElemHandle, time: u32) {
            use dereth_input::win32::Win32Message;
            let at = {
                let app = self.client.view().expect_app();
                let ui = &app.ui().expect("the shell").ui;
                let mut parent = Some(h);
                while let Some(p) = parent {
                    assert!(
                        ui.node(p).expect("a node").region.flags.visible,
                        "every ancestor of the control is visible"
                    );
                    parent = ui.parent(p);
                }
                let r = ui.screen_clip_box(h);
                assert!(r.is_valid(), "the control has a real visible box");
                assert!(
                    ui.hit_test_screen((r.x0 + r.x1) / 2, (r.y0 + r.y1) / 2)
                        .is_some_and(|hit| hit == h || ui.is_ancestor_of(h, hit)),
                    "the control is the thing under the cursor"
                );
                ((r.x0 + r.x1) / 2, (r.y0 + r.y1) / 2)
            };
            let send = |s: &mut Self, msg: Win32Message| {
                s.pump.dispatch(msg);
                s.client
                    .app_mut()
                    .input_manager_mut()
                    .expect("an input manager")
                    .on_message(msg);
            };
            let msg = self
                .pump
                .mouse_move_message(f64::from(at.0), f64::from(at.1), time);
            send(self, msg);
            for (down, t) in [(true, time + 1), (false, time + 2)] {
                let msg = self
                    .pump
                    .mouse_button_message(winit::event::MouseButton::Left, down, t)
                    .expect("a button message");
                send(self, msg);
            }
            self.settle();
        }
    }

    /// Every box on the all-at-once list, as its kind and its element.
    pub fn popups(
        app: &dereth_client::app::App,
    ) -> Vec<(dereth_ui::dialog::base::DialogKind, Option<ElemHandle>)> {
        app.ui()
            .expect("the shell")
            .ui
            .dialogs
            .non_queued()
            .iter()
            .map(|i| (i.kind, i.element))
            .collect()
    }

    /// The one open box's root element.
    pub fn popup_root(app: &dereth_client::app::App) -> Option<ElemHandle> {
        let all = popups(app);
        assert!(
            all.len() <= 1,
            "these scenarios raise at most one at a time"
        );
        all.first().and_then(|(_, e)| *e)
    }

    /// What the box actually says, off its own glyphs.
    pub fn popup_text(app: &mut dereth_client::app::App) -> String {
        let root = popup_root(app).expect("an open box");
        let ui = &mut app.ui_mut().expect("the shell").ui;
        let h = ui
            .get_child_recursive(root, dereth_ui::dialog::base::child::TEXT)
            .expect("the shipped box carries a text child");
        ui.text_element_mut(h)
            .expect("a text element")
            .glyphs
            .inq_text(false)
    }
}

// ===========================================================================================
// What a selection asks the shard, and what the answer fills in
// ===========================================================================================
//
// The subject is the toolbar's read-out over the shipped gameplay screen, driven against the
// world a recording built, so the scenarios open the real screen rather than a whole client.

/// Selecting a creature asks the shard about its health, once.
pub fn selecting_a_creature_asks_about_its_health_once() {
    use dereth_client_model::Request;

    let mut b = hud_support::Bench::new();
    // The calibration, and it is a fraction rather than a yes: both answers are present in the
    // recording, so neither leg of the question below is untested. Counted rather than pinned.
    let (creatures, attackable) = b.creature_census();
    println!(
        "selection queries: {attackable} of {creatures} creatures in the busiest recording are \
         ones the client would attack"
    );
    let both_present = attackable > 0 && attackable < creatures;

    let creature = b.an_attackable_creature();

    // Nothing selected: the client's own edge test is false, so the arm is not reached.
    let idle = b.frame().is_empty();
    let _ = b.sent();
    let nothing_yet = b.hud.stats.selection_health_queries == 0;

    let unowned = b.select(Some(creature));
    let owned = unowned.is_empty();
    let asked = b.hud.stats.selection_health_queries == 1
        && b.hud.stats.selection_mana_queries == 0
        && b.hud.stats.selection_queries_unanswerable == 0;

    let sent = b.sent();
    let queries: Vec<dereth_primitives::ObjectId> = sent
        .iter()
        .filter_map(|r| match r {
            Request::QueryHealth(q) => Some(q.target),
            _ => None,
        })
        .collect();
    // One message, not two: the clear the edge would send is guarded on the meter being up, and
    // the shipped layout brings both meters up hidden.
    let one = queries == vec![creature] && b.hud.stats.selection_query_clears == 0;

    // And it settles: thirty idle frames ask nothing more.
    let mut quiet = true;
    for _ in 0..30 {
        quiet &= b.frame().is_empty();
    }
    let settled = quiet && b.hud.stats.selection_health_queries == 1 && b.sent().is_empty();

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "selection.query.selecting-a-creature-asks-the-shard-about-its-health-once",
        move |_| both_present && idle && nothing_yet && owned && asked && one && settled,
    );
}

/// Selecting a pile of things asks the shard nothing at all.
///
/// The negative half, without which a build that asks unconditionally satisfies the claim above.
pub fn selecting_a_stack_asks_nothing() {
    let mut b = hud_support::Bench::new();
    let (stack, size) = b.the_largest_stack();
    let really_a_stack = size >= 2;

    let idle = b.frame().is_empty();
    let _ = b.sent();
    let owned = b.select(Some(stack)).is_empty();

    let asked_nothing =
        b.hud.stats.selection_health_queries == 0 && b.hud.stats.selection_mana_queries == 0;
    let sent = b.sent();
    let silent = hud_support::per_selection(&sent).is_empty();
    // ...and the half that splits a stack did run, so the arm really was reached.
    let reached = b.hud.stats.split_gate_runs == 1;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "selection.query.selecting-a-pile-of-things-asks-the-shard-nothing",
        move |_| really_a_stack && idle && owned && asked_nothing && silent && reached,
    );
}

/// What the client asks about a selected thing has four outcomes and not two.
///
/// A recording cannot supply a pet or a hostile player, so those objects are built here; the
/// branch that sorts them is the client's own, reached through the shipped screen.
pub fn what_is_asked_has_four_outcomes_and_not_two() {
    use dereth_primitives::ObjectId;
    use dereth_ui_screens::screens::gameplay::SelectionQuery;
    use {dereth_rules::weenie::bitfield, dereth_rules::weenie::item_type};

    let mut b = hud_support::Bench::new();
    let me = b
        .objects
        .world
        .player
        .expect("the recording named a player");
    let w = &mut b.objects.world;
    // A monster the player may attack.
    let monster = hud_support::put(
        w,
        0x7000_0001,
        item_type::CREATURE,
        bitfield::ATTACKABLE,
        None,
        None,
    );
    // Another player nobody may attack -- and he is asked about anyway, because he is a player.
    let other = hud_support::put(
        w,
        0x7000_0002,
        item_type::CREATURE,
        bitfield::PLAYER,
        None,
        None,
    );
    // Somebody's pet, likewise.
    let pet = hud_support::put(
        w,
        0x7000_0003,
        item_type::CREATURE,
        0,
        Some(ObjectId(0x7000_0009)),
        None,
    );
    // Something in the player's own pack: asked about, but about its magic rather than its life.
    let mine = hud_support::put(w, 0x7000_0004, item_type::CASTER, 0, None, Some(me));
    // Something lying on the ground that is nobody's: asked about at all.
    let ground = hud_support::put(w, 0x7000_0005, item_type::MISC, 0, None, None);

    let cases: &[(ObjectId, SelectionQuery)] = &[
        (monster, SelectionQuery::Health),
        (other, SelectionQuery::Health),
        (pet, SelectionQuery::Health),
        (mine, SelectionQuery::ItemMana),
        (ground, SelectionQuery::Neither),
    ];
    let idle = b.frame().is_empty();
    let _ = b.sent();
    let mut ok = true;
    for (id, want) in cases {
        let before = b.query_counts();
        ok &= b.select(Some(*id)).is_empty();
        let after = b.query_counts();
        let moved = match (after.0 - before.0, after.1 - before.1, after.2 - before.2) {
            (1, 0, 0) => SelectionQuery::Health,
            (0, 1, 0) => SelectionQuery::ItemMana,
            (0, 0, 1) => SelectionQuery::Neither,
            _ => SelectionQuery::NotAsked,
        };
        ok &= moved == *want;
        let sent = b.sent();
        let asked = hud_support::per_selection(&sent);
        ok &= match want {
            SelectionQuery::Health => matches!(
                asked.as_slice(),
                [dereth_client_model::Request::QueryHealth(q)] if q.target == *id
            ),
            SelectionQuery::ItemMana => matches!(
                asked.as_slice(),
                [dereth_client_model::Request::QueryItemMana(q)] if q.object == *id
            ),
            SelectionQuery::Neither => asked.is_empty(),
            SelectionQuery::NotAsked => false,
        };
    }
    let all_answerable = b.hud.stats.selection_queries_unanswerable == 0;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "selection.query.what-the-client-asks-about-a-selected-thing-has-four-outcomes",
        move |_| idle && ok && all_answerable,
    );
}

/// A pile that shrinks to one asks again, with no selection having changed.
pub fn a_pile_that_shrinks_to_one_asks_again() {
    use dereth_client_model::qualities::{StatKey, StatType, StatValue};

    let mut b = hud_support::Bench::new();
    let (stack, size) = b.the_largest_stack();
    let really_a_stack = size >= 2;
    let idle = b.frame().is_empty();
    let _ = b.sent();
    let selected = b.select(Some(stack)).is_empty() && b.hud.stats.selection_health_queries == 0;
    let wrote = b.hud.stats.toolbar_written;
    let _ = b.sent();

    // The shard shrank it to one, through the client's own producer.
    let effect = dereth_client_model::weenie::mirror_stat_update(
        b.objects
            .world
            .weenie_mut(stack)
            .expect("the pile is there"),
        StatKey::new(StatType::Int, 12),
        &StatValue::Int(1),
        false,
    );
    let shrank = effect.changed;

    let quiet = b.frame().is_empty();
    // The read-out half did **not** re-run: the selection did not change.
    let read_out_unchanged = b.hud.stats.toolbar_written == wrote;
    // ...but the asking half did, exactly once. Which leg it took depends on the thing, and
    // this one is not the player's.
    let counts = b.query_counts();
    let asked_once =
        counts.0 + counts.1 + counts.2 == 1 && b.hud.stats.selection_queries_unanswerable == 0;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "selection.query.a-pile-that-shrinks-to-one-asks-again-with-no-selection-change",
        move |_| {
            really_a_stack
                && idle
                && selected
                && shrank
                && quiet
                && read_out_unchanged
                && asked_once
        },
    );
}

/// The shard's answer fills the selected thing's health bar, and only its own.
pub fn the_shards_answer_fills_the_selected_things_health_bar() {
    use dereth_primitives::ObjectId;

    let mut b = hud_support::Bench::new();
    let creature = b.an_attackable_creature();

    let idle = b.frame().is_empty();
    let _ = b.sent();
    let selected = b.select(Some(creature)).is_empty();
    // The edge took the bars down and the question went out; nothing has answered yet.
    let down = !b.meter_visible(b.health) && b.hud.stats.selection_meters_written == 0;

    let answered = b.objects.world.update_object_health(creature, 0.375);
    let quiet = b.frame().is_empty();
    let filled = b.hud.stats.selection_meters_written == 1
        && b.meter_visible(b.health)
        && b.meter(b.health) == Some(0.375)
        // Nothing answered about magic, so that bar is still down.
        && !b.meter_visible(b.mana);

    // An answer about something else is dropped.
    let elsewhere = !b
        .objects
        .world
        .update_object_health(ObjectId(0x7EEE_EEEE), 0.9);
    let quiet2 = b.frame().is_empty();
    let unchanged = b.meter(b.health) == Some(0.375);

    // A new selection clears the pair, so the last thing's health cannot be drawn on the new one
    // for a round trip.
    let other = b.another_attackable_creature(creature);
    let reselected = b.select(Some(other)).is_empty();
    let cleared = !b.meter_visible(b.health) && b.objects.world.selected_meters.health.is_none();

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "selection.meters.the-shards-answer-fills-the-selected-things-health-bar",
        move |_| {
            idle && selected
                && down
                && answered
                && quiet
                && filled
                && elsewhere
                && quiet2
                && unchanged
                && reselected
                && cleared
        },
    );
}

/// An answer about a thing's magic fills its bar only when the shard says it succeeded.
pub fn a_magic_answer_fills_the_bar_only_when_it_succeeded() {
    use dereth_primitives::ObjectId;
    use dereth_rules::weenie::item_type;

    const WAND: ObjectId = ObjectId(0x7100_0001);

    let mut b = hud_support::Bench::new();
    let me = b
        .objects
        .world
        .player
        .expect("the recording named a player");
    hud_support::put(
        &mut b.objects.world,
        WAND.0,
        item_type::CASTER,
        0,
        None,
        Some(me),
    );

    let idle = b.frame().is_empty();
    let _ = b.sent();
    let selected = b.select(Some(WAND)).is_empty() && b.hud.stats.selection_mana_queries == 1;
    let sent = b.sent();
    let asked = sent
        .iter()
        .any(|r| matches!(r, dereth_client_model::Request::QueryItemMana(q) if q.object == WAND));

    // A failed answer writes nothing.
    let failed = !b.objects.world.update_item_mana(WAND, 0.5, false);
    let quiet = b.frame().is_empty();
    let still_down = !b.meter_visible(b.mana) && b.hud.stats.selection_meters_written == 0;

    let succeeded = b.objects.world.update_item_mana(WAND, 0.5, true);
    let quiet2 = b.frame().is_empty();
    let filled = b.meter_visible(b.mana)
        && b.meter(b.mana) == Some(0.5)
        && b.hud.stats.selection_meters_written == 1;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "selection.meters.an-answer-about-a-things-magic-fills-its-bar-only-when-it-succeeded",
        move |_| {
            idle && selected
                && asked
                && failed
                && quiet
                && still_down
                && succeeded
                && quiet2
                && filled
        },
    );
}

/// A new selection tells the shard to stop reporting about the old one -- for each bar that was
/// actually showing, and for no other.
///
/// The shipped layout brings both bars up hidden, so a first selection clears nothing; an answer
/// brings one up; and the next selection clears exactly that one.
pub fn a_new_selection_clears_only_the_bar_that_was_showing() {
    let mut b = hud_support::Bench::new();
    let creature = b.an_attackable_creature();
    let start_hidden = !b.meter_visible(b.health) && !b.meter_visible(b.mana);
    let idle = b.frame().is_empty();
    let _ = b.sent();

    let selected = b.select(Some(creature)).is_empty();
    let sent = b.sent();
    let nothing_to_clear =
        hud_support::clears(&sent) == 0 && b.hud.stats.selection_query_clears == 0;

    // The shard answers, which is the only thing that brings a bar up.
    let answered = b.objects.world.update_object_health(creature, 0.5);
    let quiet = b.frame().is_empty();
    let up = b.meter_visible(b.health) && !b.meter_visible(b.mana);
    let _ = b.sent();

    // Now the next selection clears exactly the one that is up.
    let other = b.another_attackable_creature(creature);
    let reselected = b.select(Some(other)).is_empty();
    let sent = b.sent();
    let one_clear = hud_support::clears(&sent) == 1
        && sent.iter().any(|r| {
            matches!(r, dereth_client_model::Request::QueryHealth(q) if q.target == dereth_primitives::ObjectId(0))
        })
        && b.hud.stats.selection_query_clears == 1
        && !b.meter_visible(b.health);

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "selection.query.a-new-selection-clears-only-the-bar-that-was-showing",
        move |_| {
            start_hidden
                && idle
                && selected
                && nothing_to_clear
                && answered
                && quiet
                && up
                && reselected
                && one_clear
        },
    );
}

// -------------------------------------------------------------------------------------------
// The fixtures the six above share
// -------------------------------------------------------------------------------------------

/// The shipped gameplay screen, the HUD that drives it, and the world a recording built.
mod hud_support {
    use dereth_client_model::Request;
    use dereth_primitives::{ObjectId, ServerTime};
    use dereth_ui::{ElemHandle, ElementId, UiSystem};
    use dereth_ui_screens::view::UiRequest;

    /// The two bars the toolbar's read-out fills, by the ids the shipped tree gives them.
    const HEALTH_METER: ElementId = ElementId(0x1000_01A1);
    const MANA_METER: ElementId = ElementId(0x1000_01A2);

    /// One gameplay screen, the HUD that drives it, and something to take what it raises.
    pub struct Bench {
        ui: UiSystem,
        screen: Box<dyn dereth_ui::framework::Screen>,
        pub hud: dereth_client_shell::hud::Hud,
        pub objects: dereth_client_runtime::objects::ObjectStream,
        inter: dereth_client_runtime::interaction::Interaction,
        pub health: ElemHandle,
        pub mana: ElemHandle,
        now: f64,
    }

    impl Bench {
        pub fn new() -> Self {
            let (ui, screen) = crate::world::world_support::shipped_gameplay();
            let objects = crate::world::world_support::replay_objects_at_peak(
                crate::world::world_support::the_busiest_recording(),
            );
            let player = objects.world.player.expect("the recording named a player");
            let pos = objects
                .presence(player)
                .and_then(|p| p.position)
                .expect("the player has a place");
            let mut hud = dereth_client_shell::hud::Hud::new();
            hud.sync(
                &objects,
                Some(dereth_client_runtime::hud::ViewerFrame {
                    position: pos,
                    heading_degrees: 0.0,
                }),
            );
            let root = *screen.roots().first().expect("the screen has a root");
            let find = |id: ElementId| {
                ui.get_child_recursive(root, id)
                    .unwrap_or_else(|| panic!("{id:?} is in the shipped layout"))
            };
            let (health, mana) = (find(HEALTH_METER), find(MANA_METER));
            Self {
                ui,
                screen,
                hud,
                objects,
                inter: dereth_client_runtime::interaction::Interaction::default(),
                health,
                mana,
                now: 0.0,
            }
        }

        /// One frame of the HUD's own driver, then what it raised put through the production
        /// dispatch. Answers anything the dispatch did not own, which must always be empty.
        pub fn frame(&mut self) -> Vec<UiRequest> {
            let g = crate::world::world_support::as_gameplay(&mut self.screen);
            self.hud.drive(&mut self.ui, g, 1, &self.objects);
            let raised = self.ui.requests.take();
            self.inter.queue(Vec::new(), raised);
            self.now += 1.0;
            self.inter
                .run_ui_requests(&mut self.objects.world, false, ServerTime(self.now))
        }

        pub fn select(&mut self, iid: Option<ObjectId>) -> Vec<UiRequest> {
            let mut out = dereth_client_model::RecordingSink::default();
            self.objects.world.set_selected_object(iid, false, &mut out);
            self.frame()
        }

        /// Everything the frame put in the outbox since the last look.
        pub fn sent(&mut self) -> Vec<Request> {
            self.inter.take_pending_requests()
        }

        pub fn query_counts(&self) -> (u64, u64, u64) {
            (
                self.hud.stats.selection_health_queries,
                self.hud.stats.selection_mana_queries,
                self.hud.stats.selection_queries_declined,
            )
        }

        /// The level in a bar, read through the attribute number the tree stores it under rather
        /// than through the same symbol the write used.
        pub fn meter(&self, h: ElemHandle) -> Option<f32> {
            const METER_LEVEL: u32 = 0x69;
            assert_eq!(METER_LEVEL, dereth_ui_screens::bind::attr::METER_LEVEL);
            let n = self.ui.node(h)?;
            match n.instance_properties.get(METER_LEVEL) {
                Some(dereth_assets::ui::PropertyValue::Float(v)) => Some(*v),
                _ => None,
            }
        }

        pub fn meter_visible(&self, h: ElemHandle) -> bool {
            self.ui
                .node(h)
                .expect("the bar is alive")
                .region
                .flags
                .visible
        }

        /// How many creatures the recording's world holds, and how many of them the client would
        /// let the player attack.
        pub fn creature_census(&self) -> (usize, usize) {
            let all = creatures(&self.objects);
            let attackable = all
                .iter()
                .filter(|id| self.objects.world.object_is_attackable(**id))
                .count();
            (all.len(), attackable)
        }

        pub fn an_attackable_creature(&self) -> ObjectId {
            *attackable_creatures(&self.objects)
                .first()
                .expect("the recording holds a creature the player could attack")
        }

        pub fn another_attackable_creature(&self, not: ObjectId) -> ObjectId {
            *attackable_creatures(&self.objects)
                .iter()
                .find(|id| **id != not)
                .expect("the recording holds a second one")
        }

        /// The biggest pile of things in the recording's world.
        pub fn the_largest_stack(&self) -> (ObjectId, u32) {
            let mut v: Vec<(ObjectId, u32)> = self
                .objects
                .world
                .tables
                .weenies
                .iter()
                .filter(|(_, w)| w.pwd.stack_size.unwrap_or(0) >= 2)
                .map(|(id, w)| (id, u32::from(w.pwd.stack_size.unwrap_or(0))))
                .collect();
            v.sort_by_key(|e| e.0 .0);
            v.into_iter()
                .max_by_key(|s| s.1)
                .expect("the recording holds a pile of things")
        }
    }

    /// The recording's creatures, by kind. Used only to choose a subject.
    fn creatures(objects: &dereth_client_runtime::objects::ObjectStream) -> Vec<ObjectId> {
        let mut v: Vec<_> = objects
            .world
            .tables
            .weenies
            .iter()
            .filter(|(_, w)| w.pwd.obj_type & dereth_rules::weenie::item_type::CREATURE != 0)
            .map(|(id, _)| id)
            .collect();
        v.sort_by_key(|id| id.0);
        v
    }

    fn attackable_creatures(
        objects: &dereth_client_runtime::objects::ObjectStream,
    ) -> Vec<ObjectId> {
        let me = objects.world.player;
        creatures(objects)
            .into_iter()
            .filter(|id| Some(*id) != me && objects.world.object_is_attackable(*id))
            .collect()
    }

    /// Put one made-up object in the world. A recording cannot supply a pet or a hostile player,
    /// so the *objects* are built and the **branch** that sorts them is the client's own.
    pub fn put(
        w: &mut dereth_client_model::World,
        id: u32,
        obj_type: u32,
        bits: u32,
        pet: Option<ObjectId>,
        container: Option<ObjectId>,
    ) -> ObjectId {
        let mut o = dereth_client_model::weenie::Weenie::new(ObjectId(id));
        o.pwd = dereth_protocol::types::PublicWeenieDesc {
            name: format!("subject {id}"),
            obj_type,
            bitfield: bits,
            pet_owner: pet,
            container_id: container,
            ..dereth_protocol::types::PublicWeenieDesc::default()
        };
        w.tables.weenies.insert(ObjectId(id), o);
        ObjectId(id)
    }

    /// The requests a selection raised, with the "stop reporting" ones taken out.
    pub fn per_selection(sent: &[Request]) -> Vec<&Request> {
        sent.iter()
            .filter(|r| {
                !matches!(r, Request::QueryHealth(q) if q.target == ObjectId(0))
                    && !matches!(r, Request::QueryItemMana(q) if q.object == ObjectId(0))
            })
            .collect()
    }

    /// ...and how many of the "stop reporting" ones there were.
    pub fn clears(sent: &[Request]) -> usize {
        sent.iter()
            .filter(|r| {
                matches!(r, Request::QueryHealth(q) if q.target == ObjectId(0))
                    || matches!(r, Request::QueryItemMana(q) if q.object == ObjectId(0))
            })
            .count()
    }
}

// -------------------------------------------------------------------------------------------
// Held objects: the holding frame, parent events, placement and picking.
// -------------------------------------------------------------------------------------------

mod held {
    use std::collections::BTreeMap;

    use dereth_animation::data::LocationEntry;
    use dereth_assets::Decode;
    use dereth_client_net::client_session::SessionEvent;
    use dereth_client_runtime::objects::ObjectStream;
    use dereth_primitives::{CellId, DataId, Frame, LocalTime, ObjectId, Quat, Vec3, Viewport};
    use dereth_protocol::objects::{ItemCreateObject, ItemParentEvent, ObjectCreatePayload};
    use dereth_protocol::types::physicsdesc::{flags, PhysicsDesc};
    use dereth_protocol::types::{
        ObjDesc, PhysicsEventStamp, PhysicsTimestamps, PositionWire, PublicWeenieDesc,
    };
    use dereth_protocol::{write_body, Opcode};
    use {
        dereth_client_runtime::models::resolve_parts,
        dereth_client_runtime::models::resolve_parts_at,
        dereth_client_runtime::models::PLACEMENT_RESTING,
    };
    use {dereth_client_runtime::pick::PickScene, dereth_client_runtime::pick::WorldPicker};

    use super::{store, PERSON_SETUP, RIGHT_HAND};

    const CELL: u32 = 0x8602_01AD;
    const HOLDER: u32 = 0x5000_0001;
    const ITEM: u32 = 0x5000_0002;
    const ITEM_SETUP: u32 = 0x0200_0ED7;
    const PICK_SETUP: u32 = 0x0200_0181;
    const MISSILE_SETUP: u32 = 0x0200_0124;
    const BOOKCASE_SETUP: u32 = 0x0200_0183;

    fn quat_about_z(deg: f64) -> Quat {
        let h = deg.to_radians() / 2.0;
        #[allow(clippy::cast_possible_truncation)]
        Quat::new(
            dereth_primitives::num::math::cos(h) as f32,
            0.0,
            0.0,
            dereth_primitives::num::math::sin(h) as f32,
        )
    }

    fn rotate(q: Quat, v: Vec3) -> (f64, f64, f64) {
        let (w, x, y, z) = (
            f64::from(q.w),
            f64::from(q.x),
            f64::from(q.y),
            f64::from(q.z),
        );
        let (vx, vy, vz) = (f64::from(v.x), f64::from(v.y), f64::from(v.z));
        let (tx, ty, tz) = (
            2.0 * (y * vz - z * vy),
            2.0 * (z * vx - x * vz),
            2.0 * (x * vy - y * vx),
        );
        (
            vx + w * tx + (y * tz - z * ty),
            vy + w * ty + (z * tx - x * tz),
            vz + w * tz + (x * ty - y * tx),
        )
    }

    fn create_event(
        id: u32,
        setup: u32,
        parent: Option<(u32, u32)>,
        placement: Option<u32>,
    ) -> SessionEvent {
        let mut physicsdesc = PhysicsDesc {
            bitfield: flags::SETUP,
            setup_id: Some(setup),
            timestamps: PhysicsTimestamps::default(),
            ..PhysicsDesc::default()
        };
        if let Some((holder, location)) = parent {
            physicsdesc.bitfield |= flags::PARENT;
            physicsdesc.parent = Some((ObjectId(holder), location));
        } else {
            physicsdesc.bitfield |= flags::POSITION;
            physicsdesc.position = Some(PositionWire {
                objcell_id: CELL,
                ..PositionWire::default()
            });
        }
        if let Some(placement) = placement {
            physicsdesc.bitfield |= flags::ANIMFRAME;
            physicsdesc.animframe_id = Some(placement);
        }
        let body = write_body(&ItemCreateObject(ObjectCreatePayload {
            id: ObjectId(id),
            objdesc: ObjDesc::default(),
            physicsdesc,
            wdesc: PublicWeenieDesc::default(),
        }))
        .expect("the create encodes");
        SessionEvent::WorldObject {
            opcode: Opcode::ITEM_CREATE_OBJECT,
            body,
        }
    }

    fn contained_create(id: u32, setup: u32) -> SessionEvent {
        let physicsdesc = PhysicsDesc {
            bitfield: flags::SETUP,
            setup_id: Some(setup),
            ..PhysicsDesc::default()
        };
        let body = write_body(&ItemCreateObject(ObjectCreatePayload {
            id: ObjectId(id),
            objdesc: ObjDesc::default(),
            physicsdesc,
            wdesc: PublicWeenieDesc::default(),
        }))
        .expect("the contained create encodes");
        SessionEvent::WorldObject {
            opcode: Opcode::ITEM_CREATE_OBJECT,
            body,
        }
    }

    fn parent_event(
        holder: u32,
        item: u32,
        location: u32,
        placement: u32,
        stamp: u16,
    ) -> SessionEvent {
        let body = write_body(&ItemParentEvent {
            creature: ObjectId(holder),
            item: ObjectId(item),
            location,
            placement_frame: placement,
            timestamps: PhysicsEventStamp {
                instance: 0,
                event: stamp,
            },
        })
        .expect("the parent event encodes");
        SessionEvent::WorldObject {
            opcode: Opcode::ITEM_PARENT_EVENT,
            body,
        }
    }

    fn objects() -> ObjectStream {
        ObjectStream::with_store(store())
    }

    pub(super) fn an_out_of_range_holding_part_uses_the_holders_frame() -> bool {
        let root = Frame::new(Vec3::new(1.0, 2.0, 3.0), quat_about_z(90.0));
        let parts = vec![Frame::new(Vec3::new(9.0, 9.0, 9.0), Quat::IDENTITY); 2];
        let holding = LocationEntry {
            part_id: 2,
            frame: Frame::new(Vec3::new(0.5, 0.0, 0.0), Quat::IDENTITY),
        };
        let want = dereth_animation::frame::combine(&root, &holding.frame);
        let fallback = dereth_client_runtime::models::child_frame(&root, &parts, &holding) == want;
        assert_eq!(
            dereth_client_runtime::models::child_frame(&root, &parts, &holding),
            want,
            "an index equal to the part count is already out of range"
        );
        let inside = LocationEntry {
            part_id: 1,
            ..holding
        };
        let present_part_wins =
            dereth_client_runtime::models::child_frame(&root, &parts, &inside) != want;
        assert!(present_part_wins);
        fallback && present_part_wins
    }

    pub(super) fn a_new_parent_event_attaches_the_child_in_wire_order() -> bool {
        let mut s = objects();
        s.apply_event(
            &create_event(HOLDER, PERSON_SETUP, None, None),
            LocalTime(0.0),
        );
        s.apply_event(&create_event(ITEM, ITEM_SETUP, None, None), LocalTime(0.0));
        let unhandled = s.stats.unhandled;
        s.apply_event(
            &parent_event(HOLDER, ITEM, RIGHT_HAND, 1, 3),
            LocalTime(0.0),
        );
        assert_eq!(s.stats.unhandled, unhandled);
        assert_eq!(s.stats.parent_events, 1);
        let item = s.presence(ObjectId(ITEM)).expect("the child remains known");
        assert_eq!(item.parent, Some((ObjectId(HOLDER), RIGHT_HAND)));
        assert_eq!(item.placement, 1);
        assert!(item.position.is_none());
        assert_eq!(item.position_ts, 3);
        let holder = s
            .presence(ObjectId(HOLDER))
            .expect("the holder remains known");
        assert!(holder.parent.is_none());
        assert!(holder.position.is_some());
        let attached = s.stats.unhandled == unhandled
            && s.stats.parent_events == 1
            && item.parent == Some((ObjectId(HOLDER), RIGHT_HAND))
            && item.placement == 1
            && item.position.is_none()
            && item.position_ts == 3
            && holder.parent.is_none()
            && holder.position.is_some();

        let mut reversed = objects();
        reversed.apply_event(
            &create_event(HOLDER, PERSON_SETUP, None, None),
            LocalTime(0.0),
        );
        reversed.apply_event(
            &create_event(ITEM, PERSON_SETUP, None, None),
            LocalTime(0.0),
        );
        reversed.apply_event(
            &parent_event(ITEM, HOLDER, RIGHT_HAND, 1, 3),
            LocalTime(0.0),
        );
        let reversed_child = reversed
            .presence(ObjectId(HOLDER))
            .expect("the reversed child exists");
        let reversed_holder = reversed
            .presence(ObjectId(ITEM))
            .expect("the reversed holder exists");
        assert_eq!(reversed_child.parent, Some((ObjectId(ITEM), RIGHT_HAND)));
        assert!(reversed_holder.parent.is_none());
        attached
            && reversed_child.parent == Some((ObjectId(ITEM), RIGHT_HAND))
            && reversed_holder.parent.is_none()
    }

    pub(super) fn old_and_incomplete_parent_events_change_nothing() -> bool {
        let mut stale = objects();
        stale.apply_event(
            &create_event(HOLDER, PERSON_SETUP, None, None),
            LocalTime(0.0),
        );
        stale.apply_event(&create_event(ITEM, ITEM_SETUP, None, None), LocalTime(0.0));
        stale.apply_event(
            &parent_event(HOLDER, ITEM, RIGHT_HAND, 1, 7),
            LocalTime(0.0),
        );
        stale.apply_event(&parent_event(HOLDER, ITEM, 2, 3, 7), LocalTime(0.0));
        stale.apply_event(&parent_event(HOLDER, ITEM, 2, 3, 4), LocalTime(0.0));
        assert_eq!(stale.stats.parent_events, 1);
        assert_eq!(stale.stats.parent_events_stale, 2);
        let item = stale
            .presence(ObjectId(ITEM))
            .expect("the attached child exists");
        assert_eq!(item.parent, Some((ObjectId(HOLDER), RIGHT_HAND)));
        assert_eq!(item.placement, 1);
        let stale_refused = stale.stats.parent_events == 1
            && stale.stats.parent_events_stale == 2
            && item.parent == Some((ObjectId(HOLDER), RIGHT_HAND))
            && item.placement == 1;

        let mut missing_item = objects();
        missing_item.apply_event(
            &create_event(HOLDER, PERSON_SETUP, None, None),
            LocalTime(0.0),
        );
        missing_item.apply_event(
            &parent_event(HOLDER, ITEM, RIGHT_HAND, 1, 3),
            LocalTime(0.0),
        );
        assert_eq!(missing_item.stats.parent_events_unknown_item, 1);
        assert_eq!(missing_item.stats.parent_events, 0);
        let missing_item_counted = missing_item.stats.parent_events_unknown_item == 1
            && missing_item.stats.parent_events == 0;

        let mut missing_holder = objects();
        missing_holder.apply_event(&create_event(ITEM, ITEM_SETUP, None, None), LocalTime(0.0));
        let before = missing_holder
            .presence(ObjectId(ITEM))
            .expect("the item exists");
        let before_parent = before.parent;
        let before_position = before.position;
        let before_placement = before.placement;
        missing_holder.apply_event(
            &parent_event(HOLDER, ITEM, RIGHT_HAND, 1, 3),
            LocalTime(0.0),
        );
        assert_eq!(missing_holder.stats.parent_events_unknown_creature, 1);
        assert_eq!(missing_holder.stats.parent_events, 0);
        let after = missing_holder
            .presence(ObjectId(ITEM))
            .expect("the item still exists");
        assert_eq!(after.parent, before_parent);
        assert_eq!(after.position, before_position);
        assert_eq!(after.placement, before_placement);
        let missing_holder_unchanged = missing_holder.stats.parent_events_unknown_creature == 1
            && missing_holder.stats.parent_events == 0
            && after.parent == before_parent
            && after.position == before_position
            && after.placement == before_placement;
        stale_refused && missing_item_counted && missing_holder_unchanged
    }

    pub(super) fn a_position_report_releases_an_object_from_its_parent() -> bool {
        use dereth_protocol::movement::{position_flags, MovementPositionEvent, PositionPack};

        let mut s = objects();
        s.apply_event(
            &create_event(HOLDER, PERSON_SETUP, None, None),
            LocalTime(0.0),
        );
        s.apply_event(
            &create_event(ITEM, ITEM_SETUP, Some((HOLDER, RIGHT_HAND)), Some(1)),
            LocalTime(0.0),
        );
        assert_eq!(
            s.presence(ObjectId(ITEM))
                .expect("the held item exists")
                .parent,
            Some((ObjectId(HOLDER), RIGHT_HAND))
        );
        let position = PositionPack {
            flags: position_flags::HAS_PLACEMENT_ID,
            origin: dereth_protocol::types::Origin {
                objcell_id: CELL,
                origin: dereth_protocol::types::Vec3::default(),
            },
            placement_id: Some(101),
            position_timestamp: 9,
            ..PositionPack::default()
        };
        let body = write_body(&MovementPositionEvent {
            id: ObjectId(ITEM),
            position,
        })
        .expect("the position event encodes");
        s.apply_event(
            &SessionEvent::WorldObject {
                opcode: Opcode::MOVEMENT_POSITION_EVENT,
                body,
            },
            LocalTime(0.0),
        );
        let item = s.presence(ObjectId(ITEM)).expect("the dropped item exists");
        assert!(item.parent.is_none());
        assert_eq!(
            item.position.map(|position| position.cell),
            Some(CellId(CELL))
        );
        assert_eq!(item.pending_placement, Some(101));
        item.parent.is_none()
            && item.position.map(|position| position.cell) == Some(CellId(CELL))
            && item.pending_placement == Some(101)
    }

    fn setup(id: u32) -> dereth_assets::Setup {
        let store = store();
        let did = DataId(id);
        let bytes = store
            .read_typed(dereth_dat::DbType::Setup, did)
            .expect("the setup reads");
        dereth_assets::Setup::decode_payload(did, &bytes).expect("the setup decodes")
    }

    fn moved(a: Vec3, b: Vec3) -> f32 {
        Vec3::new(a.x - b.x, a.y - b.y, a.z - b.z).magnitude()
    }

    pub(super) fn a_server_named_placement_draws_its_own_frames() -> bool {
        let store = store();
        let mut every_part = true;
        for (id, named) in [(MISSILE_SETUP, 52u32), (BOOKCASE_SETUP, 103)] {
            let did = DataId(id);
            let s = setup(id);
            let want = &s.placement_frames[&named].frames;
            let resting = &s.placement_frames[&PLACEMENT_RESTING].frames;
            assert_ne!(
                want, resting,
                "{did} named placement must differ from Resting"
            );
            let drawn = resolve_parts_at(&*store, did, named);
            assert_eq!(drawn.len(), s.parts.len());
            assert_eq!(want.len(), s.parts.len());
            for (index, part) in drawn.iter().enumerate() {
                assert_eq!(part.frame, want[index]);
                every_part &= part.frame == want[index];
            }
            let installed = resolve_parts(&*store, did);
            assert_eq!(installed.len(), s.parts.len());
            assert_eq!(resting.len(), s.parts.len());
            for (index, part) in installed.iter().enumerate() {
                assert_eq!(part.frame, resting[index]);
                every_part &= part.frame == resting[index];
            }
            let moves = drawn
                .iter()
                .zip(&installed)
                .map(|(a, b)| moved(a.frame.origin, b.frame.origin))
                .fold(0.0f32, f32::max)
                > 0.0;
            assert!(moves);
            every_part &= want != resting && moves;
        }
        every_part
    }

    struct PickAt {
        viewer: Frame,
        frames: BTreeMap<ObjectId, Frame>,
    }

    impl PickScene for PickAt {
        fn viewer(&self) -> Frame {
            self.viewer
        }

        fn object_frame(&self, id: ObjectId) -> Option<Frame> {
            self.frames.get(&id).copied()
        }

        fn fov_y_rad(&self, _viewport: (u32, u32)) -> f32 {
            45.0f32.to_radians()
        }
    }

    fn sphere_centre(placement: u32) -> (Vec3, f32) {
        let store = store();
        let s = setup(PICK_SETUP);
        let frame = s.placement_frames[&placement].frames[0];
        let bytes = store
            .read_typed(dereth_dat::DbType::GfxObj, s.parts[0])
            .expect("the pick part reads");
        let gfx = dereth_assets::GfxObj::decode_payload(s.parts[0], &bytes)
            .expect("the pick part decodes");
        let sphere = gfx
            .drawing_bsp
            .as_ref()
            .and_then(|tree| tree.nodes.first().and_then(|node| node.sphere))
            .expect("the pick part has a drawing sphere");
        let rotated = rotate(frame.rotation, sphere.center);
        #[allow(clippy::cast_possible_truncation)]
        let centre = Vec3::new(
            frame.origin.x + rotated.0 as f32,
            frame.origin.y + rotated.1 as f32,
            frame.origin.z + rotated.2 as f32,
        );
        (centre, sphere.radius)
    }

    fn pick_scene(centre: Vec3) -> PickAt {
        PickAt {
            viewer: Frame::new(
                Vec3::new(centre.x, centre.y - 3.0, centre.z),
                Quat::IDENTITY,
            ),
            frames: [(ObjectId(ITEM), Frame::new(Vec3::ZERO, Quat::IDENTITY))]
                .into_iter()
                .collect(),
        }
    }

    pub(super) fn a_pick_sweep_uses_the_placement_the_shard_named() -> bool {
        let store = store();
        let (combat, radius) = sphere_centre(1);
        let (resting, _) = sphere_centre(101);
        let lateral = ((combat.x - resting.x).powi(2) + (combat.z - resting.z).powi(2)).sqrt();
        assert!(lateral > radius + 0.2);
        let screen = (800u32, 600u32);
        let viewport = Viewport {
            x: 0,
            y: 0,
            width: screen.0,
            height: screen.1,
        };
        let scene = pick_scene(combat);
        let mut pick = WorldPicker::new();

        let mut posed = objects();
        posed.apply_event(
            &create_event(ITEM, PICK_SETUP, None, Some(1)),
            LocalTime(0.0),
        );
        let armed_hit = pick.find_object(399, 299, viewport);
        assert!(armed_hit);
        let hit = pick.draw_no_blit(&*store, &scene, &posed, screen, viewport);
        assert_eq!(hit, Some(ObjectId(ITEM)));

        let mut resting_objects = objects();
        resting_objects.apply_event(
            &create_event(ITEM, PICK_SETUP, None, Some(101)),
            LocalTime(0.0),
        );
        let armed_miss = pick.find_object(399, 299, viewport);
        assert!(armed_miss);
        let miss = pick.draw_no_blit(&*store, &scene, &resting_objects, screen, viewport);
        assert_eq!(miss, Some(ObjectId(0)));
        lateral > radius + 0.2
            && armed_hit
            && hit == Some(ObjectId(ITEM))
            && armed_miss
            && miss == Some(ObjectId(0))
    }

    pub(super) fn a_held_object_is_pickable_and_a_contained_object_is_not() -> bool {
        let store = store();
        let (combat, _) = sphere_centre(1);
        let screen = (800u32, 600u32);
        let viewport = Viewport {
            x: 0,
            y: 0,
            width: screen.0,
            height: screen.1,
        };
        let scene = pick_scene(combat);

        let mut held = objects();
        held.apply_event(
            &create_event(HOLDER, PERSON_SETUP, None, None),
            LocalTime(0.0),
        );
        held.apply_event(
            &create_event(ITEM, PICK_SETUP, Some((HOLDER, RIGHT_HAND)), Some(1)),
            LocalTime(0.0),
        );
        let item = held.presence(ObjectId(ITEM)).expect("the held item exists");
        let really_held = item.parent.is_some() && item.position.is_none();
        assert!(really_held);
        let mut pick = WorldPicker::new();
        let armed_held = pick.find_object(399, 299, viewport);
        assert!(armed_held);
        let held_pick = pick.draw_no_blit(&*store, &scene, &held, screen, viewport);
        assert_eq!(held_pick, Some(ObjectId(ITEM)));

        let mut contained = objects();
        contained.apply_event(&contained_create(ITEM, PICK_SETUP), LocalTime(0.0));
        let mut pick = WorldPicker::new();
        let armed_contained = pick.find_object(399, 299, viewport);
        assert!(armed_contained);
        let contained_pick = pick.draw_no_blit(&*store, &scene, &contained, screen, viewport);
        assert_eq!(contained_pick, Some(ObjectId(0)));
        really_held
            && armed_held
            && held_pick == Some(ObjectId(ITEM))
            && armed_contained
            && contained_pick == Some(ObjectId(0))
    }
}

pub fn an_out_of_range_holding_part_uses_the_holders_frame() {
    let held = held::an_out_of_range_holding_part_uses_the_holders_frame();
    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "objects.held-frame.an-out-of-range-part-uses-the-holders-own-frame",
        move |_| held,
    );
}

pub fn a_new_parent_event_attaches_the_child_in_wire_order() {
    let attached = held::a_new_parent_event_attaches_the_child_in_wire_order();
    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "objects.parent.a-new-parent-event-attaches-the-child-in-wire-order",
        move |_| attached,
    );
}

pub fn old_and_incomplete_parent_events_change_nothing() {
    let unchanged = held::old_and_incomplete_parent_events_change_nothing();
    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "objects.parent.an-old-or-incomplete-parent-event-changes-nothing",
        move |_| unchanged,
    );
}

pub fn a_position_report_releases_an_object_from_its_parent() {
    let released = held::a_position_report_releases_an_object_from_its_parent();
    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "objects.position.a-position-report-releases-an-object-from-its-parent",
        move |_| released,
    );
}

pub fn a_server_named_placement_draws_its_own_frames() {
    let placed = held::a_server_named_placement_draws_its_own_frames();
    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "objects.placement.a-server-named-placement-draws-its-own-frames",
        move |_| placed,
    );
}

pub fn a_pick_sweep_uses_the_placement_the_shard_named() {
    let picked = held::a_pick_sweep_uses_the_placement_the_shard_named();
    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "objects.pick.a-sweep-uses-the-placement-the-shard-named",
        move |_| picked,
    );
}

pub fn a_held_object_is_pickable_and_a_contained_object_is_not() {
    let picked = held::a_held_object_is_pickable_and_a_contained_object_is_not();
    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "objects.held-object.is-a-pick-candidate-while-a-contained-object-is-not",
        move |_| picked,
    );
}
