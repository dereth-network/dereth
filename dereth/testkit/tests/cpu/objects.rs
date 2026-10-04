//! The object stream: what the client does with a reference it cannot resolve, how it echoes the
//! shard's position generations back, what the panels read once a frame, what it says about a
//! message nothing here receives, creation, state changes, removal, housekeeping and replacement of
//! an object, the ordered UI replies, the teardown at log-off, and what the player may attack.
//!
//! Most scenarios drive a model-only client with messages delivered through `Inbound`; the
//! ordered-reply and teardown scenarios replay the recorded corpus through the client's own
//! endpoint. No data file is opened.

use dereth_client::dropped::{self, Site};
use dereth_client_model::{
    Notice, NullSink, RecordingRequests, RecordingSink, Request, Weenie, World,
};
use dereth_client_net::client_session::SessionEvent;
use dereth_primitives::{ObjectId, ServerTime};
use dereth_protocol::objects::{ItemCreateObject, ItemObjDescEvent, ObjectCreatePayload};
use dereth_protocol::types::physicsdesc::{flags, PhysicsDesc, PhysicsTimestamps};
use dereth_protocol::types::PublicWeenieDesc;
use dereth_protocol::types::{AnimPartChange, ContentProfile, ObjDesc, Origin, PositionWire};
use dereth_protocol::Opcode;
use dereth_testkit::{HeadlessClient, Inbound};

// =============================================================================================
// 1. objects.dangling-reference.is-asked-about-once-and-then-dropped
// =============================================================================================

/// The reference is asked about twenty seconds after it appears, not before, and the placeholder
/// is thrown away five seconds after that.
pub fn a_dangling_reference_is_asked_about_once() {
    const ORPHAN: ObjectId = ObjectId(0x8000_0BAD);
    const T0: f64 = 100.0;

    let mut c = HeadlessClient::model();
    let mut sink = NullSink;
    c.world_mut().get_null_weenie_object(ORPHAN, ServerTime(T0));

    // Frame cadence, the way the client's own maintenance runs it. Across the whole interval
    // before the twenty seconds: silence.
    let mut before = RecordingRequests::default();
    for i in 1..=200 {
        c.world_mut()
            .use_time(ServerTime(T0 + f64::from(i) * 0.1), &mut sink, &mut before);
    }
    let silent_before = before.0.is_empty();

    // Across the next interval: exactly one, naming the orphan.
    let mut after = RecordingRequests::default();
    for i in 201..=400 {
        c.world_mut()
            .use_time(ServerTime(T0 + f64::from(i) * 0.1), &mut sink, &mut after);
    }
    let asked: Vec<ObjectId> = after
        .0
        .iter()
        .filter_map(|r| match r {
            Request::ForceObjdesc(m) => Some(m.id),
            _ => None,
        })
        .collect();

    // And then it stops, because there is nothing left to ask about.
    let mut third = RecordingRequests::default();
    for i in 401..=600 {
        c.world_mut()
            .use_time(ServerTime(T0 + f64::from(i) * 0.1), &mut sink, &mut third);
    }
    let then_quiet = third.0.is_empty();

    // Both edges are strict, and both are measured from the moment the placeholder was made --
    // which is the current clock and not zero.
    let mut edges = HeadlessClient::model();
    const HOLDER: ObjectId = ObjectId(0x8000_0BAE);
    let mut edge_requests = RecordingRequests::default();
    let strict = {
        let w = edges.world_mut();
        let mut p = ObjectCreatePayload {
            id: HOLDER,
            ..ObjectCreatePayload::default()
        };
        p.physicsdesc = PhysicsDesc {
            children: Some(vec![dereth_protocol::types::physicsdesc::ChildLink {
                child_id: ORPHAN,
                location_id: 1,
            }]),
            ..PhysicsDesc::default()
        };
        w.create_or_merge(&p, ServerTime(1_000.0), &mut sink)
            .expect("the holder is created");
        w.use_time(ServerTime(1_000.1), &mut sink, &mut edge_requests);
        let fresh = edge_requests.0.is_empty();
        w.get_null_physics_object(ORPHAN, ServerTime(1_019.0));
        let stamped = w.tables.null_physics.get(ORPHAN).map(|p| p.update_time)
            == Some(ServerTime(1_000.0))
            && w.tables.doomed.get(ORPHAN) == Some(&ServerTime(1_025.0));
        w.use_time(ServerTime(1_020.0), &mut sink, &mut edge_requests);
        let equality_waits = edge_requests.0.is_empty();
        w.use_time(ServerTime(1_020.001), &mut sink, &mut edge_requests);
        let asked_after = edge_requests.0
            == vec![Request::ForceObjdesc(
                dereth_protocol::objects::ObjectSendForceObjdesc { id: ORPHAN },
            )];
        w.use_time(ServerTime(1_025.0), &mut sink, &mut edge_requests);
        let deadline_equality = w.tables.null_physics.contains_key(ORPHAN);
        w.use_time(ServerTime(1_025.001), &mut sink, &mut edge_requests);
        let destroyed = !w.tables.null_physics.contains_key(ORPHAN);
        fresh && stamped && equality_waits && asked_after && deadline_equality && destroyed
    };

    c.assert_behaviour(
        "objects.dangling-reference.is-asked-about-once-and-then-dropped",
        move |_| silent_before && asked == [ORPHAN] && then_quiet && strict,
    );
}

// =============================================================================================
// 2. objects.position.teleport-and-force-stamps-follow-the-shards-generations
// =============================================================================================

const MOVER: ObjectId = ObjectId(0x5000_0002);
const OTHER: ObjectId = ObjectId(0x5000_0099);
const CELL: u32 = 0x00A9_0125;

fn create_with(id: ObjectId, timestamps: PhysicsTimestamps) -> ItemCreateObject {
    ItemCreateObject(ObjectCreatePayload {
        id,
        objdesc: ObjDesc::default(),
        physicsdesc: PhysicsDesc {
            bitfield: flags::POSITION,
            position: Some(PositionWire {
                objcell_id: CELL,
                frame: dereth_protocol::types::Frame::default(),
            }),
            timestamps,
            ..PhysicsDesc::default()
        },
        wdesc: dereth_protocol::types::PublicWeenieDesc::default(),
    })
}

fn position_event(
    id: ObjectId,
    position_ts: u16,
    teleport_ts: u16,
    force_ts: u16,
) -> dereth_protocol::movement::MovementPositionEvent {
    dereth_protocol::movement::MovementPositionEvent {
        id,
        position: dereth_protocol::movement::PositionPack {
            flags: 0,
            origin: Origin {
                objcell_id: CELL,
                origin: dereth_protocol::types::Vec3::default(),
            },
            orientation: dereth_protocol::types::Quat::default(),
            velocity: None,
            placement_id: None,
            instance_timestamp: 1,
            position_timestamp: position_ts,
            teleport_timestamp: teleport_ts,
            force_position_timestamp: force_ts,
        },
    }
}

fn stamps(c: &HeadlessClient, id: ObjectId) -> Option<(u16, u16, u16)> {
    c.view()
        .objects()
        .presence(id)
        .map(|p| (p.position_ts, p.teleport_ts, p.force_position_ts))
}

/// The create seeds both generations, only a newer message moves them, and the forced-position
/// one belongs to the player's own body alone.
pub fn position_generations_follow_the_shard() {
    let mut c = HeadlessClient::model();
    c.when(Inbound::event(SessionEvent::PlayerCreated(MOVER)));
    // The nine slots the create carries, each deliberately different, so a slot wired to the
    // wrong neighbour cannot pass.
    c.when(Inbound::world_view(&create_with(
        MOVER,
        PhysicsTimestamps {
            position: 7,
            movement: 0,
            state: 0,
            vector: 0,
            teleport: 5,
            server_controlled_move: 0,
            force_position: 9,
            objdesc: 0,
            instance: 1,
        },
    )));
    let seeded = stamps(&c, MOVER) == Some((7, 5, 9));

    // An equal stamp is not newer, and neither is an older one.
    c.when(Inbound::world_view(&position_event(MOVER, 8, 5, 9)));
    let equal_is_not_newer = stamps(&c, MOVER).map(|s| (s.1, s.2)) == Some((5, 9));
    c.when(Inbound::world_view(&position_event(MOVER, 9, 4, 8)));
    let older_is_not_newer = stamps(&c, MOVER).map(|s| (s.1, s.2)) == Some((5, 9));

    // A newer forced-position generation is taken even when the position itself is rejected.
    c.when(Inbound::world_view(&position_event(MOVER, 10, 4, 11)));
    let force_alone = stamps(&c, MOVER) == Some((8, 5, 11));
    // …and a newer teleport generation advances with it.
    c.when(Inbound::world_view(&position_event(MOVER, 10, 6, 11)));
    let both_advance = stamps(&c, MOVER).map(|s| (s.1, s.2)) == Some((6, 11));
    let counted = c.view().objects().stats.teleport_stamps == 1
        && c.view().objects().stats.force_position_stamps == 1;

    // The same message to a different object moves the teleport generation and not the forced
    // one: only a second object can tell those two rules apart.
    c.when(Inbound::world_view(&create_with(
        OTHER,
        PhysicsTimestamps {
            instance: 1,
            ..PhysicsTimestamps::default()
        },
    )));
    c.when(Inbound::world_view(&position_event(OTHER, 3, 4, 8)));
    let others_body = stamps(&c, OTHER).map(|s| (s.1, s.2)) == Some((4, 0))
        && c.view().objects().stats.force_position_stamps == 1;

    // The announcement that a teleport is coming advances neither.
    let mut announced = HeadlessClient::model();
    announced.when(Inbound::event(SessionEvent::PlayerCreated(MOVER)));
    announced.when(Inbound::world_view(&create_with(
        MOVER,
        PhysicsTimestamps {
            teleport: 2,
            instance: 1,
            ..PhysicsTimestamps::default()
        },
    )));
    announced.when(Inbound::world_view(
        &dereth_protocol::objects::EffectsPlayerTeleport {
            teleport_sequence: 7,
        },
    ));
    let announcement_is_not_a_move = stamps(&announced, MOVER).map(|s| s.1) == Some(2)
        && announced.view().objects().stats.teleport_stamps == 0;

    c.assert_behaviour(
        "objects.position.teleport-and-force-stamps-follow-the-shards-generations",
        move |_| {
            seeded
                && equal_is_not_newer
                && older_is_not_newer
                && force_alone
                && both_advance
                && counted
                && others_body
                && announcement_is_not_a_move
        },
    );
}

// =============================================================================================
// 3. objects.panels.poll-the-world-rather-than-waiting-for-a-notice
// =============================================================================================

const PLAYER: ObjectId = ObjectId(0x5000_0001);
const CONTAINER: ObjectId = ObjectId(0x5000_0002);
const ITEM: ObjectId = ObjectId(0x5000_0003);
const SIDE_PACK: ObjectId = ObjectId(0x5000_0004);

/// An appearance that survives the wire and is distinguishable from the default one.
fn appearance(part_id: u32) -> ObjDesc {
    ObjDesc {
        anim_part_changes: vec![AnimPartChange {
            part_index: 0,
            part_id,
        }],
        ..ObjDesc::default()
    }
}

fn named(w: &mut World, id: ObjectId, name: &str) {
    let mut it = Weenie::new(id);
    it.valid = true;
    it.pwd.name = name.to_owned();
    w.tables.weenies.insert(id, it);
}

