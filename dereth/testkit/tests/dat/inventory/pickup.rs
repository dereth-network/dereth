use dereth_client_contract::UiRequest;
use dereth_client_model::inventory::requests::InventoryRequest;
use dereth_client_model::inventory::use_object::{UseOutcome, UseResult};
use dereth_client_model::inventory::SplitState;
use dereth_client_model::weenie::{bitfield, item_type};
use dereth_client_model::{NullSink, RecordingRequests, RecordingSink, Request, World};
use dereth_primitives::{ObjectId, ServerTime};
use dereth_protocol::types::PublicWeenieDesc;
use dereth_testkit::{ClientSpec, HeadlessClient, Player};

/// The player these scenarios are of.
const ME: ObjectId = ObjectId(0x5000_0001);
/// The thing on the ground.
const THING: ObjectId = ObjectId(0x8000_0100);
/// A second thing, for the stack that merges.
const CARRIED: ObjectId = ObjectId(0x8000_0101);
/// The channel the client's own inventory feedback goes out on.
const FEEDBACK: u32 = 0x1A;
/// The one-thing-at-a-time refusal, verbatim: it is the sentence the player reads.
const BUSY: &str = "You can only move or use one item at a time";
/// What the client says about picking up something alive, verbatim.
const CREATURES: &str = "You cannot pick up creatures!";

/// A client over the retail data: what these scenarios want from it is the frame the gesture
/// runs in, and the place to book the claim.
fn a_client() -> HeadlessClient {
    HeadlessClient::new(ClientSpec::retail())
}

/// A player who can carry things, in `w`.
///
/// The two capacities are a premise and never a claim: the spill that decides *whether* a
/// request is built reads them, and nothing below turns on their value.
fn seed_player(w: &mut World, player: ObjectId) {
    w.player = Some(player);
    let mut me = dereth_client_model::Weenie::new(player);
    me.pwd.bitfield = bitfield::PLAYER;
    me.pwd.obj_type = item_type::CREATURE;
    me.pwd.items_capacity = Some(102);
    me.pwd.containers_capacity = Some(7);
    me.valid = true;
    w.tables.weenies.insert(player, me);
    w.tables.inventories.insert(
        player,
        dereth_client_model::objects::ObjectInventory::new(player),
    );
}

/// The thing lying loose in the world, and the player who is looking at it.
fn a_world_with_something_on_the_ground(item: ObjectId, pwd: PublicWeenieDesc) -> World {
    let mut w = World::new();
    seed_player(&mut w, ME);
    let mut it = dereth_client_model::Weenie::new(item);
    it.pwd = pwd;
    it.valid = true;
    w.tables.weenies.insert(item, it);
    w.selected = Some(item);
    w
}

/// A plain thing with a name, and nothing else said about it.
fn a_thing(name: &str) -> PublicWeenieDesc {
    PublicWeenieDesc {
        name: name.into(),
        wcid: 0x0114,
        ..PublicWeenieDesc::default()
    }
}

/// A pile of `n` coins.
fn a_pile(n: u16) -> PublicWeenieDesc {
    PublicWeenieDesc {
        name: "Pyreal".into(),
        wcid: 0x0111,
        obj_type: item_type::MONEY,
        stack_size: Some(n),
        max_stack_size: Some(25000),
        ..PublicWeenieDesc::default()
    }
}

/// Every line the world has waiting, with the channel it went out on.
fn lines(w: &World) -> Vec<(u32, String)> {
    w.scroll
        .pending()
        .iter()
        .map(|f| (f.chat_type, f.body.clone()))
        .collect()
}

// -----------------------------------------------------------------------------------------
// The decision: what a use on a loose thing does that a use on anything else does not.
// -----------------------------------------------------------------------------------------