/// The scene and the panels read the world once a frame; every change below is complete with the
/// notice that names it thrown away.
pub fn the_panels_poll_rather_than_wait() {
    // ---- a create ---------------------------------------------------------------------------
    let payload = {
        let mut p = ObjectCreatePayload {
            id: ITEM,
            objdesc: ObjDesc::default(),
            physicsdesc: PhysicsDesc::default(),
            wdesc: dereth_protocol::types::PublicWeenieDesc::default(),
        };
        p.physicsdesc.timestamps.instance = 7;
        p
    };
    let raised = {
        let mut w = World::new();
        let mut out = RecordingSink::default();
        w.recreate(&payload, ServerTime(1.0), &mut out)
            .expect("the object is created");
        out.0.contains(&Notice::ObjectCreated(ITEM))
    };
    let mut c = HeadlessClient::model();
    c.when(Inbound::world_view(&ItemCreateObject(payload)));
    // The queue holds the create with no notice consumer anywhere, and is drained by the frame
    // that read it -- one create, one geometry build.
    let offered_once =
        c.objects_mut().take_created() == vec![ITEM] && c.objects_mut().take_created().is_empty();

    // ---- a container's contents -------------------------------------------------------------
    let profiles = [
        ContentProfile {
            iid: ITEM,
            container_properties: 0,
        },
        ContentProfile {
            iid: SIDE_PACK,
            container_properties: 1,
        },
    ];
    let inventory_raised = {
        let mut w = World::new();
        named(&mut w, CONTAINER, "a chest");
        let mut out = RecordingSink::default();
        w.view_object_contents(CONTAINER, &profiles, &mut out);
        let opened = out
            .0
            .iter()
            .filter(|n| **n == Notice::InventoryChanged(CONTAINER))
            .count();
        let mut out = RecordingSink::default();
        w.stop_viewing_object_contents(CONTAINER, &mut out);
        opened == 1 && out.0 == vec![Notice::InventoryChanged(CONTAINER)]
    };
    // …and the same two calls with the sink production actually passes: the polled state is
    // identical.
    let polled_state_is_complete = {
        let mut c = HeadlessClient::model();
        named(c.world_mut(), CONTAINER, "a chest");
        c.world_mut()
            .view_object_contents(CONTAINER, &profiles, &mut NullSink);
        let filled = c
            .view()
            .world()
            .inventory(CONTAINER)
            .is_some_and(|inv| inv.items == vec![ITEM] && inv.containers == vec![SIDE_PACK]);
        c.world_mut()
            .stop_viewing_object_contents(CONTAINER, &mut NullSink);
        filled && c.view().world().inventory(CONTAINER).is_none()
    };

    // ---- the player's own appearance --------------------------------------------------------
    let appearance_raised = {
        let mut w = World::new();
        let mut out = RecordingSink::default();
        w.set_player_visual_desc(appearance(0x0100_0042), &mut out);
        out.0 == vec![Notice::PlayerObjDescChanged] && w.player_objdesc.is_some()
    };
    let mut dressed = HeadlessClient::model();
    dressed.when(Inbound::event(SessionEvent::PlayerCreated(PLAYER)));
    dressed.when(Inbound::world_view(&ItemCreateObject({
        let mut p = ObjectCreatePayload {
            id: PLAYER,
            objdesc: appearance(0x0100_0042),
            physicsdesc: PhysicsDesc::default(),
            wdesc: dereth_protocol::types::PublicWeenieDesc::default(),
        };
        p.physicsdesc.timestamps.instance = 3;
        p
    })));
    let whose_body = dressed.view().objects().player() == Some(PLAYER)
        && dressed.objects_mut().take_created() == vec![PLAYER];
    let new_clothes = appearance(0x0100_00A5);
    dressed.when(Inbound::world_view(&ItemObjDescEvent {
        id: PLAYER,
        objdesc: new_clothes.clone(),
        timestamps: dereth_protocol::types::PhysicsEventStamp::default(),
    }));
    let re_offered = dressed.objects_mut().take_created() == vec![PLAYER]
        && dressed
            .view()
            .objects()
            .presence(PLAYER)
            .map(|p| p.objdesc.clone())
            == Some(new_clothes);

    c.assert_behaviour(
        "objects.panels.poll-the-world-rather-than-waiting-for-a-notice",
        move |_| {
            raised
                && offered_once
                && inventory_raised
                && polled_state_is_complete
                && appearance_raised
                && whose_body
                && re_offered
        },
    );
}

// =============================================================================================
// 4. diagnostics.unreceived-message.names-itself-once-with-its-site
// =============================================================================================

/// An opcode no router in this workspace has an arm for and the catalogue does not carry either.
const NOWHERE: Opcode = Opcode(0xEA61);
/// A second one, for the "per message, not once ever" half.
const NOWHERE_TOO: Opcode = Opcode(0xEA62);

/// One arrival at both UI-queue consumers, which is what it takes for the ledger to conclude that
/// a message reaches no receiver at all.
fn deliver_unknown(opcode: Opcode) {
    let mut blob = opcode.0.to_le_bytes().to_vec();
    blob.extend(std::iter::repeat_n(0_u8, 64));
    let mut c = HeadlessClient::model();
    c.when(Inbound::event(SessionEvent::UiEvent { opcode, blob }));
}

/// A message with no receiver is reported once, with its site and its count.
///
/// The ledger is a process-global. This scenario takes no lock: it runs once per process, as its
/// own `#[test]`, because the census lists scenarios and does not run them, so no second run of
/// *itself* can clear the ledger under it.
pub fn an_unreceived_message_names_itself_once() {
    dropped::clear();
    let starts_empty = dropped::announcements().is_empty();

    deliver_unknown(NOWHERE);
    let lines = dropped::announcements();
    let one_line = lines.len() == 1
        && lines[0].starts_with("inbound ")
        && lines[0].contains("0xEA61")
        && lines[0].contains("not in the opcode table")
        && (lines[0].contains("ui_event") || lines[0].contains("interaction"))
        && lines[0].contains("1 so far")
        && lines[0].contains("reached no receiver");
    let both_sites = dropped::count(Site::UiEvent, NOWHERE) == 1
        && dropped::count(Site::Interaction, NOWHERE) == 1
        && dropped::unreceived(NOWHERE);

    // Ten arrivals cost one line and ten counts at each site.
    dropped::clear();
    for _ in 0..10 {
        deliver_unknown(NOWHERE);
    }
    let no_spam = dropped::announcements().len() == 1
        && dropped::count(Site::UiEvent, NOWHERE) == 10
        && dropped::count(Site::Interaction, NOWHERE) == 10
        && dropped::total() == 20;

    // A second message gets its own line, in arrival order.
    dropped::clear();
    deliver_unknown(NOWHERE);
    deliver_unknown(NOWHERE_TOO);
    deliver_unknown(NOWHERE);
    let lines = dropped::announcements();
    let per_message =
        lines.len() == 2 && lines[0].contains("0xEA61") && lines[1].contains("0xEA62");

    // …and the instrument can stay quiet, which is what makes its silence a result.
    let mut quiet = true;
    for opcode in [
        Opcode::FELLOWSHIP_FULL_UPDATE,
        Opcode::COMMUNICATION_POP_UP_STRING,
        Opcode::HOUSE_HOUSE_STATUS,
        Opcode::FELLOWSHIP_FELLOW_UPDATE_DONE,
    ] {
        dropped::clear();
        deliver_unknown(opcode);
        quiet &= dropped::announcements().is_empty() && !dropped::unreceived(opcode);
    }

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "diagnostics.unreceived-message.names-itself-once-with-its-site",
        move |_| starts_empty && one_line && both_sites && no_spam && per_message && quiet,
    );
}

// -------------------------------------------------------------------------------------------

dereth_testkit::scenarios! {
    scenario_a_dangling_reference_is_asked_about_once => a_dangling_reference_is_asked_about_once ["objects.dangling-reference.is-asked-about-once-and-then-dropped"],
    scenario_position_generations_follow_the_shard => position_generations_follow_the_shard ["objects.position.teleport-and-force-stamps-follow-the-shards-generations"],
    scenario_the_panels_poll_rather_than_wait => the_panels_poll_rather_than_wait ["objects.panels.poll-the-world-rather-than-waiting-for-a-notice"],
    scenario_an_unreceived_message_names_itself_once => an_unreceived_message_names_itself_once ["diagnostics.unreceived-message.names-itself-once-with-its-site"],
    scenario_a_second_create_carries_the_new_word => a_second_create_carries_the_new_word ["object.create.a-second-create-carries-the-new-word-to-the-one-table"],
    scenario_a_state_change_that_is_not_newer_is_refused => a_state_change_that_is_not_newer_is_refused ["object.set-state.the-stamp-gate-refuses-a-change-that-is-not-newer"],
    scenario_a_removal_detaches_the_item_and_defers_the_deletion => a_removal_detaches_the_item_and_defers_the_deletion ["objects.deletion.the-shards-remove-detaches-the-item-and-defers-the-rest"],
    scenario_the_housekeeping_deadlines_are_strict => the_housekeeping_deadlines_are_strict ["objects.maintenance.every-deadline-is-strict-and-a-child-that-left-is-spared"],
    scenario_a_newer_instance_replaces_and_the_old_ones_traffic_is_dropped => a_newer_instance_replaces_and_the_old_ones_traffic_is_dropped ["objects.replacement.a-newer-instance-wins-and-the-old-ones-traffic-is-dropped"],
    scenario_an_ordered_reply_waits_for_the_object_it_is_about => an_ordered_reply_waits_for_the_object_it_is_about ["objects.ordered-replies.wait-for-an-object-that-has-not-arrived-yet"],
    scenario_ordered_replies_arrive_in_the_shards_own_order => ordered_replies_arrive_in_the_shards_own_order ["objects.ordered-replies.arrive-in-the-order-the-shard-stamped-them"],
    scenario_every_recorded_reply_reaches_a_consumer => every_recorded_reply_reaches_a_consumer ["objects.ordered-replies.every-recorded-reply-reaches-a-consumer"],
    scenario_the_panels_and_the_scroll_take_the_shards_values => the_panels_and_the_scroll_take_the_shards_values ["objects.ordered-replies.the-panels-and-the-scroll-take-the-shards-own-values"],
    scenario_the_world_is_torn_down_when_the_character_leaves => the_world_is_torn_down_when_the_character_leaves ["session.end.the-world-the-character-left-is-torn-down"],
    scenario_what_the_client_will_let_you_attack => what_the_client_will_let_you_attack ["selection.attackable.the-clients-own-test-is-more-than-is-it-a-creature"],
    scenario_force_objdesc_leaves_on_the_control_queue => force_objdesc_leaves_on_the_control_queue ["objects.force-objdesc.leaves-on-the-control-queue"],
    scenario_set_state_moves_the_visible_list => set_state_moves_the_visible_list ["object.set-state.visible-list-follows"],
}

// =============================================================================================
// Creation, state, removal, housekeeping and replacement
// =============================================================================================

/// The physics word bit the visible-object sweep reads, as a literal. Reading it back through the
/// constant that defines it could not notice a wrong constant, which is why it is written out
/// here.
const STATIC_PS: u32 = 0x0000_0001;
/// A word the recordings really carry -- a door standing open -- rather than an invented one.
const DOOR_OPEN: u32 = 0x0001_001C;
/// Another one: the same door shut.
const DOOR_SHUT: u32 = 0x0001_0018;
/// The client reads a stamp difference of this much or more as a wrap, and the comparison inverts.
const IS_NEWER_HALF_PERIOD: u16 = 0x8000;