/// **Which things a use picks up, and which it merely uses.**
///
/// Five arms; each one here differs from the first in exactly one fact about the thing, which
/// is what makes the answer discriminating rather than five one-sided measurements.
pub fn something_lying_loose_is_picked_up_and_anything_else_is_used() {
    let mut c = a_client();

    let loose = a_world_with_something_on_the_ground(THING, a_thing("Dagger"))
        .determine_use_result(THING)
        == UseResult::PlaceInBackpack;

    // In something: the general path.
    let mut w = a_world_with_something_on_the_ground(THING, a_thing("Dagger"));
    w.weenie_mut(THING).expect("seeded").pwd.container_id = Some(ME);
    let contained = w.determine_use_result(THING) == UseResult::Useable;
    // ...unless what it is in is the container the player has open, which is how loot is
    // taken off a corpse.
    w.ground_object = Some(ME);
    let looted = w.determine_use_result(THING) == UseResult::PlaceInBackpack;

    // Nailed down.
    let mut w = a_world_with_something_on_the_ground(THING, a_thing("Signpost"));
    w.weenie_mut(THING).expect("seeded").pwd.bitfield = bitfield::STUCK;
    let stuck = w.determine_use_result(THING) == UseResult::Useable;

    // In somebody else's hand.
    let mut w = a_world_with_something_on_the_ground(THING, a_thing("Sword"));
    w.weenie_mut(THING).expect("seeded").pwd.wielder_id = Some(ObjectId(0x5000_0099));
    let held = w.determine_use_result(THING) == UseResult::Useable;

    // A container of its own -- a chest, a pack, a corpse.
    let mut w = a_world_with_something_on_the_ground(THING, a_thing("Chest"));
    w.weenie_mut(THING).expect("seeded").pwd.items_capacity = Some(10);
    let a_container_itself = w.determine_use_result(THING) == UseResult::Useable;

    c.assert_behaviour(
        "inventory.pickup.something-lying-loose-is-picked-up-and-anything-else-is-used",
        move |_| loose && contained && looted && stuck && held && a_container_itself,
    );
    c.shutdown();
}

/// Something alive passes every one of those gates and is refused by name at the layer below.
pub fn a_creature_is_refused_by_name_rather_than_picked_up() {
    let mut c = a_client();
    let mut w = a_world_with_something_on_the_ground(THING, a_thing("Drudge"));
    w.weenie_mut(THING).expect("seeded").pwd.obj_type = item_type::CREATURE;
    let gets_that_far = w.determine_use_result(THING) == UseResult::PlaceInBackpack;
    let refused_by_name = w.place_in_container_item_legal(THING) == Err(CREATURES.to_owned());

    c.assert_behaviour(
        "inventory.pickup.a-creature-is-refused-by-name-rather-than-picked-up",
        move |_| gets_that_far && refused_by_name,
    );
    c.shutdown();
}

// -----------------------------------------------------------------------------------------
// The gesture, through the frame's own pass.
// -----------------------------------------------------------------------------------------

/// **The use gesture on something lying in the road asks for a move, and predicts nothing.**
///
/// This is measured at the frame's own entry point rather than at the decision function, which
/// is the stronger of the two: the request below is the one the running client's frame put in
/// its outbox, not one a test called a handler to get.
pub fn the_gesture_asks_for_one_move_and_predicts_nothing() {
    let mut c = a_client();
    *c.world_mut() = a_world_with_something_on_the_ground(THING, a_thing("Dagger"));
    c.when(Player::Ui(vec![UiRequest::Use(THING)])).tick(2);

    let asked = c.outbound()
        == [Request::PutItemInContainer(
            dereth_protocol::items::InventoryPutItemInContainer {
                item: THING,
                container: ME,
                slot: 0,
            },
        )];
    let w = c.view().world();
    let took_the_lock =
        w.request_lock.pending == InventoryRequest::PickUp && w.request_lock.object == Some(THING);
    let thing = w.weenie(THING).expect("seeded");
    // The client predicts nothing: the thing is still lying where it was, greyed while the
    // shard is asked, and it is not in the pack until the shard says so.
    let predicts_nothing = thing.waiting
        && thing.pwd.container_id.unwrap_or_default() == ObjectId(0)
        && w.inventory(ME).is_none_or(|i| i.items.is_empty());

    c.assert_behaviour(
        "inventory.pickup.the-gesture-asks-for-one-move-and-predicts-nothing",
        move |_| asked && took_the_lock && predicts_nothing,
    );
    c.shutdown();
}

// -----------------------------------------------------------------------------------------
// What stops a second one.
// -----------------------------------------------------------------------------------------

/// The short window that stops one double-click from being two pickups.
pub fn a_second_use_inside_the_short_window_sends_nothing() {
    let mut c = a_client();
    let mut w = a_world_with_something_on_the_ground(THING, a_thing("Dagger"));
    let mut first = RecordingRequests::default();
    let dispatched = w.use_object(
        &mut first,
        &mut NullSink,
        THING,
        SplitState::default(),
        ServerTime(10.0),
    ) == UseOutcome::Dispatched {
        result: UseResult::PlaceInBackpack,
        sent: true,
    } && first.0.len() == 1;

    let mut again = RecordingRequests::default();
    let throttled = w.use_object(
        &mut again,
        &mut NullSink,
        THING,
        SplitState::default(),
        ServerTime(10.1),
    ) == UseOutcome::Throttled
        && again.0.is_empty();

    c.assert_behaviour(
        "inventory.pickup.a-second-use-inside-the-short-window-sends-nothing",
        move |_| dispatched && throttled,
    );
    c.shutdown();
}

/// With a move already outstanding the pickup is refused in the client's own words, and
/// nothing at all is asked of the shard.
pub fn a_pickup_while_another_move_is_outstanding_is_refused_in_words() {
    let mut c = a_client();
    let mut w = a_world_with_something_on_the_ground(THING, a_thing("Dagger"));
    w.request_lock.record(
        ObjectId(0x8000_0999),
        InventoryRequest::Drop,
        ServerTime(0.0),
    );

    let mut req = RecordingRequests::default();
    let mut notices = RecordingSink::default();
    let busy = w.use_object(
        &mut req,
        &mut notices,
        THING,
        SplitState::default(),
        ServerTime(10.0),
    ) == UseOutcome::Busy;
    let sent_nothing = req.0.is_empty();
    let said_so = notices.0.iter().any(|n| {
        matches!(n, dereth_client_model::Notice::DisplayString { channel, text , ..}
                if *channel == FEEDBACK && text == BUSY)
    });

    c.assert_behaviour(
        "inventory.pickup.while-another-move-is-outstanding-it-is-refused-in-words",
        move |_| busy && sent_nothing && said_so,
    );
    c.shutdown();
}

/// A pile that can go into one the player already carries is merged rather than piled up
/// twice -- which is why picking up coins does not leave the player with two purses.
pub fn a_pile_that_can_merge_is_merged_rather_than_piled_up_twice() {
    let mut c = a_client();
    let mut w = a_world_with_something_on_the_ground(THING, a_pile(150));
    let mut held = dereth_client_model::Weenie::new(CARRIED);
    held.pwd = a_pile(40);
    held.pwd.container_id = Some(ME);
    held.valid = true;
    w.tables.weenies.insert(CARRIED, held);
    w.view_object_contents(
        ME,
        &[dereth_protocol::types::ContentProfile {
            iid: CARRIED,
            container_properties: 0,
        }],
        &mut NullSink,
    );

    let mut req = RecordingRequests::default();
    let dispatched = w.use_object(
        &mut req,
        &mut NullSink,
        THING,
        SplitState::whole_stack(150),
        ServerTime(1.0),
    ) == UseOutcome::Dispatched {
        result: UseResult::PlaceInBackpack,
        sent: true,
    };
    let merged = match req.0.as_slice() {
        [Request::StackableMerge(m)] => {
            m.merge_from == THING && m.merge_to == CARRIED && m.amount == 150
        }
        _ => false,
    };
    let lock = w.request_lock.pending == InventoryRequest::Merge;

    c.assert_behaviour(
        "inventory.pickup.a-pile-that-can-merge-is-merged-rather-than-piled-up-twice",
        move |_| dispatched && merged && lock,
    );
    c.shutdown();
}

// -----------------------------------------------------------------------------------------
// The wire, and the recording.
// -----------------------------------------------------------------------------------------

/// **A handler whose request nothing routes is not implemented.** The pickup is pushed
/// through the one place a request becomes bytes, and what comes out is a game action naming
/// the thing and the player.
pub fn the_request_reaches_the_wire_naming_the_thing_and_the_player() {
    use dereth_client_net::client_session::testing::MockTransport;
    use dereth_client_net::client_session::Session;

    let mut c = a_client();
    let req = Request::PutItemInContainer(dereth_protocol::items::InventoryPutItemInContainer {
        item: THING,
        container: ME,
        slot: 0,
    });
    let mut s = Session::new(MockTransport::new());
    let routed = dereth_client_runtime::requests::send_request(&mut s, &req);
    let sent = s.transport.sent.last().expect("one blob").clone();
    let dword = |n: usize| {
        u32::from_le_bytes([
            sent.payload[n],
            sent.payload[n + 1],
            sent.payload[n + 2],
            sent.payload[n + 3],
        ])
    };
    let on_the_wire = sent.queue == dereth_primitives::NetQueue::Weenie
        && sent.ordered
        && dword(0) == dereth_testkit::outbound::ORDERED_ACTION
        && dword(8) == PUT_IN_CONTAINER
        && ObjectId(dword(12)) == THING
        && ObjectId(dword(16)) == ME;

    c.assert_behaviour(
        "inventory.pickup.the-request-reaches-the-wire-naming-the-thing-and-the-player",
        move |_| routed && on_the_wire,
    );
    c.shutdown();
}