/// A create for `id` carrying `state`, whose descriptor's state stamp is `state_ts`.
fn create_with_state(id: u32, state: u32, cell: Option<u32>, state_ts: u16) -> Vec<u8> {
    let mut physicsdesc = PhysicsDesc {
        state,
        timestamps: dereth_protocol::types::PhysicsTimestamps {
            state: state_ts,
            ..dereth_protocol::types::PhysicsTimestamps::default()
        },
        ..PhysicsDesc::default()
    };
    if let Some(c) = cell {
        physicsdesc.bitfield |= flags::POSITION;
        physicsdesc.position = Some(PositionWire {
            objcell_id: c,
            frame: dereth_protocol::types::Frame::default(),
        });
    }
    dereth_protocol::write_body(&ItemCreateObject(ObjectCreatePayload {
        id: ObjectId(id),
        objdesc: ObjDesc::default(),
        physicsdesc,
        wdesc: dereth_protocol::types::PublicWeenieDesc::default(),
    }))
    .expect("the encoder writes what the decoder reads")
}

/// A state change for `id` carrying `state`, stamped `event`.
fn set_state_with(id: u32, state: u32, event: u16) -> Vec<u8> {
    dereth_protocol::write_body(&dereth_protocol::objects::ItemSetState {
        id: ObjectId(id),
        state,
        timestamps: dereth_protocol::types::PhysicsEventStamp { instance: 0, event },
    })
    .expect("the encoder writes what the decoder reads")
}

/// Hand one encoded body to the object stream on the smart-box queue, as the session does.
fn feed_body(s: &mut dereth_client::objects::ObjectStream, opcode: Opcode, body: Vec<u8>) {
    s.apply_event(
        &SessionEvent::WorldObject { opcode, body },
        dereth_primitives::LocalTime(0.0),
    );
}

fn is_visible(s: &dereth_client::objects::ObjectStream, id: u32) -> bool {
    s.world.tables.visible.contains(&ObjectId(id))
}

// =============================================================================================
// object.create.a-second-create-carries-the-new-word-to-the-one-table
// =============================================================================================

/// A second create for a live object is a change to it: the descriptor wins and the sweep follows.
///
/// The calibration first -- the sweep has to be shown answering **both** ways from a create word
/// alone, or "it is not in the list" and "this bench never lists anything" are the same reading.
pub fn a_second_create_carries_the_new_word() {
    use dereth_client::objects::ObjectStream;

    const PLAIN: u32 = 0x7000_0001;
    const STATIC: u32 = 0x7000_0002;
    const MERGED: u32 = 0x7000_0020;
    const CELL: u32 = 0xA9B4_0100;

    let one = |id: u32, state: u32| -> ObjectStream {
        let mut s = ObjectStream::new();
        feed_body(
            &mut s,
            Opcode::ITEM_CREATE_OBJECT,
            create_with_state(id, state, Some(CELL), 0),
        );
        s.world.update_visible_object_list();
        s
    };

    // The calibration.
    let listed = is_visible(&one(PLAIN, DOOR_OPEN), PLAIN);
    let not_listed = !is_visible(&one(STATIC, DOOR_OPEN | STATIC_PS), STATIC);

    // The claim.
    let mut s = one(MERGED, DOOR_OPEN);
    let starts_listed = is_visible(&s, MERGED);
    feed_body(
        &mut s,
        Opcode::ITEM_CREATE_OBJECT,
        create_with_state(MERGED, DOOR_OPEN | STATIC_PS, Some(CELL), 1),
    );
    let merged = s.stats.merges == 1;
    let word_moved = s.physics_state(ObjectId(MERGED)) == Some(DOOR_OPEN | STATIC_PS);
    s.world.update_visible_object_list();
    let sweep_followed = !is_visible(&s, MERGED);

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "object.create.a-second-create-carries-the-new-word-to-the-one-table",
        move |_| listed && not_listed && starts_listed && merged && word_moved && sweep_followed,
    );
}

// =============================================================================================
// object.set-state.the-stamp-gate-refuses-a-change-that-is-not-newer
// =============================================================================================

/// The gate on both sides of the half-period, and the create's own stamp as its starting value.
pub fn a_state_change_that_is_not_newer_is_refused() {
    use dereth_client::objects::ObjectStream;

    let word = |s: &ObjectStream, id: u32| s.physics_state(ObjectId(id));
    let seeded = |id: u32, state_ts: u16| -> ObjectStream {
        let mut s = ObjectStream::new();
        feed_body(
            &mut s,
            Opcode::ITEM_CREATE_OBJECT,
            create_with_state(id, DOOR_SHUT, None, state_ts),
        );
        s
    };

    // (a) ordinary ordering: one beats the create's zero, and one does not beat itself.
    let mut a = seeded(0x7000_0001, 0);
    feed_body(
        &mut a,
        Opcode::ITEM_SET_STATE,
        set_state_with(0x7000_0001, DOOR_OPEN, 1),
    );
    let opened = word(&a, 0x7000_0001) == Some(DOOR_OPEN) && a.stats.state_events == 1;
    feed_body(
        &mut a,
        Opcode::ITEM_SET_STATE,
        set_state_with(0x7000_0001, DOOR_SHUT, 1),
    );
    let equal_refused = word(&a, 0x7000_0001) == Some(DOOR_OPEN) && a.stats.state_events_stale == 1;

    // (b) one below the half-period is ordinary ordering and is taken.
    let mut b = seeded(0x7000_0002, 0);
    feed_body(
        &mut b,
        Opcode::ITEM_SET_STATE,
        set_state_with(0x7000_0002, DOOR_OPEN, IS_NEWER_HALF_PERIOD - 1),
    );
    let just_inside = word(&b, 0x7000_0002) == Some(DOOR_OPEN);

    // (c) exactly the half-period is a wrap, so it reads as older and is refused.
    let mut d = seeded(0x7000_0003, 0);
    feed_body(
        &mut d,
        Opcode::ITEM_SET_STATE,
        set_state_with(0x7000_0003, DOOR_OPEN, IS_NEWER_HALF_PERIOD),
    );
    let at_the_edge = word(&d, 0x7000_0003) == Some(DOOR_SHUT) && d.stats.state_events_stale == 1;

    // (d) the create seeds the gate, so a change stamped below it is refused and one above is not.
    let mut e = seeded(0x7000_0004, 100);
    let create_seeds = e
        .presence(ObjectId(0x7000_0004))
        .is_some_and(|p| p.state_ts == 100);
    feed_body(
        &mut e,
        Opcode::ITEM_SET_STATE,
        set_state_with(0x7000_0004, DOOR_OPEN, 50),
    );
    let below_refused = word(&e, 0x7000_0004) == Some(DOOR_SHUT) && e.stats.state_events_stale == 1;
    feed_body(
        &mut e,
        Opcode::ITEM_SET_STATE,
        set_state_with(0x7000_0004, DOOR_OPEN, 101),
    );
    let above_taken = word(&e, 0x7000_0004) == Some(DOOR_OPEN);

    // And a refused change moves neither the word nor the list the sweep keeps.
    let mut f = ObjectStream::new();
    feed_body(
        &mut f,
        Opcode::ITEM_CREATE_OBJECT,
        create_with_state(0x7000_0013, DOOR_OPEN, Some(0xA9B4_0100), 5),
    );
    f.world.update_visible_object_list();
    let starts_listed = is_visible(&f, 0x7000_0013);
    feed_body(
        &mut f,
        Opcode::ITEM_SET_STATE,
        set_state_with(0x7000_0013, DOOR_OPEN | STATIC_PS, 3),
    );
    f.world.update_visible_object_list();
    let sweep_unmoved =
        is_visible(&f, 0x7000_0013) && f.stats.state_events == 0 && f.stats.state_events_stale == 1;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "object.set-state.the-stamp-gate-refuses-a-change-that-is-not-newer",
        move |_| {
            opened
                && equal_refused
                && just_inside
                && at_the_edge
                && create_seeds
                && below_refused
                && above_taken
                && starts_listed
                && sweep_unmoved
        },
    );
}

// =============================================================================================
// objects.deletion.the-shards-remove-detaches-the-item-and-defers-the-rest
// =============================================================================================

/// Every recorded "this object is gone" the corpus carries, replayed into a world that holds the
/// item it names.
///
/// **The count is not pinned.** A count of recorded messages would be a pinned corpus literal; the
/// recordings are the oracle, so the list is read out of the corpus the index names and the
/// scenario asserts only that it found some.
fn recorded_removals() -> Vec<Vec<u8>> {
    use dereth_client_net::client_session::testing::{Corpus, Direction};

    let mut rows = Vec::new();
    for (id, _slug) in dereth_client_net::client_session::testing::session_index() {
        let Ok(Some(corpus)) = Corpus::load(id) else {
            continue;
        };
        for row in corpus.blobs {
            if row.dir == Direction::ServerToClient
                && row.opcode == u32::from(REMOVE_OBJECT_EVENT)
                && row.payload.len() >= 8
            {
                rows.push(row.payload);
            }
        }
    }
    assert!(
        !rows.is_empty(),
        "the recordings carry no removal at all; the corpus this scenario reads is its oracle"
    );
    rows
}

/// The game event the shard sends to take an object out of a pack.
const REMOVE_OBJECT_EVENT: u16 = 0x24;

/// A weenie in `parent`'s pack, waiting on the shard.
fn item_in(world: &mut World, id: ObjectId, parent: ObjectId) {
    let mut w = Weenie::new(id);
    w.valid = true;
    w.waiting = true;
    w.pwd.container_id = Some(parent);
    w.pwd.name = "a removal station".to_owned();
    world.tables.weenies.insert(id, w);
}

/// The item leaves the pack at once and the object itself is only scheduled to go.
pub fn a_removal_detaches_the_item_and_defers_the_deletion() {
    use dereth_client::interaction::{self, Interaction};
    use dereth_client_model::inventory::requests::InventoryRequest;
    use dereth_client_model::objects::ObjectInventory;

    let blobs = recorded_removals();
    let mut every_one = true;
    for raw in &blobs {
        let id = ObjectId(u32::from_le_bytes(
            raw[4..8].try_into().expect("four bytes"),
        ));
        let parent = ObjectId(0x5000_0017);
        let mut w = World::new();
        item_in(&mut w, id, parent);
        w.tables.inventories.insert(
            parent,
            ObjectInventory {
                container: parent,
                items: vec![id],
                ..ObjectInventory::default()
            },
        );
        w.selected = Some(id);
        w.request_lock
            .record(id, InventoryRequest::Move, ServerTime(300.0));
        let mut inter = Interaction::new();
        inter.last_use_time = dereth_primitives::LocalTime(321.0);
        interaction::apply_events(
            &mut inter,
            &[SessionEvent::UiEvent {
                opcode: Opcode(u32::from(REMOVE_OBJECT_EVENT)),
                blob: raw.clone(),
            }],
            &mut w,
        );
        every_one &= w.inventory(parent).is_some_and(|i| i.items.is_empty())
            && w.weenie(id)
                .is_some_and(|it| !it.waiting && !it.marked_for_deletion)
            && w.request_lock.is_idle()
            && w.selected.is_none()
            && w.tables.doomed.get(id) == Some(&ServerTime(346.0));
    }

    // An object the client never heard of: nothing is scheduled, and another item's pending
    // request is left exactly where it was.
    let raw = blobs[0].clone();
    let id = ObjectId(u32::from_le_bytes(
        raw[4..8].try_into().expect("four bytes"),
    ));
    let mut w = World::new();
    let other = ObjectId(17);
    w.request_lock
        .record(other, InventoryRequest::Move, ServerTime(0.0));
    let mut inter = Interaction::new();
    interaction::apply_events(
        &mut inter,
        &[SessionEvent::UiEvent {
            opcode: Opcode(u32::from(REMOVE_OBJECT_EVENT)),
            blob: raw,
        }],
        &mut w,
    );
    let unknown_schedules_nothing = w.tables.doomed.is_empty() && !w.request_lock.is_idle();
    let but_the_panel_is_told = matches!(
        inter.take_external_container_notices().as_slice(),
        [dereth_ui_screens::panels::external_container::ExternalContainerNotice::ItemMoved {
            object,
            container,
        }] if *object == id && container.0 == 0
    );

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "objects.deletion.the-shards-remove-detaches-the-item-and-defers-the-rest",
        move |_| every_one && unknown_schedules_nothing && but_the_panel_is_told,
    );
}