/// `Inventory_PutItemInContainer`, which is what a pickup is on the wire.
const PUT_IN_CONTAINER: u32 = 0x0019;
/// `Item_ServerSaysContainID`, the shard's own answer to one.
const SERVER_SAYS_CONTAIN: u32 = 0x0022;

/// **Full rigour on which thing.** Picking up the wrong object is invisible and acquisitive,
/// so every id here comes out of a recording: the thing, its whole descriptor, the player,
/// the container the recorded client asked for, the slot it asked for, and the shard's own
/// answer. Nothing below is a value written by hand.
///
/// The recorded client's own asks are read with [`dereth_testkit::Outbound`] and the shard's
/// answers off the decoded corpus, which is the same pair of readers the give census in this
/// file uses.
pub fn every_recorded_pickup_is_reproduced_with_the_recordings_own_ids() {
    use dereth_client_net::client_session::testing::{Corpus, Direction};
    use dereth_client_net::client_session::SessionEvent;
    use dereth_protocol::objects::ItemServerSaysContainId;
    use dereth_protocol::Message;

    let mut c = a_client();

    let mut reproduced = 0usize;
    let mut drags = 0usize;
    let mut asks = 0usize;
    let mut ok = true;
    for session in dereth_client_net::client_session::testing::session_names() {
        // One recording of the locked corpus decodes to a single blob and has no client
        // half at all; a census that walked it would die on the reader rather than say so.
        if !dereth_testkit::Outbound::has_client_half(session) {
            continue;
        }
        let Some(corpus) = Corpus::load(session).expect("the recording parses") else {
            continue;
        };
        // The shard's own answers, in recorded order, with the thing each one is about.
        let answers: Vec<(usize, ObjectId, Vec<u8>)> = corpus
            .blobs
            .iter()
            .filter(|b| b.dir == Direction::ServerToClient && b.opcode == ORDERED_EVENT)
            .filter_map(|b| {
                let blob = b.payload.get(12..)?.to_vec();
                let sub = u32::from_le_bytes([blob[0], blob[1], blob[2], blob[3]]);
                if sub != SERVER_SAYS_CONTAIN {
                    return None;
                }
                let m: ItemServerSaysContainId = dereth_protocol::read_body(&blob[4..]).ok()?;
                Some((b.idx, m.item, blob))
            })
            .collect();

        // **Which of the recorded moves are pickups is the recording's answer and not a
        // rule written here.** The recording is replayed into the client's own object
        // stream, and at the moment of each ask that world is asked whether the thing was
        // lying in the 3-D world -- which is the one fact that makes a move a pickup rather
        // than a drag from one container into another.
        let mut replayed = dereth_client_runtime::objects::ObjectStream::new();
        let mut at = 0usize;
        for ask in dereth_testkit::Outbound::all(session)
            .into_iter()
            .filter(|s| s.message == PUT_IN_CONTAINER)
        {
            asks += 1;
            for b in corpus
                .blobs
                .iter()
                .filter(|b| b.dir == Direction::ServerToClient && b.idx >= at && b.idx < ask.idx)
            {
                let now = dereth_primitives::LocalTime(
                    std::time::Duration::from_micros(b.t_rel_micros).as_secs_f64(),
                );
                replayed.apply_event(&dereth_testkit::inbound::event_of(b), now);
            }
            at = ask.idx;

            let item = ObjectId(ask.field(0).unwrap_or_default());
            let container = ObjectId(ask.field(1).unwrap_or_default());
            let slot = ask.field(2).unwrap_or_default();
            if !replayed.world.is_in_3d_world(item) {
                // A move from one container into another: that gesture is the drag, and it
                // has rows of its own.
                continue;
            }
            if slot != 0 {
                // A move off the ground that names a **place inside** the container is the
                // drag: the player aimed at one slot of one list and the gesture carries
                // where he aimed. A use names no place at all, so a recorded move that does
                // is not this gesture. One recording carries one such move; the drag has
                // rows of its own and this is not one of them.
                drags += 1;
                continue;
            }
            let (Some(pwd), Some((_, _, blob))) = (
                replayed.world.weenie(item).map(|w| w.pwd.clone()),
                answers
                    .iter()
                    .find(|(idx, id, _)| *idx > ask.idx && *id == item),
            ) else {
                continue;
            };

            // The world this ask was made in, built out of the recording's own descriptor.
            let mut w = World::new();
            seed_player(&mut w, container);
            let mut it = dereth_client_model::Weenie::new(item);
            it.pwd = pwd.clone();
            it.valid = true;
            w.tables.weenies.insert(item, it);
            w.selected = Some(item);
            if w.determine_use_result(item) != UseResult::PlaceInBackpack {
                continue;
            }

            let mut req = RecordingRequests::default();
            let dispatched = w.use_object(
                &mut req,
                &mut NullSink,
                item,
                SplitState::whole_stack(u32::from(pwd.stack_size.unwrap_or(1))),
                ServerTime(1.0),
            ) == UseOutcome::Dispatched {
                result: UseResult::PlaceInBackpack,
                sent: true,
            };
            let same = matches!(
                req.0.as_slice(),
                [Request::PutItemInContainer(m)]
                    if m.item == item && m.container == container && m.slot == slot
            );
            let ghosted = w.request_lock.pending == InventoryRequest::PickUp
                && w.weenie(item).expect("seeded").waiting;

            // The shard's own answer, byte for byte as recorded, through the client's own
            // apply path.
            let mut inter = dereth_client_runtime::interaction::Interaction::new();
            dereth_client_runtime::interaction::apply_events(
                &mut inter,
                &[SessionEvent::UiEvent {
                    opcode: ItemServerSaysContainId::OPCODE,
                    blob: blob.clone(),
                }],
                &mut w,
            );
            let answered: ItemServerSaysContainId =
                dereth_protocol::read_body(&blob[4..]).expect("the recorded answer decodes");
            let landed = inter.stats.move_items_applied == 1
                && w.weenie(item).expect("still there").pwd.container_id
                    == Some(answered.container)
                && !w.weenie(item).expect("still there").waiting
                && w.inventory(answered.container)
                    .is_some_and(|i| i.items.contains(&item))
                && w.request_lock.is_idle();

            ok &= dispatched && same && ghosted && landed;
            reproduced += 1;
        }
    }
    eprintln!(
        "pickup census: {asks} recorded moves, {reproduced} of them pickups driven end to end, \
             {drags} of them a drag onto a chosen place"
    );
    assert!(
        asks > 0,
        "the recordings carry no move at all; the corpus is the oracle here"
    );
    assert!(
        reproduced > 0,
        "no recorded pickup was driven: this scenario would be a pass over nothing"
    );

    c.assert_behaviour(
        "inventory.pickup.every-recorded-one-is-reproduced-with-the-recordings-own-ids",
        move |_| ok,
    );
    c.shutdown();
}