// =============================================================================================
// objects.maintenance.every-deadline-is-strict-and-a-child-that-left-is-spared
// =============================================================================================

/// Nothing goes at its deadline, nothing goes while the housekeeping is off, and a container that
/// goes spares the children that left it.
pub fn the_housekeeping_deadlines_are_strict() {
    use dereth_client_model::objects::{
        DoomEntry, NullPlaceholder, ObjectInventory, PhysicsPresence,
    };

    let mut out = RecordingSink::default();
    let mut req = RecordingRequests::default();

    // 1. The deadline itself is strict: equality is not expiry.
    let mut w = World::new();
    let id = ObjectId(44);
    item_in(&mut w, id, ObjectId(0));
    w.schedule_destroy(id, ServerTime(100.0));
    w.use_time(ServerTime(125.0), &mut out, &mut req);
    let equality_waits = w.weenie(id).is_some();
    w.use_time(ServerTime(125.000_1), &mut out, &mut req);
    let past_it_goes = w.weenie(id).is_none();

    // …and a heap entry whose deadline is only an epsilon away is stale rather than due.
    item_in(&mut w, id, ObjectId(0));
    w.tables
        .doom_queue
        .push(std::cmp::Reverse(DoomEntry { when: 0.0, id }));
    w.tables.doomed.insert(
        id,
        ServerTime(dereth_client_model::objects::DESTRUCTION_RECHECK_EPSILON),
    );
    w.use_time(ServerTime(1.0), &mut out, &mut req);
    let epsilon_is_stale = w.weenie(id).is_some();

    // 2. A placeholder's age is checked on every call, and its own equality waits too.
    let mut w = World::new();
    let ghost = ObjectId(9);
    let mut asks = RecordingRequests::default();
    w.tables.null_physics.insert(
        ghost,
        NullPlaceholder {
            update_time: ServerTime(1.0),
            ..NullPlaceholder::default()
        },
    );
    w.use_time(ServerTime(20.5), &mut out, &mut asks);
    let silent_before = asks.0.is_empty();
    w.use_time(ServerTime(21.0), &mut out, &mut asks);
    let age_equality_waits = asks.0.is_empty();
    w.use_time(ServerTime(21.1), &mut out, &mut asks);
    let then_it_asks = matches!(asks.0.as_slice(), [dereth_client_model::Request::ForceObjdesc(m)] if m.id == ghost);

    // 3. The visible list is refreshed before the deletion, and waits strictly more than a second.
    let mut w = World::new();
    let seen = ObjectId(90);
    item_in(&mut w, seen, ObjectId(0));
    w.tables.physics.insert(
        seen,
        PhysicsPresence {
            cell: Some(dereth_primitives::CellId(0xA9B4_0001)),
            ..PhysicsPresence::default()
        },
    );
    w.schedule_destroy(seen, ServerTime(0.0));
    w.use_time(ServerTime(26.0), &mut out, &mut req);
    let gone_but_listed =
        w.weenie(seen).is_none() && w.physics(seen).is_none() && w.tables.visible.contains(&seen);
    w.use_time(ServerTime(27.0), &mut out, &mut req);
    let a_second_is_not_enough = w.tables.visible.contains(&seen);
    w.use_time(ServerTime(27.001), &mut out, &mut req);
    let then_the_list_follows = !w.tables.visible.contains(&seen);

    // 4. Nothing happens at all while the housekeeping is switched off, and each placeholder's
    //    own stamp is its own even when both placeholder tables name one id.
    let mut w = World::new();
    let held = ObjectId(77);
    item_in(&mut w, held, ObjectId(0));
    w.tables
        .null_physics
        .insert(ObjectId(88), NullPlaceholder::default());
    w.tables
        .null_weenies
        .insert(ObjectId(88), NullPlaceholder::default());
    w.schedule_destroy(held, ServerTime(0.0));
    w.maintenance_active = false;
    let mut asks = RecordingRequests::default();
    w.use_time(ServerTime(100.0), &mut out, &mut asks);
    let switched_off = w.weenie(held).is_some() && asks.0.is_empty();
    w.maintenance_active = true;
    w.use_time(ServerTime(100.0), &mut out, &mut asks);
    let switched_on = w.weenie(held).is_none() && asks.0.len() == 2;

    // 5. A container that goes takes the contents it still holds -- through however many nested
    //    packs -- and the callback that removes it can still read the pack it was in.
    let mut w = World::new();
    let (owner, bag, inner, leaf) = (ObjectId(1), ObjectId(2), ObjectId(3), ObjectId(4));
    for (id, parent) in [
        (owner, ObjectId(0)),
        (bag, owner),
        (inner, bag),
        (leaf, inner),
    ] {
        item_in(&mut w, id, parent);
    }
    for (holder, contents) in [(owner, vec![bag]), (bag, vec![inner])] {
        w.tables.inventories.insert(
            holder,
            ObjectInventory {
                container: holder,
                containers: contents,
                ..ObjectInventory::default()
            },
        );
    }
    w.tables.inventories.insert(
        inner,
        ObjectInventory {
            container: inner,
            items: vec![leaf],
            ..ObjectInventory::default()
        },
    );
    for id in [inner, leaf] {
        w.schedule_destroy(id, ServerTime(300.0));
    }
    w.remove_contents_from_destruction_queue(bag);
    let nested_recursed = w.tables.doomed.is_empty();
    w.schedule_destroy(bag, ServerTime(300.0));
    let mut sink = RecordingSink::default();
    w.use_time(
        ServerTime(350.0),
        &mut sink,
        &mut RecordingRequests::default(),
    );
    let read_the_old_pack = w.inventory(owner).is_some_and(|i| i.containers.is_empty());
    let child_took_the_real_clock =
        w.tables.doomed.get(inner) == Some(&ServerTime(375.0)) && w.weenie(inner).is_some();

    // …and it spares a child that has moved elsewhere or that is out in the world itself.
    let mut w = World::new();
    for (id, parent) in [(8, 0), (9, 8), (10, 99), (11, 8)] {
        item_in(&mut w, ObjectId(id), ObjectId(parent));
    }
    w.tables.inventories.insert(
        ObjectId(8),
        ObjectInventory {
            container: ObjectId(8),
            items: vec![ObjectId(9), ObjectId(10), ObjectId(11)],
            ..ObjectInventory::default()
        },
    );
    w.tables.physics.insert(
        ObjectId(11),
        PhysicsPresence {
            cell: Some(dereth_primitives::CellId(1)),
            ..PhysicsPresence::default()
        },
    );
    let mut sink = RecordingSink::default();
    w.delete_object(ObjectId(8), ServerTime(1000.0), &mut sink);
    let still_held_goes = w.tables.doomed.get(ObjectId(9)) == Some(&ServerTime(1025.0));
    let moved_child_spared = w.tables.doomed.get(ObjectId(10)).is_none();
    let world_child_spared = w.tables.doomed.get(ObjectId(11)).is_none();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "objects.maintenance.every-deadline-is-strict-and-a-child-that-left-is-spared",
        move |_| {
            equality_waits
                && past_it_goes
                && epsilon_is_stale
                && silent_before
                && age_equality_waits
                && then_it_asks
                && gone_but_listed
                && a_second_is_not_enough
                && then_the_list_follows
                && switched_off
                && switched_on
                && nested_recursed
                && read_the_old_pack
                && child_took_the_real_clock
                && still_held_goes
                && moved_child_spared
                && world_child_spared
        },
    );
}

// =============================================================================================
// objects.replacement.a-newer-instance-wins-and-the-old-ones-traffic-is-dropped
// =============================================================================================

/// A create carrying only an instance number, which is what decides whether it replaces.
fn instance_descriptor(id: ObjectId, instance: u16) -> ObjectCreatePayload {
    let mut p = ObjectCreatePayload {
        id,
        objdesc: ObjDesc::default(),
        physicsdesc: PhysicsDesc::default(),
        wdesc: dereth_protocol::types::PublicWeenieDesc::default(),
    };
    p.physicsdesc.timestamps.instance = instance;
    p
}

/// Only a newer instance replaces, and everything the replaced one had pending goes with it.
pub fn a_newer_instance_replaces_and_the_old_ones_traffic_is_dropped() {
    use dereth_client::objects::ObjectStream;
    use dereth_client_net::client_session::testing::MockTransport;
    use dereth_primitives::{LocalTime, NetQueue};
    use dereth_protocol::events::pack_event;
    use dereth_protocol::objects::{
        ItemDeleteObject, ItemServerSaysRemove, ItemSetState, ItemUpdateObject,
    };

    // 1. Equal merges, older is dropped, newer replaces -- and a forced update replaces with an
    //    instance older than the one standing.
    let id = ObjectId(900);
    let mut stream = ObjectStream::new();
    let mut ladder = true;
    for (instance, forced, expect_delete) in [
        (10, false, false),
        (10, false, false),
        (9, false, false),
        (11, false, true),
        (1, true, true),
    ] {
        let p = instance_descriptor(id, instance);
        let (opcode, body) = if forced {
            (
                Opcode::ITEM_UPDATE_OBJECT,
                dereth_protocol::write_body(&ItemUpdateObject(p)).expect("encode"),
            )
        } else {
            (
                Opcode::ITEM_CREATE_OBJECT,
                dereth_protocol::write_body(&ItemCreateObject(p)).expect("encode"),
            )
        };
        feed_body(&mut stream, opcode, body);
        ladder &= !stream.take_removed().is_empty() == expect_delete;
        let want = if instance == 9 { 10 } else { instance };
        ladder &= stream.presence(id).is_some_and(|p| p.instance == want);
    }

    // 2. When the object goes, only its own ordering windows and stamps go with it.
    let (one, other) = (ObjectId(920), ObjectId(921));
    let mut session = dereth_client_net::client_session::Session::new(MockTransport::new());
    session.object_arrived(one, 1);
    session.object_arrived(other, 1);
    let seeded = session.stamper(one).update(7, 20)
        && session.stamper(other).update(7, 20)
        && session.stamper(one).update_house(20);
    for owner in [one, other, ObjectId(0)] {
        for stamp in [1, 3] {
            session.transport.deliver_blob(
                NetQueue::UiQueue,
                &pack_event(owner, stamp, &ItemServerSaysRemove { object: owner }).expect("encode"),
            );
        }
    }
    session.transport.deliver_blob(
        NetQueue::WorldObjects,
        &dereth_protocol::write_blob(&ItemSetState {
            id: one,
            state: 0x0000_4000,
            timestamps: dereth_protocol::types::PhysicsEventStamp {
                instance: 2,
                event: 10,
            },
        })
        .expect("encode"),
    );
    session.tick(LocalTime(1.0));
    let first: Vec<_> = session.drain_events().collect();
    let three_passed = first
        .iter()
        .filter(|e| matches!(e, SessionEvent::UiEvent { .. }))
        .count()
        == 3;
    session.object_deleted(one);
    let only_its_own = !session.instances_mut().knows(one)
        && session.instances_mut().knows(other)
        && session.stamper(one).get(7).is_none()
        && session.stamper(one).house().is_none()
        && session.stamper(other).get(7) == Some(20);
    session.object_arrived(one, 2);
    let nothing_replays = session.drain_events().next().is_none();
    for (owner, stamp) in [(one, 1), (other, 2), (ObjectId(0), 2)] {
        session.transport.deliver_blob(
            NetQueue::UiQueue,
            &pack_event(owner, stamp, &ItemServerSaysRemove { object: owner }).expect("encode"),
        );
    }
    session.tick(LocalTime(2.0));
    let after: Vec<_> = session.drain_events().collect();
    let the_gap_is_kept = after
        .iter()
        .filter(|e| matches!(e, SessionEvent::UiEvent { .. }))
        .count()
        == 5;
    session.transport.deliver_blob(
        NetQueue::WorldObjects,
        &dereth_protocol::write_blob(&ItemCreateObject(instance_descriptor(one, 2)))
            .expect("encode"),
    );
    session.tick(LocalTime(3.0));
    let mut replacement = ObjectStream::new();
    replacement.pump_session(&mut session, LocalTime(3.0));
    let fresh = replacement.presence(one).is_some_and(|p| p.instance == 2)
        && session.stamper(one).update(7, 1);

    // 3. A change for the instance that left, arriving in the same batch as the replacement,
    //    cannot write the replacement.
    let late = ObjectId(930);
    let mut stream = ObjectStream::new();
    let mut session = dereth_client_net::client_session::Session::new(MockTransport::new());
    session.transport.deliver_blob(
        NetQueue::WorldObjects,
        &dereth_protocol::write_blob(&ItemCreateObject(instance_descriptor(late, 1)))
            .expect("encode"),
    );
    session.tick(LocalTime(1.0));
    stream.pump_session(&mut session, LocalTime(1.0));
    for blob in [
        dereth_protocol::write_blob(&ItemDeleteObject {
            id: late,
            instance_sequence: 1,
        })
        .expect("encode"),
        dereth_protocol::write_blob(&ItemCreateObject(instance_descriptor(late, 2)))
            .expect("encode"),
        dereth_protocol::write_blob(&ItemSetState {
            id: late,
            state: 0x0000_4000,
            timestamps: dereth_protocol::types::PhysicsEventStamp {
                instance: 1,
                event: 30,
            },
        })
        .expect("encode"),
    ] {
        session
            .transport
            .deliver_blob(NetQueue::WorldObjects, &blob);
    }
    session.tick(LocalTime(2.0));
    stream.pump_session(&mut session, LocalTime(2.0));
    let replacement_is_clean = stream.presence(late).is_some_and(|p| p.instance == 2)
        && stream.physics_state(late) == Some(0);

    // 4. A message about an instance that has not arrived yet is parked once, and comes back out
    //    with its bytes unchanged when it does.
    let parked = ObjectId(931);
    let mut session = dereth_client_net::client_session::Session::new(MockTransport::new());
    session.object_arrived(parked, 2);
    let state = ItemSetState {
        id: parked,
        state: 0x0000_4000,
        timestamps: dereth_protocol::types::PhysicsEventStamp {
            instance: 2,
            event: 7,
        },
    };
    let blob = dereth_protocol::write_blob(&state).expect("encode");
    session
        .transport
        .deliver_blob(NetQueue::WorldObjects, &blob);
    session.tick(LocalTime(1.0));
    let accepted: Vec<_> = session.drain_events().collect();
    let admitted = matches!(accepted.as_slice(), [SessionEvent::WorldObject { .. }]);
    session.object_deleted(parked);
    session.object_arrived(parked, 1);
    let SessionEvent::WorldObject { opcode, body } = &accepted[0] else {
        panic!("the accepted event is the smart-box one")
    };
    let parks_again = session
        .recheck_world_object_delivery(*opcode, body)
        .is_none();
    session.object_arrived(parked, 2);
    let released: Vec<_> = session.drain_events().collect();
    let came_back_whole = matches!(
        released.as_slice(),
        [SessionEvent::WorldObject { opcode: o, body: b }]
            if *o == Opcode::ITEM_SET_STATE && b == &blob[4..]
    ) && session
        .recheck_world_object_delivery(*opcode, body)
        .is_some();
    session.object_arrived(parked, 2);
    let only_once = session.drain_events().next().is_none();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "objects.replacement.a-newer-instance-wins-and-the-old-ones-traffic-is-dropped",
        move |_| {
            ladder
                && seeded
                && three_passed
                && only_its_own
                && nothing_replays
                && the_gap_is_kept
                && fresh
                && replacement_is_clean
                && admitted
                && parks_again
                && came_back_whole
                && only_once
        },
    );
}

// =============================================================================================
// The ordered UI stream
// =============================================================================================
//
// Four scenarios share one replay of the locked corpus. The recordings are the oracle and the
// replay is the application's own three lines -- the endpoint, the object stream's pump and the
// interaction layer -- so nothing below constructs a message.
//
// **No count in this section is pinned.** The corpus's own totals of ordered blobs, combat
// notifications and health fractions would be pinned corpus literals; every denominator here is
// read off the recordings at run time and the recordings themselves are the list
// `dereth_client_net::client_session::testing::session_index()` names.

/// The ordering header the shard wraps a UI-queue blob in: a magic dword, the object it is about
/// and the stamp it sits at.
const ORDER_HEADER: usize = 12;

/// One ordered UI blob exactly as the shard put it on the wire.
#[derive(Clone)]
struct OrderedBlob {
    iid: ObjectId,
    stamp: u32,
    /// With the ordering header, which is what the client's queue is handed.
    whole: Vec<u8>,
    /// Without it, which is what the dispatcher decodes and what a delivered event carries.
    body: Vec<u8>,
}