/// `Ordered_GameEvent`.
const ORDERED_EVENT: u32 = 0xF7B0;
dereth_testkit::scenarios! {
    scenario_something_lying_loose_is_picked_up_and_anything_else_is_used => something_lying_loose_is_picked_up_and_anything_else_is_used ["inventory.pickup.something-lying-loose-is-picked-up-and-anything-else-is-used"],
    scenario_a_creature_is_refused_by_name_rather_than_picked_up => a_creature_is_refused_by_name_rather_than_picked_up ["inventory.pickup.a-creature-is-refused-by-name-rather-than-picked-up"],
    scenario_the_gesture_asks_for_one_move_and_predicts_nothing => the_gesture_asks_for_one_move_and_predicts_nothing ["inventory.pickup.the-gesture-asks-for-one-move-and-predicts-nothing"],
    scenario_a_second_use_inside_the_short_window_sends_nothing => a_second_use_inside_the_short_window_sends_nothing ["inventory.pickup.a-second-use-inside-the-short-window-sends-nothing"],
    scenario_a_pickup_while_another_move_is_outstanding_is_refused_in_words => a_pickup_while_another_move_is_outstanding_is_refused_in_words ["inventory.pickup.while-another-move-is-outstanding-it-is-refused-in-words"],
    scenario_a_pile_that_can_merge_is_merged_rather_than_piled_up_twice => a_pile_that_can_merge_is_merged_rather_than_piled_up_twice ["inventory.pickup.a-pile-that-can-merge-is-merged-rather-than-piled-up-twice"],
    scenario_the_request_reaches_the_wire_naming_the_thing_and_the_player => the_request_reaches_the_wire_naming_the_thing_and_the_player ["inventory.pickup.the-request-reaches-the-wire-naming-the-thing-and-the-player"],
    scenario_every_recorded_pickup_is_reproduced_with_the_recordings_own_ids => every_recorded_pickup_is_reproduced_with_the_recordings_own_ids ["inventory.pickup.every-recorded-one-is-reproduced-with-the-recordings-own-ids"],
}