impl OrderedBlob {
    fn opcode(&self) -> Option<u32> {
        let b = self.body.get(..4)?;
        Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    /// The blob as the transport would hand it over: ephemeral, ordering type zero, which is what
    /// every recorded shard stamps on everything it sends.
    fn message(&self) -> dereth_primitives::IncomingMessage {
        dereth_primitives::IncomingMessage {
            opcode: dereth_protocol::OrderedEventHeader::MAGIC,
            queue: dereth_primitives::NetQueue::UiQueue,
            sender: dereth_primitives::RecipientId(0),
            blob_id: dereth_primitives::NetBlobId(0x8000_0000_u64 << 32),
            body: self.whole[4..].to_vec(),
        }
    }
}

/// Whether a recording carries the message that ends a character's session, in either direction.
fn carries_a_log_off(
    records: &[dereth_client_net::client_session::testing::capture::Datagram],
) -> bool {
    /// The opcode the client sends to leave the world.
    const EXECUTE_LOG_OFF: u32 = 0xF653;
    records.iter().any(|r| {
        dereth_transport::wire::ParsedPacket::parse(&r.raw).is_ok_and(|p| {
            p.fragments.iter().any(|f| {
                f.payload.get(..4).is_some_and(|b| {
                    u32::from_le_bytes([b[0], b[1], b[2], b[3]]) == EXECUTE_LOG_OFF
                })
            })
        })
    })
}

/// The recording and the endpoint it replays into, both `dereth_testkit::replay`'s.
use dereth_testkit::replay::{recorded_endpoint as endpoint, records as recording};

/// Every ordered UI blob the **shard** sent in one recording, in wire order, with a stamp that
/// repeats on one object dropped the way the client's own window drops it as stale.
///
/// The transport is polled directly rather than through the session, so this is the shard's
/// stream and not the client's opinion of it -- which is what makes the comparison below a
/// measurement.
fn shard_ordered_stream(
    records: &[dereth_client_net::client_session::testing::capture::Datagram],
) -> Vec<OrderedBlob> {
    let mut raw = endpoint(records);
    for r in records.iter().filter(|r| !r.c2s) {
        raw.feed(&r.raw, r.peer(), dereth_primitives::LocalTime(r.t));
    }
    let mut out = Vec::new();
    let mut seen: std::collections::BTreeSet<(u32, u32)> = std::collections::BTreeSet::new();
    while let Some(m) = dereth_primitives::Transport::poll(&mut raw.session.transport) {
        if m.opcode != dereth_protocol::OrderedEventHeader::MAGIC || m.body.len() < ORDER_HEADER - 4
        {
            continue;
        }
        let iid = u32::from_le_bytes([m.body[0], m.body[1], m.body[2], m.body[3]]);
        let stamp = u32::from_le_bytes([m.body[4], m.body[5], m.body[6], m.body[7]]);
        if !seen.insert((iid, stamp)) {
            continue;
        }
        let mut whole = dereth_protocol::OrderedEventHeader::MAGIC
            .to_le_bytes()
            .to_vec();
        whole.extend_from_slice(&m.body);
        out.push(OrderedBlob {
            iid: ObjectId(iid),
            stamp,
            body: m.body[ORDER_HEADER - 4..].to_vec(),
            whole,
        });
    }
    out
}

/// One object's ordered stream, sorted by stamp, or nothing when the recording does not carry its
/// beginning.
///
/// **A stream that starts above its first stamp is the shard's doing and is excluded, measured
/// rather than named.** The client's window holds an entry whose predecessors never arrived, by
/// construction; the recordings carry a few such streams, each one a handful of blobs the shard
/// sent *after* it had acknowledged that character's log-off. Listing them by recording and stamp
/// would be a pinned corpus literal; the rule here is the property
/// -- does this stream begin at its beginning -- and the count of what it excluded is asserted to
/// be a small fraction of the whole rather than pinned.
fn stream_of(all: &[OrderedBlob], iid: ObjectId) -> Vec<OrderedBlob> {
    let mut v: Vec<OrderedBlob> = all.iter().filter(|o| o.iid == iid).cloned().collect();
    v.sort_by_key(|o| o.stamp);
    if v.first().is_none_or(|o| o.stamp > 1) {
        return Vec::new();
    }
    for w in v.windows(2) {
        assert_eq!(
            w[1].stamp,
            w[0].stamp + 1,
            "a gap between stamps {} and {} on one object: the recording itself is not contiguous",
            w[0].stamp,
            w[1].stamp
        );
    }
    v
}

/// The opcode the session consumes itself instead of handing on, so it never reaches a receiver.
const PLAYER_DESCRIPTION: u32 = 0x0013;

/// What one recording produced when driven through the endpoint, the object stream's pump and the
/// interaction layer -- the application's own three lines.
struct Replayed {
    session: &'static str,
    /// Per opcode, the ordered bodies the **shard** sent, in wire order, from streams the
    /// recording carries whole.
    sent: std::collections::BTreeMap<u32, Vec<Vec<u8>>>,
    /// How many ordered blobs were left out because their stream does not begin at its beginning.
    excluded: usize,
    /// Per opcode, the bodies that reached the interaction layer, in delivery order.
    bodies: std::collections::BTreeMap<u32, Vec<Vec<u8>>>,
    stats: dereth_client::interaction::InteractionStats,
    hud_stats: dereth_client::hud::HudStats,
    chat: Vec<dereth_ui_screens::chat::interface::ChatMessage>,
    /// Per container reply, in arrival order: what the shard named and what the grid held on the
    /// frame it landed.
    filled_on_arrival: Vec<(Vec<ObjectId>, std::collections::BTreeSet<ObjectId>)>,
    /// Per close, whether the world still held a list for the container immediately afterwards.
    closed_on_arrival: Vec<bool>,
    /// Per appraisal, whether the assess cache held a profile immediately afterwards.
    appraised_on_arrival: Vec<bool>,
    /// The allegiance tree's size on the frame each update landed.
    allegiance_on_arrival: Vec<usize>,
    /// How many enchantments were in effect on the frame each update landed.
    enchantments_on_arrival: Vec<Option<usize>>,
    presences_peak: usize,
    presences_final: usize,
    /// Whether the recording carries a log-off at all. Not every one does: a player can leave the
    /// world without going back to character select, and the recording then stops with the
    /// character still in it.
    logged_off: bool,
}

impl Replayed {
    fn delivered(&self, opcode: u32) -> usize {
        self.bodies.get(&opcode).map_or(0, Vec::len)
    }
}

#[allow(clippy::too_many_lines)]
fn replay_one(session: &'static str) -> Replayed {
    use dereth_client::hud::Hud;
    use dereth_client::interaction::{self, Interaction};
    use dereth_client::objects::ObjectStream;
    use dereth_protocol::Message as _;
    use dereth_ui_screens::view::GameView;

    let records = recording(session);
    let all = shard_ordered_stream(&records);
    let ids: std::collections::BTreeSet<ObjectId> = all.iter().map(|o| o.iid).collect();
    let kept: std::collections::BTreeSet<(u32, u32)> = ids
        .iter()
        .flat_map(|id| stream_of(&all, *id))
        .map(|o| (o.iid.0, o.stamp))
        .collect();
    let mut sent: std::collections::BTreeMap<u32, Vec<Vec<u8>>> = std::collections::BTreeMap::new();
    let mut excluded = 0usize;
    for o in &all {
        if !kept.contains(&(o.iid.0, o.stamp)) {
            excluded += 1;
            continue;
        }
        let Some(op) = o.opcode() else { continue };
        if op == PLAYER_DESCRIPTION {
            continue;
        }
        sent.entry(op).or_default().push(o.body.clone());
    }

    let mut net = endpoint(&records);
    let mut objects = ObjectStream::new();
    let mut inter = Interaction::new();
    let mut hud = Hud::new();
    let mut entered = false;
    let mut out = Replayed {
        session,
        sent,
        excluded,
        bodies: std::collections::BTreeMap::new(),
        stats: dereth_client::interaction::InteractionStats::default(),
        hud_stats: dereth_client::hud::HudStats::default(),
        chat: Vec::new(),
        filled_on_arrival: Vec::new(),
        closed_on_arrival: Vec::new(),
        appraised_on_arrival: Vec::new(),
        allegiance_on_arrival: Vec::new(),
        enchantments_on_arrival: Vec::new(),
        presences_peak: 0,
        presences_final: 0,
        logged_off: carries_a_log_off(&records),
    };
    for r in records.iter().filter(|r| !r.c2s) {
        let now = dereth_primitives::LocalTime(r.t);
        net.feed(&r.raw, r.peer(), now);
        net.tick(now);
        let _ = net.take_outgoing();
        let events = objects.pump(&mut net, now);
        let mut opened: Vec<(ObjectId, Vec<ObjectId>)> = Vec::new();
        let mut closed: Vec<ObjectId> = Vec::new();
        let mut appraised: Vec<ObjectId> = Vec::new();
        let mut allegiance_updates = 0usize;
        let mut enchantment_updates = 0usize;
        for e in &events {
            match e {
                SessionEvent::CharacterSet(set) if !entered => {
                    if let Some(ch) = set.characters.first() {
                        let account = set.account.clone();
                        net.enter_world(ch.gid, &account);
                        entered = true;
                    }
                }
                SessionEvent::UiEvent { opcode, blob } => {
                    out.bodies.entry(opcode.0).or_default().push(blob.clone());
                    let (_, body) = e.ui_body().expect("UI event");
                    let mut rd = dereth_protocol::archive::Reader::new(body);
                    match *opcode {
                        Opcode::ITEM_ON_VIEW_CONTENTS => {
                            if let Ok(m) =
                                dereth_protocol::objects::ItemOnViewContents::read(&mut rd)
                            {
                                opened.push((
                                    m.container,
                                    m.contents.iter().map(|c| c.iid).collect(),
                                ));
                            }
                        }
                        Opcode::ITEM_STOP_VIEWING_OBJECT_CONTENTS => {
                            if let Ok(m) =
                                dereth_protocol::objects::ItemStopViewingObjectContents::read(
                                    &mut rd,
                                )
                            {
                                closed.push(m.object);
                            }
                        }
                        Opcode::ITEM_SET_APPRAISE_INFO => {
                            if let Ok(m) =
                                dereth_protocol::objects::ItemSetAppraiseInfo::read(&mut rd)
                            {
                                appraised.push(m.object);
                            }
                        }
                        _ => {
                            if opcode.0 == 0x0020 {
                                allegiance_updates += 1;
                            } else if opcode.0 == 0x02C2 {
                                enchantment_updates += 1;
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        out.chat
            .extend(hud.apply_events(&events, &mut objects.world));
        interaction::apply_events(&mut inter, &events, &mut objects.world);
        // **What the panel would draw on the frame the reply landed**, not at the end of the
        // recording: a container can be emptied, closed or destroyed later, and every recording
        // ends with its character leaving.
        {
            let view = hud.view(&objects);
            for (container, named) in &opened {
                let held: std::collections::BTreeSet<ObjectId> = view
                    .container_contents(*container)
                    .iter()
                    .chain(view.contained_containers(*container).iter())
                    .copied()
                    .collect();
                out.filled_on_arrival.push((named.clone(), held));
            }
        }
        for c in &closed {
            out.closed_on_arrival
                .push(objects.world.inventory(*c).is_some());
        }
        for id in &appraised {
            out.appraised_on_arrival
                .push(objects.world.appraisal.get(*id).is_some());
        }
        for _ in 0..allegiance_updates {
            out.allegiance_on_arrival
                .push(objects.world.allegiance.data.len());
        }
        for _ in 0..enchantment_updates {
            out.enchantments_on_arrival.push(
                objects
                    .world
                    .player_qualities()
                    .map(|q| q.enchantments.enchantments_in_effect().len()),
            );
        }
        out.presences_peak = out.presences_peak.max(objects.len());
    }
    out.presences_final = objects.len();
    out.stats = inter.stats;
    out.hud_stats = hud.stats;
    out
}

/// Every recording the locked corpus holds, replayed once for the whole binary.
///
/// The list is [`dereth_client_net::client_session::testing::session_index`]'s, so promoting a recording widens these
/// scenarios without an edit here -- and a count in any of them is the sum of what this replay
/// found rather than a number written down.
fn corpus() -> &'static [Replayed] {
    static CORPUS: std::sync::OnceLock<Vec<Replayed>> = std::sync::OnceLock::new();
    CORPUS.get_or_init(|| {
        let out: Vec<Replayed> = dereth_client_net::client_session::testing::session_index()
            .iter()
            .map(|(id, _slug)| replay_one(id))
            .collect();
        assert!(!out.is_empty(), "the corpus index names no recording");
        out
    })
}

// =============================================================================================
// objects.ordered-replies.wait-for-an-object-that-has-not-arrived-yet
// =============================================================================================

/// The ordering window itself, driven with a recording's own blobs at every seam position.
pub fn an_ordered_reply_waits_for_the_object_it_is_about() {
    use dereth_client_net::client_session::UiOrdering;

    /// The blob as it was parked keeps its ordering header; a routed one has already lost it.
    fn released(
        u: &mut UiOrdering,
        id: ObjectId,
        now: dereth_primitives::LocalTime,
    ) -> Vec<Vec<u8>> {
        u.object_arrived(id, now)
            .into_iter()
            .map(|b| b[ORDER_HEADER..].to_vec())
            .collect()
    }

    let records = recording("long-solo-play");
    let all = shard_ordered_stream(&records);
    let ids: std::collections::BTreeSet<ObjectId> = all.iter().map(|o| o.iid).collect();
    let mut every_seam = true;
    let mut total = 0usize;
    for iid in ids {
        let stream = stream_of(&all, iid);
        if stream.is_empty() {
            continue;
        }
        let n = stream.len();
        let expected: Vec<&[u8]> = stream.iter().map(|o| o.body.as_slice()).collect();
        // Every seam for a short stream; a stride plus both ends for a long one.
        let mut flips: std::collections::BTreeSet<usize> = (0..=n).collect();
        if n > 96 {
            flips = (0..=n).step_by(n / 64).collect();
            flips.extend([0, 1, 2, 3, n - 2, n - 1, n]);
        }
        for &k in &flips {
            let mut u = UiOrdering::new();
            let mut delivered: Vec<Vec<u8>> = Vec::new();
            for (i, o) in stream.iter().enumerate() {
                if i == k {
                    delivered.extend(released(
                        &mut u,
                        iid,
                        dereth_primitives::LocalTime(i as f64),
                    ));
                }
                let known = i >= k;
                let d = u.route(
                    &o.message(),
                    dereth_primitives::LocalTime(i as f64),
                    &|_| known,
                );
                delivered.extend(d.ready);
                every_seam &= d.events.is_empty();
            }
            if k >= n {
                delivered.extend(released(
                    &mut u,
                    iid,
                    dereth_primitives::LocalTime(n as f64),
                ));
            }
            let got: Vec<&[u8]> = delivered.iter().map(Vec::as_slice).collect();
            every_seam &= got == expected
                && u.blocked_on(iid) == 0
                && u.highest_on(iid) == stream[n - 1].stamp;
        }
        total += n;
    }
    let the_instrument_read_something = total > 0;

    // Parking is the bare insert and nothing more: it delivers nothing, cannot go stale and does
    // not start the clock that would otherwise skip a stamp the shard did send.
    let iid = all[0].iid;
    let stream = stream_of(&all, iid);
    let mut u = UiOrdering::new();
    let mut parking_is_silent = true;
    for o in stream.iter().take(25) {
        let d = u.route(&o.message(), dereth_primitives::LocalTime(0.0), &|_| false);
        parking_is_silent &= d.ready.is_empty() && d.events.is_empty();
    }
    let parked_held = u.blocked_on(iid) == 25 && u.highest_on(iid) == 0;
    // An hour later. A park that had started the deadlock clock would now fabricate a missing
    // entry rather than replay the twenty-five.
    let released_later = u.object_arrived(iid, dereth_primitives::LocalTime(3600.0));
    let all_came_back = released_later.len() == 25
        && u.highest_on(iid) == stream[24].stamp
        && u.blocked_on(iid) == 0
        && released_later
            .iter()
            .zip(stream.iter())
            .all(|(got, want)| got[ORDER_HEADER..] == want.body[..]);

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "objects.ordered-replies.wait-for-an-object-that-has-not-arrived-yet",
        move |_| {
            every_seam
                && the_instrument_read_something
                && parking_is_silent
                && parked_held
                && all_came_back
        },
    );
}

// =============================================================================================
// objects.ordered-replies.arrive-in-the-order-the-shard-stamped-them
// =============================================================================================

/// Every ordered reply the recordings carry reaches the interaction layer, and the replies that
/// move an item between packs arrive in the order the shard stamped them.
///
/// **The order is asserted on the three replies that move an item**, which is where it decides
/// anything: all three funnel into one last-writer-wins step on an item's container, so two
/// replies about one item applied backwards leave the item in the wrong pack and a count-only
/// test would call that a pass. The other kinds are asserted for arrival, which is the guarantee
/// the ordering window itself makes.
pub fn ordered_replies_arrive_in_the_shards_own_order() {
    /// The shard's three answers that move an item: into a container, onto the body, and between
    /// two places.
    const MOVES: [u32; 3] = [0x0022, 0x0023, 0x019A];

    let mut sent_total = 0usize;
    let mut excluded_total = 0usize;
    let mut moves = 0usize;
    let mut moved = 0u64;
    for r in corpus() {
        for (opcode, bodies) in &r.sent {
            sent_total += bodies.len();
            let got = r.bodies.get(opcode).map_or(&[][..], Vec::as_slice);
            assert_eq!(
                got.len(),
                bodies.len(),
                "{}: the shard sent {} ordered replies of one kind and the client's own dispatch \
                 saw {}",
                r.session,
                bodies.len(),
                got.len()
            );
            if MOVES.contains(opcode) {
                moves += bodies.len();
                assert!(
                    got.iter().zip(bodies.iter()).all(|(a, b)| a == b),
                    "{}: a reply that moves an item reached the dispatch out of the order the \
                     shard stamped it",
                    r.session
                );
            }
        }
        excluded_total += r.excluded;
        moved += r.stats.move_items_applied;
    }
    // The streams the shard began after it had acknowledged a log-off are a rounding error on the
    // traffic, measured rather than listed.
    let the_gap_is_negligible = excluded_total * 100 < sent_total;
    // And every one of those replies really reached the step that moves the item.
    let pickups_closed = moved >= u64::try_from(moves).expect("a count fits");

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "objects.ordered-replies.arrive-in-the-order-the-shard-stamped-them",
        move |_| sent_total > 0 && moves > 0 && the_gap_is_negligible && pickups_closed,
    );
}

// =============================================================================================
// objects.ordered-replies.every-recorded-reply-reaches-a-consumer
// =============================================================================================

/// Every kind of ordered reply this client has a receiver for, and how much of the traffic the
/// receivers take between them.
///
/// The list is the subject: it is what the interaction layer's and the heads-up display's
/// wildcard arms used to swallow, so a regression that deletes a receiver has to delete a row
/// here for this to pass.
const HAS_A_RECEIVER: [u32; 25] = [
    0x0020, 0x0022, 0x0023, 0x0029, 0x0052, 0x00A0, 0x00C9, 0x0196, 0x019A, 0x01A7, 0x01AD, 0x01B1,
    0x01B2, 0x01B3, 0x01B4, 0x01B8, 0x01C0, 0x01C7, 0x0264, 0x028A, 0x028B, 0x02BD, 0x02C2, 0x02C7,
    0x02EB,
];

/// Every receiver is exercised by the recordings, and the receivers take most of the traffic.
pub fn every_recorded_reply_reaches_a_consumer() {
    let mut census: std::collections::BTreeMap<u32, usize> = std::collections::BTreeMap::new();
    for r in corpus() {
        for (opcode, bodies) in &r.sent {
            *census.entry(*opcode).or_default() += bodies.len();
        }
    }
    // An arm for a reply the recordings never carry is untested code, and this scenario must not
    // claim it.
    let every_receiver_is_exercised = HAS_A_RECEIVER.iter().all(|op| census.contains_key(op));
    let total: usize = census.values().sum();
    let consumed: usize = census
        .iter()
        .filter(|(op, _)| HAS_A_RECEIVER.contains(op))
        .map(|(_, n)| *n)
        .sum();
    for (op, n) in &census {
        println!(
            "ordered census: {op:#06X} -- {n:>5} blobs -- {}",
            if HAS_A_RECEIVER.contains(op) {
                "has a receiver"
            } else {
                "still dropped"
            }
        );
    }
    println!("{consumed} of {total} ordered replies reach a receiver");
    // **Not a fraction.** What the corpus carries and what this client has a receiver for are two
    // independent facts, and a ratio between them is a number the next promotion moves. The claim
    // is that no receiver is untested and that everything a receiver exists for reaches it --
    // `objects.ordered-replies.arrive-in-the-order-the-shard-stamped-them` is the other half.
    let some_of_it_reaches_one = consumed > 0;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "objects.ordered-replies.every-recorded-reply-reaches-a-consumer",
        move |_| total > 0 && every_receiver_is_exercised && some_of_it_reaches_one,
    );
}

// =============================================================================================
// objects.ordered-replies.the-panels-and-the-scroll-take-the-shards-own-values
// =============================================================================================

/// What the player looks at, filled from the recordings' own replies.
#[allow(clippy::too_many_lines)]
pub fn the_panels_and_the_scroll_take_the_shards_values() {
    use dereth_protocol::Message as _;

    let mut containers = 0usize;
    let mut container_ok = true;
    let mut closes = 0usize;
    let mut close_ok = true;
    let mut appraisals = 0usize;
    let mut appraisal_ok = true;
    let mut allegiance = 0usize;
    let mut allegiance_with_members = 0usize;
    let mut allegiance_ok = true;
    let mut enchantments = 0usize;
    let mut accepted_enchantments = 0u64;
    let mut enchantment_ok = true;
    let mut combat = 0usize;
    let mut combat_ok = true;
    let mut evaded = 0usize;
    let mut damaged = 0usize;
    let mut victim = 0usize;
    let mut victim_ok = true;
    let mut health = 0usize;
    let mut mana = 0usize;
    let mut meter_ok = true;
    let mut uses = 0u64;
    let mut use_ok = true;

    for r in corpus() {
        // The chest: what the shard named is what the grid holds on the frame it landed.
        for (named, held) in &r.filled_on_arrival {
            containers += 1;
            let want: std::collections::BTreeSet<ObjectId> = named.iter().copied().collect();
            container_ok &= *held == want;
        }
        // Closing it takes the list away entirely rather than emptying it.
        closes += r.closed_on_arrival.len();
        close_ok &= r.closed_on_arrival.iter().all(|still| !still);
        // An appraisal reaches the cache the assess pane reads.
        appraisals += r.appraised_on_arrival.len();
        appraisal_ok &= r.appraised_on_arrival.iter().all(|got| *got);
        // The allegiance tree is rebuilt, and the replies really carry members.
        allegiance += r.allegiance_on_arrival.len();
        allegiance_with_members += r.allegiance_on_arrival.iter().filter(|n| **n > 0).count();
        allegiance_ok &=
            r.stats.allegiance_updates == u64::try_from(r.delivered(0x0020)).expect("a count fits");
        // The enchantment registry takes the wire's updates: one snapshot per update, the player
        // has qualities to hold them by the last one, and the registry is holding something
        // unless its own parity check refused every update it was offered.
        let updates = r.delivered(0x02C2);
        enchantments += updates;
        accepted_enchantments += r.stats.enchantments_updated;
        if updates > 0 {
            let live = r.enchantments_on_arrival.last().copied().flatten();
            enchantment_ok &= r.enchantments_on_arrival.len() == updates
                && live.is_some()
                && (live.unwrap_or(0) > 0 || r.stats.enchantments_updated == 0);
        }
        // Every blow struck reaches the scroll, and none of it is squelched in a replay that
        // squelches nobody.
        let blows: usize = [0x01B1, 0x01B2, 0x01B3, 0x01B4]
            .iter()
            .map(|o| r.delivered(*o))
            .sum();
        combat += blows;
        combat_ok &= r.hud_stats.combat_lines + r.hud_stats.combat_lines_squelched
            == u64::try_from(blows).expect("a count fits")
            && r.hud_stats.combat_lines_squelched == 0;
        for m in &r.chat {
            if m.ty == 21 || m.ty == 22 {
                combat_ok &= m.body.ends_with('\n');
                if m.body.contains("evaded") {
                    evaded += 1;
                }
                if m.body.contains("point") {
                    damaged += 1;
                }
            }
        }
        // A victim notification is the shard's own words, in the scroll.
        for blob in r.bodies.get(&0x01AD).map(Vec::as_slice).unwrap_or_default() {
            let mut rd = dereth_protocol::archive::Reader::new(blob.get(4..).unwrap_or_default());
            let m = dereth_protocol::combat::VictimNotificationOther::read(&mut rd)
                .expect("a victim notification is one string and must decode");
            if m.message.is_empty() {
                continue;
            }
            victim += 1;
            victim_ok &= r.chat.iter().any(|c| c.ty == 0 && c.body == m.message);
        }
        // The selected object's meters take the shard's own fractions -- and only for the object
        // that is actually selected, which is the half a one-directional test cannot see.
        for blob in r.bodies.get(&0x01C0).map(Vec::as_slice).unwrap_or_default() {
            let mut rd = dereth_protocol::archive::Reader::new(blob.get(4..).unwrap_or_default());
            let m = dereth_protocol::combat::CombatQueryHealthResponse::read(&mut rd)
                .expect("a health reply is an id and a fraction");
            let mut w = World::new();
            w.set_selected_object(Some(m.object), true, &mut NullSink);
            meter_ok &= w.update_object_health(m.object, m.health)
                && w.selected_meters.health == Some(m.health);
            let mut other = World::new();
            other.set_selected_object(Some(ObjectId(m.object.0 ^ 1)), true, &mut NullSink);
            meter_ok &= !other.update_object_health(m.object, m.health)
                && other.selected_meters.health.is_none();
            health += 1;
        }
        for blob in r.bodies.get(&0x0264).map(Vec::as_slice).unwrap_or_default() {
            let mut rd = dereth_protocol::archive::Reader::new(blob.get(4..).unwrap_or_default());
            let m = dereth_protocol::items::ItemQueryItemManaResponse::read(&mut rd)
                .expect("a mana reply is an id, a fraction and a flag");
            let mut w = World::new();
            w.set_selected_object(Some(m.object), true, &mut NullSink);
            let wrote = w.update_item_mana(m.object, m.mana, m.success != 0);
            if m.success == 0 {
                meter_ok &= !wrote && w.selected_meters.mana.is_none();
            } else {
                meter_ok &= wrote && w.selected_meters.mana == Some(m.mana);
                mana += 1;
            }
            let mut other = World::new();
            other.set_selected_object(Some(ObjectId(m.object.0 ^ 1)), true, &mut NullSink);
            meter_ok &= !other.update_item_mana(m.object, m.mana, m.success != 0);
        }
        // Nothing selects anything in a replay, so no meter may have been written by it.
        meter_ok &= r.stats.selection_meters_written == 0
            && r.stats.health_responses + r.stats.mana_responses
                == u64::try_from(r.delivered(0x01C0) + r.delivered(0x0264)).expect("a count fits");
        // Every use is acknowledged, which is what brings the busy cursor down again.
        uses += r.stats.uses_done;
        use_ok &= r.stats.uses_done == u64::try_from(r.delivered(0x01C7)).expect("a count fits");
    }
    // The busy count itself, driven the way the client drives it: one cast up, one acknowledgement
    // down, and it never goes below nothing.
    let mut w = World::new();
    w.magic.busy_count = 2;
    w.use_done(0);
    let one_off = w.magic.busy_count == 1;
    w.use_done(0);
    w.use_done(0);
    let saturates = w.magic.busy_count == 0;

    for (what, ok) in [
        ("a container fills with what the shard named", container_ok),
        ("closing one takes its list away", close_ok),
        ("an appraisal reaches the cache", appraisal_ok),
        ("an allegiance update rebuilds the tree", allegiance_ok),
        ("an enchantment update reaches the registry", enchantment_ok),
        ("every blow reaches the scroll", combat_ok),
        ("a victim notification is the shard's own words", victim_ok),
        (
            "the meters take the shard's fractions, and only for the selection",
            meter_ok,
        ),
        ("every use is acknowledged", use_ok),
    ] {
        println!("panels: {what}: {ok}");
    }
    println!(
        "panels: {containers} container replies, {closes} closes, {appraisals} appraisals, \
         {allegiance} allegiance updates ({allegiance_with_members} with members), \
         {enchantments} enchantment updates, {combat} blows ({evaded} evasions, {damaged} \
         damage lines), {victim} victim notifications, {health} health and {mana} mana \
         fractions, {uses} uses acknowledged"
    );

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "objects.ordered-replies.the-panels-and-the-scroll-take-the-shards-own-values",
        move |_| {
            containers > 0
                && container_ok
                && closes > 0
                && close_ok
                && appraisals > 0
                && appraisal_ok
                && allegiance > 0
                && allegiance_with_members > 0
                && allegiance_ok
                && enchantments > 0
                && accepted_enchantments > 0
                && enchantment_ok
                && combat > 0
                && combat_ok
                && evaded > 0
                && damaged > 0
                && victim > 0
                && victim_ok
                && health > 0
                && mana > 0
                && meter_ok
                && uses > 0
                && use_ok
                && one_off
                && saturates
        },
    );
}

// =============================================================================================
// session.end.the-world-the-character-left-is-torn-down
// =============================================================================================

/// The log-off is what throws the world away, and a recording without one keeps its objects.
pub fn the_world_is_torn_down_when_the_character_leaves() {
    let mut held_a_world = 0usize;
    let mut after_a_log_off_is_empty = true;
    let mut without_one_it_still_stands = true;
    let mut never_logged_off = 0usize;
    for r in corpus() {
        if r.presences_peak == 0 {
            continue;
        }
        held_a_world += 1;
        println!(
            "teardown: {} held {} objects at its peak and {} at the end; log-off {}",
            r.session, r.presences_peak, r.presences_final, r.logged_off
        );
        if r.logged_off {
            after_a_log_off_is_empty &= r.presences_final == 0;
        } else {
            // **The claim is about the log-off and not about the recording ending.** Only a
            // recording with no log-off in it separates the two, so it is measured rather than
            // named.
            never_logged_off += 1;
            without_one_it_still_stands &= r.presences_final > 0;
        }
    }

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "session.end.the-world-the-character-left-is-torn-down",
        move |_| {
            held_a_world > 0
                && never_logged_off < held_a_world
                && after_a_log_off_is_empty
                && without_one_it_still_stands
        },
    );
}

// -------------------------------------------------------------------------------------------
// selection.attackable.the-clients-own-test-is-more-than-is-it-a-creature
// -------------------------------------------------------------------------------------------

/// Whether the client will let the player attack a thing is more than "is it a creature".
///
/// It opens no data file, so it belongs in this tier. The masks are written out as numbers rather
/// than read back through the same symbols the client reads them through, so a wrong mask is
/// visible here.
pub fn what_the_client_will_let_you_attack() {
    use dereth_client_model::weenie::{bitfield, item_type};

    // The bits, as numbers. Reading them through the client's own names would not notice a
    // wrong one.
    let masks = bitfield::PLAYER == 0x0000_0008
        && bitfield::ATTACKABLE == 0x0000_0010
        && bitfield::PLAYER_KILLER == 0x0000_0020
        && bitfield::PK_LITE == 0x0200_0000
        && bitfield::IMPENETRABLE == 0x0020_0000
        && item_type::CREATURE == 0x0000_0010;

    let mut w = World::new();
    let me = ObjectId(1);
    w.set_player(me);
    let put = |w: &mut World, id: u32, obj_type: u32, bits: u32, pet: Option<ObjectId>| {
        let mut o = Weenie::new(ObjectId(id));
        o.pwd = dereth_protocol::types::PublicWeenieDesc {
            name: format!("subject {id}"),
            obj_type,
            bitfield: bits,
            pet_owner: pet,
            ..dereth_protocol::types::PublicWeenieDesc::default()
        };
        w.tables.weenies.insert(ObjectId(id), o);
    };
    put(&mut w, 1, item_type::CREATURE, bitfield::PLAYER, None); // the player himself
    put(&mut w, 2, item_type::CREATURE, bitfield::ATTACKABLE, None); // an ordinary monster
    put(&mut w, 3, item_type::CREATURE, 0, None); // a creature nobody may attack
    put(&mut w, 4, item_type::MISC, bitfield::ATTACKABLE, None); // not a creature at all
    put(
        &mut w,
        5,
        item_type::CREATURE,
        bitfield::ATTACKABLE,
        Some(ObjectId(9)),
    ); // a pet
    put(&mut w, 6, item_type::CREATURE, bitfield::PLAYER, None); // another player, at peace
    put(
        &mut w,
        7,
        item_type::CREATURE,
        bitfield::PLAYER | bitfield::PLAYER_KILLER,
        None,
    );
    put(&mut w, 8, item_type::CREATURE, bitfield::IMPENETRABLE, None);

    let arms = w.object_is_attackable(ObjectId(0))
        // The player himself answers yes, which is the arm that was once the wrong way round.
        && w.object_is_attackable(me)
        && w.object_is_attackable(ObjectId(2))
        && !w.object_is_attackable(ObjectId(3))
        && !w.object_is_attackable(ObjectId(4))
        // A pet is refused before the bit is even read.
        && !w.object_is_attackable(ObjectId(5))
        && !w.object_is_attackable(ObjectId(6))
        // They are marked for player combat and the player is not.
        && !w.object_is_attackable(ObjectId(7))
        && w.object_is_attackable(ObjectId(8))
        && !w.object_is_attackable(ObjectId(99));

    // Both marked for the same kind of player combat: yes. The two kinds are separate pairs and
    // not the same bit.
    w.tables
        .weenies
        .get_mut(me)
        .expect("the player")
        .pwd
        .bitfield |= bitfield::PLAYER_KILLER;
    let both_marked = w.object_is_attackable(ObjectId(7));
    w.tables
        .weenies
        .get_mut(me)
        .expect("the player")
        .pwd
        .bitfield = bitfield::PLAYER | bitfield::PK_LITE;
    put(
        &mut w,
        10,
        item_type::CREATURE,
        bitfield::PLAYER | bitfield::PK_LITE,
        None,
    );
    let lighter = w.object_is_attackable(ObjectId(10)) && !w.object_is_attackable(ObjectId(7));

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "selection.attackable.the-clients-own-test-is-more-than-is-it-a-creature",
        move |_| masks && arms && both_marked && lighter,
    );
}

// -------------------------------------------------------------------------------------------
// 5. objects.force-objdesc.leaves-on-the-control-queue
// -------------------------------------------------------------------------------------------

/// `0xF6EA`, the opcode a re-ask for an object description leaves as.
const FORCE_OBJDESC: u32 = 0xF6EA;

/// An outdoor landcell for an object that needs a position and nothing more.
const OUTDOOR_CELL: u32 = 0xA9B4_0001;

/// A dangling reference is asked about again, once, and the ask really becomes bytes.
pub fn force_objdesc_leaves_on_the_control_queue() {
    const ORPHAN: ObjectId = ObjectId(0x8000_0BAD);

    let mut c = HeadlessClient::model();
    c.world_mut()
        .get_null_weenie_object(ORPHAN, ServerTime(0.0));
    // Twenty-three seconds of simulated time: past the twenty the sweep waits and short of the
    // twenty-five after which the placeholder is destroyed and there is nothing left to ask about.
    c.tick(700);

    c.assert_behaviour("objects.force-objdesc.leaves-on-the-control-queue", |v| {
        let asked: Vec<ObjectId> = v
            .outbound()
            .iter()
            .filter_map(|r| match r {
                Request::ForceObjdesc(m) => Some(m.id),
                _ => None,
            })
            .collect();
        asked == [ORPHAN] && v.outbound_opcodes() == [FORCE_OBJDESC]
    });
}

// -------------------------------------------------------------------------------------------
// 6. object.set-state.visible-list-follows
// -------------------------------------------------------------------------------------------

/// A state message after the create moves the object in and out of the visible list.
pub fn set_state_moves_the_visible_list() {
    /// Bit 0 of the physics state word.
    const STATIC: u32 = 0x0000_0001;
    /// One of the words a recording actually carries: a door, opened.
    const DOOR_OPEN: u32 = 0x0001_001C;
    const ID: ObjectId = ObjectId(0x7000_0010);

    let mut c = HeadlessClient::model();
    let create =
        dereth_protocol::objects::ItemCreateObject(dereth_protocol::objects::ObjectCreatePayload {
            id: ID,
            objdesc: dereth_protocol::types::ObjDesc::default(),
            physicsdesc: dereth_protocol::types::PhysicsDesc {
                bitfield: dereth_protocol::types::physicsdesc::flags::POSITION,
                state: DOOR_OPEN,
                position: Some(dereth_protocol::types::PositionWire {
                    objcell_id: OUTDOOR_CELL,
                    frame: dereth_protocol::types::Frame::default(),
                }),
                ..dereth_protocol::types::PhysicsDesc::default()
            },
            wdesc: PublicWeenieDesc::default(),
        });
    c.when(Inbound::world_view(&create));
    let visible_at_create = {
        c.world_mut().update_visible_object_list();
        c.view().world().tables.visible.contains(&ID)
    };

    let set = |state: u32, event: u16| dereth_protocol::objects::ItemSetState {
        id: ID,
        state,
        timestamps: dereth_protocol::types::PhysicsEventStamp { instance: 0, event },
    };
    c.when(Inbound::world_view(&set(DOOR_OPEN | STATIC, 1)));
    let visible_when_static = {
        c.world_mut().update_visible_object_list();
        c.view().world().tables.visible.contains(&ID)
    };

    c.when(Inbound::world_view(&set(DOOR_OPEN, 2)));
    c.world_mut().update_visible_object_list();

    c.assert_behaviour("object.set-state.visible-list-follows", move |v| {
        visible_at_create
            && !visible_when_static
            && v.world().tables.visible.contains(&ID)
            && v.objects().stats.state_events == 2
            && v.objects().stats.state_events_without_physics == 0
    });
}
