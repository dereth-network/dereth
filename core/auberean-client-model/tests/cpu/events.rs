//! The event queue: what waits, and in what order messages take effect.

use auberean_client_model::events::{EventQueue, Item};
use auberean_protocol::ServerMessage;

use crate::{create_item, fx, player_object, private_int, PLAYER};

fn drain(q: &mut EventQueue) -> Vec<Item> {
    let mut v = Vec::new();
    while let Some(i) = q.pop() {
        v.push(i);
    }
    v
}

fn effect_of(i: &Item) -> Option<u32> {
    match &i.message {
        ServerMessage::DoFx(e) => Some(e.action.fx),
        _ => None,
    }
}

fn quality_of(i: &Item) -> Option<u32> {
    match &i.message {
        ServerMessage::UpdateInt(q) => Some(q.quality),
        _ => None,
    }
}

/// While the client is entering the world, everything but the player's creation waits for it;
/// once the player is created, the world's messages wait for the player's object, and its creation
/// lets them through in the order they came.
///
/// Behaviour: model.events.the-world-waits-for-the-player
#[test]
fn the_world_waits_for_the_player_and_its_object() {
    let mut q = EventQueue::new();
    q.start_entering();
    q.receive(0.0, private_int(1, 280));
    q.receive(0.0, create_item(0x8130_0000_0000_0001, 1, 1));
    assert!(
        drain(&mut q).is_empty(),
        "nothing takes effect before the player's creation"
    );
    q.receive(0.1, player_object());
    let first = drain(&mut q);
    // The player's creation, then the object (unordered); the private update waits for the
    // player's object.
    assert_eq!(first.len(), 2);
    assert!(matches!(first[0].message, ServerMessage::PlayerObject(_)));
    assert!(matches!(first[1].message, ServerMessage::CreateObject(_)));
    q.receive(0.2, create_item(PLAYER, 1, 1));
    let then = drain(&mut q);
    assert_eq!(then.len(), 2);
    assert!(matches!(then[0].message, ServerMessage::CreateObject(_)));
    assert_eq!(quality_of(&then[1]), Some(280));
}

/// Private messages take effect in stamp order whatever order they arrive in; an older or repeated
/// stamp is dropped.
///
/// Behaviour: model.events.private-messages-in-stamp-order
#[test]
fn private_messages_take_effect_in_stamp_order() {
    let mut q = EventQueue::new();
    q.start_entering();
    q.receive(0.0, player_object());
    q.receive(0.0, create_item(PLAYER, 1, 1));
    drain(&mut q);
    q.receive(0.1, private_int(1, 10));
    q.receive(0.1, private_int(3, 30));
    q.receive(0.1, private_int(4, 40));
    q.receive(0.1, private_int(2, 20));
    q.receive(0.1, private_int(2, 99));
    let got: Vec<u32> = drain(&mut q).iter().filter_map(quality_of).collect();
    assert_eq!(got, vec![10, 20, 30, 40]);
    assert_eq!(q.stats().dropped, 1);
}

/// An object's visual messages take effect in visual-stamp order after its creation's stamp; one
/// stamped at or before the creation is dropped, and one about a newer incarnation of the object
/// waits while the client has the older one.
///
/// Behaviour: model.events.visual-messages-in-stamp-order-per-object
#[test]
fn visual_messages_take_effect_in_stamp_order_per_object() {
    let id = 0x8130_0000_0000_0002;
    let mut q = EventQueue::new();
    q.receive(0.0, create_item(id, 1, 8));
    drain(&mut q);
    q.receive(0.1, fx(id, 1, 10, 0xA));
    q.receive(0.1, fx(id, 1, 8, 0x8));
    q.receive(0.1, fx(id, 1, 9, 0x9));
    q.receive(0.1, fx(id, 2, 1, 0x21));
    let got: Vec<u32> = drain(&mut q).iter().filter_map(effect_of).collect();
    assert_eq!(got, vec![0x9, 0xA]);
    assert_eq!(
        q.waiting_objects(),
        1,
        "the newer incarnation's effect waits"
    );
}

/// A creation of a newer incarnation replaces the one the client has: the old one goes first,
/// and with it its forgetting and its time to be destroyed, which no longer take the new one.
///
/// Behaviour: model.events.a-newer-incarnation-replaces-the-old-one
#[test]
fn a_newer_incarnation_replaces_the_old_one_and_its_forgetting_and_time_to_be_destroyed() {
    use auberean_protocol::physics::ObjectOnly;
    use auberean_protocol::types::ObjectStamp;
    let id = 0x8130_0000_0000_0003;
    let mut q = EventQueue::new();
    q.receive(0.0, create_item(id, 1, 1));
    drain(&mut q);
    q.add_object_to_be_destroyed(id, 0.0);
    q.receive(0.2, create_item(id, 2, 0));
    drain(&mut q);
    assert_eq!(q.stats().replaced, 1);
    assert_eq!(q.to_be_destroyed(id), None);
    q.use_time(30.0);
    assert!(
        q.take_destroyed().is_empty(),
        "the old one's time went with it"
    );
    // Forgotten, then made again newer before the upkeep: the new incarnation stays.
    q.receive(
        30.1,
        Item {
            blob_id: 3 << 56,
            message: ServerMessage::ForgetObject(ObjectOnly(ObjectStamp {
                id,
                instance_stamp: 2,
                other_stamp: 0,
            })),
        },
    );
    q.receive(30.1, create_item(id, 3, 0));
    drain(&mut q);
    q.use_time(30.2);
    assert!(q.take_destroyed().is_empty());
    assert!(q.has_object(id));
}

/// What came about a newer incarnation of an object while the client had the older one takes
/// effect once the newer one is created, in place of the old one: the September 2013 client drops
/// it with the old incarnation, which it had let go of 25 seconds after it left the world.
///
/// Behaviour: model.events.what-came-about-a-newer-incarnation-takes-effect-after-it
#[test]
fn what_came_about_a_newer_incarnation_takes_effect_after_its_creation() {
    let id = 0x8130_0000_0000_0009;
    let mut q = EventQueue::new();
    q.receive(0.0, create_item(id, 1, 8));
    drain(&mut q);
    q.receive(0.1, fx(id, 2, 1, 0x21));
    assert!(drain(&mut q).is_empty());
    q.receive(0.2, create_item(id, 2, 0));
    let got: Vec<u32> = drain(&mut q).iter().filter_map(effect_of).collect();
    assert_eq!(got, vec![0x21]);
    assert_eq!(q.waiting_objects(), 0);
}

/// What waited for an object goes again when it is created, in the order it came; one that must
/// wait again (about a newer incarnation) stays in the object's queue, which still asks for the
/// object when it is given up if anything that waited in it did.
///
/// Behaviour: model.events.what-waited-goes-again-at-the-creation
#[test]
fn what_waited_for_an_object_goes_again_and_what_must_wait_again_stays_in_its_queue() {
    let id = 0x8130_0000_0000_0004;
    let mut q = EventQueue::new();
    q.receive(0.0, fx(id, 1, 2, 0x2));
    q.receive(0.0, fx(id, 1, 3, 0x3));
    // About a newer incarnation, sent to everyone in a cell (no ask of its own).
    let mut newer = fx(id, 2, 1, 0x21);
    newer.blob_id |= 0x4000_0000_0000_0000;
    q.receive(1.0, newer);
    q.receive(2.0, create_item(id, 1, 1));
    let got: Vec<u32> = drain(&mut q).iter().filter_map(effect_of).collect();
    assert_eq!(got, vec![0x2, 0x3]);
    assert_eq!(
        q.waiting_objects(),
        1,
        "the newer incarnation's effect waits again"
    );
    q.use_time(26.0);
    assert_eq!(q.waiting_objects(), 1, "25 seconds from its waiting again");
    q.use_time(27.1);
    assert_eq!(q.waiting_objects(), 0);
    assert_eq!(q.take_force_creates(), vec![id]);
}

/// The client's book on an object keeps the cell its physics object stands in, as the scene
/// says: the objects in a cell are found by it, and an object leaving every cell is in none.
///
/// Behaviour: model.objects.a-book-keeps-the-cell-its-object-stands-in
#[test]
fn an_objects_book_keeps_the_cell_its_physics_object_stands_in() {
    let (a, b) = (0x8130_0000_0000_0005, 0x8130_0000_0000_0006);
    let mut q = EventQueue::new();
    q.receive(0.0, create_item(a, 1, 1));
    q.receive(0.0, create_item(b, 1, 1));
    drain(&mut q);
    assert_eq!(
        q.cell_of(a),
        Some(0),
        "in no cell until its body enters one"
    );
    q.change_cell(a, Some(0x7F7F_0101));
    q.change_cell(b, Some(0x7F7F_0101));
    q.change_cell(0x8130_0000_0000_00FF, Some(0x7F7F_0101));
    assert_eq!(q.objects_in_cell(0x7F7F_0101), vec![a, b]);
    q.change_cell(b, Some(0x7F7F_0102));
    q.change_cell(a, None);
    assert_eq!(q.cell_of(a), Some(0));
    assert_eq!(q.objects_in_cell(0x7F7F_0101), Vec::<u64>::new());
    assert_eq!(q.objects_in_cell(0x7F7F_0102), vec![b]);
    assert_eq!(q.cell_of(0x8130_0000_0000_00FF), None);
}

/// The camera's stand-ins and the objects the client makes itself are among its objects, each
/// with a physics object and no game object: a stand-in is made once; an object the client made
/// ends when its lifetime does, and asked to be destroyed it goes a quarter second on, as a game
/// object goes 25 seconds on.
///
/// Behaviour: model.objects.stand-ins-and-the-clients-own-objects-are-kept
#[test]
fn stand_ins_and_the_objects_the_client_makes_are_kept_without_game_objects() {
    let camera = 0x1200_0000_0048_0001;
    let made = 0x1300_0000_0000_0001;
    let game = 0x8130_0000_0000_0007;
    let mut q = EventQueue::new();
    q.receive(0.0, create_item(game, 1, 1));
    drain(&mut q);
    assert_eq!(q.has_game_object(game), Some(true));
    assert_eq!(q.has_physics_object(game), Some(true));
    assert!(q.create_dummy_object(camera));
    assert!(!q.create_dummy_object(camera), "made once");
    assert!(!q.create_dummy_object(game));
    assert_eq!(q.has_game_object(camera), Some(false));
    assert_eq!(q.has_physics_object(camera), Some(true));
    q.initialize_temporary_object(made, 3.0, 10.0);
    assert!(q.has_object(made));
    assert_eq!(q.has_game_object(made), Some(false));
    assert_eq!(q.temporary_ends(made), Some(13.0));
    q.initialize_temporary_object(made + 1, 0.0, 10.0);
    assert_eq!(q.temporary_ends(made + 1), None, "no lifetime, no end");
    q.add_object_to_be_destroyed(made, 11.0);
    assert_eq!(q.to_be_destroyed(made), Some(11.25));
    q.use_time(11.25);
    assert_eq!(q.take_destroyed(), vec![made]);
    assert_eq!(q.temporary_ends(made), None);
    q.remove_object(made + 1);
    assert!(!q.has_object(made + 1));
    q.leave_world();
    assert!(!q.has_object(camera));
}

/// A queue waiting for an object that is being made from its records is never given up, however
/// long it waits.
///
/// Behaviour: model.events.a-queue-for-an-object-being-made-waits
#[test]
fn a_queue_for_an_object_being_made_is_never_given_up() {
    let id = 0x8130_0000_0000_0008;
    let mut q = EventQueue::new();
    q.receive(0.0, fx(id, 1, 2, 0x2));
    q.use_time_creating(100.0, |o| o == id);
    assert_eq!(q.waiting_objects(), 1);
    assert!(q.take_force_creates().is_empty());
    q.use_time_creating(100.0, |_| false);
    assert_eq!(q.waiting_objects(), 0);
    assert_eq!(q.take_force_creates(), vec![id]);
}

/// Messages about an object the client does not have wait for it; nothing added for 25 seconds,
/// the client asks the server to send the object again and drops what waited.
///
/// Behaviour: model.events.an-object-not-created-in-25-seconds-is-asked-for
#[test]
fn an_object_not_created_within_25_seconds_is_asked_for_again() {
    let id = 0x8130_0000_0000_0003;
    let mut q = EventQueue::new();
    q.receive(10.0, fx(id, 1, 1, 0x1));
    q.receive(20.0, fx(id, 1, 2, 0x2));
    q.use_time(44.9);
    assert!(
        q.take_force_creates().is_empty(),
        "25 s from the last message, not the first"
    );
    q.use_time(45.1);
    assert_eq!(q.take_force_creates(), vec![id]);
    assert_eq!(q.waiting_objects(), 0);
    q.receive(46.0, create_item(id, 1, 0));
    assert_eq!(
        drain(&mut q).iter().filter_map(effect_of).count(),
        0,
        "what waited was dropped"
    );
}

/// A visual stream blocked on a missing stamp is looked at again only when the next visual message
/// about its object comes (or the object is created again): blocked for more than 20 seconds then,
/// the client asks for the object again and discards what was held, the new message with it. The
/// upkeep alone gives nothing up, and a missing message that comes late lets everything held
/// through.
///
/// Behaviour: model.events.a-visual-stream-blocked-20-seconds-is-given-up
#[test]
fn a_visual_stream_blocked_for_20_seconds_is_given_up() {
    let id = 0x8130_0000_0000_0004;
    let mut q = EventQueue::new();
    q.receive(0.0, create_item(id, 1, 1));
    drain(&mut q);
    q.receive(1.0, fx(id, 1, 3, 0x3));
    q.use_time(30.0);
    assert!(
        q.take_force_creates().is_empty(),
        "the upkeep does not look at a blocked stream"
    );
    q.receive(30.0, fx(id, 1, 2, 0x2));
    let got: Vec<u32> = drain(&mut q).iter().filter_map(effect_of).collect();
    assert_eq!(
        got,
        vec![0x2, 0x3],
        "the late message lets the held one through"
    );
    q.receive(31.0, fx(id, 1, 5, 0x5));
    q.receive(51.0, fx(id, 1, 6, 0x6));
    assert!(q.take_force_creates().is_empty(), "20 seconds, not more");
    q.receive(51.5, fx(id, 1, 7, 0x7));
    assert_eq!(q.take_force_creates(), vec![id]);
    q.receive(52.0, fx(id, 1, 4, 0x4));
    let got: Vec<u32> = drain(&mut q).iter().filter_map(effect_of).collect();
    assert_eq!(got, vec![0x4], "stamps 5 to 7 went with the give-up");
}

/// An object the server has the client forget stays until the next upkeep: what comes with the
/// forgetting still finds it; after the upkeep it is gone, and what then comes about it waits for
/// a new creation.
///
/// Behaviour: model.events.a-forgotten-object-goes-at-the-next-upkeep
#[test]
fn a_forgotten_object_goes_at_the_next_upkeep() {
    use auberean_protocol::physics::ObjectOnly;
    use auberean_protocol::types::ObjectStamp;
    let id = 0x8130_0000_0000_0005;
    let forget = |stamp: u16| Item {
        blob_id: 3 << 56,
        message: ServerMessage::ForgetObject(ObjectOnly(ObjectStamp {
            id,
            instance_stamp: 1,
            other_stamp: stamp,
        })),
    };
    let mut q = EventQueue::new();
    q.receive(0.0, create_item(id, 1, 1));
    drain(&mut q);
    q.receive(1.0, forget(0));
    q.receive(1.0, fx(id, 1, 2, 0x2));
    let got: Vec<u32> = drain(&mut q).iter().filter_map(effect_of).collect();
    assert_eq!(
        got,
        vec![0x2],
        "the effect that came with the forgetting takes effect"
    );
    assert!(q.has_object(id));
    q.use_time(1.1);
    assert_eq!(q.take_destroyed(), vec![id]);
    assert!(!q.has_object(id));
    q.receive(1.2, fx(id, 1, 3, 0x3));
    assert!(drain(&mut q).is_empty());
    assert_eq!(q.waiting_objects(), 1);
}

/// An object asked to be destroyed goes at the first upkeep 25 seconds on (a temporary object's
/// a quarter second on), unless the asking is called off; asking again moves the time, and an
/// object the client does not have is not asked for.
///
/// Behaviour: model.events.an-object-asked-to-be-destroyed-goes-in-25-seconds
#[test]
fn an_object_asked_to_be_destroyed_goes_in_25_seconds_unless_called_off() {
    let id = 0x8130_0000_0000_0007;
    let other = 0x8130_0000_0000_0008;
    let temporary = 0x1300_0000_0000_0009;
    let mut q = EventQueue::new();
    for o in [id, other, temporary] {
        q.receive(0.0, create_item(o, 1, 1));
    }
    drain(&mut q);
    q.add_object_to_be_destroyed(id, 1.0);
    q.add_object_to_be_destroyed(other, 1.0);
    q.add_object_to_be_destroyed(temporary, 1.0);
    q.add_object_to_be_destroyed(0x8130_0000_0000_00FF, 1.0);
    assert_eq!(q.to_be_destroyed(0x8130_0000_0000_00FF), None);
    q.remove_object_to_be_destroyed(other);
    q.add_object_to_be_destroyed(id, 5.0);
    q.use_time(1.25);
    assert_eq!(q.take_destroyed(), vec![temporary]);
    q.use_time(29.9);
    assert!(q.take_destroyed().is_empty(), "asked again at 5: due at 30");
    q.use_time(30.0);
    assert_eq!(q.take_destroyed(), vec![id]);
    q.use_time(100.0);
    assert!(q.take_destroyed().is_empty(), "the called-off object stays");
    assert!(q.has_object(other));
}

/// A let-go of an item the client does not have yet never lets go of it: the September 2013 client
/// holds the let-go for the item and, when the item comes, sends it through its holder's visual
/// order again, where its stamp is already taken, so it is dropped; this client takes it at once,
/// finding no item. Either way the item, made and then held, stays held.
///
/// Behaviour: model.events.a-let-go-of-an-item-not-yet-made-never-takes-effect
#[test]
fn a_let_go_of_an_item_not_yet_made_never_takes_effect() {
    use auberean_client_model::Model;
    use auberean_protocol::physics::{Attach, ObjectEvent};
    use auberean_protocol::types::ObjectStamp;
    const HOLDER: u64 = 0x8130_0000_0000_0010;
    const HELD: u64 = 0x8130_0000_0000_0011;
    let stamp = |id, instance_stamp, other_stamp| ObjectStamp {
        id,
        instance_stamp,
        other_stamp,
    };
    let mut q = EventQueue::new();
    let mut m = Model::new();
    let run = |q: &mut EventQueue, m: &mut Model| {
        let items = drain(q);
        for i in &items {
            m.apply(i);
        }
        items
    };
    q.start_entering();
    q.receive(0.0, player_object());
    q.receive(0.0, create_item(PLAYER, 1, 1));
    q.receive(0.0, create_item(HOLDER, 1, 1));
    run(&mut q, &mut m);
    q.receive(
        0.1,
        crate::item(
            2,
            0,
            ServerMessage::DeParent(ObjectEvent {
                sender: stamp(HOLDER, 1, 2),
                action: stamp(HELD, 1, 3),
            }),
        ),
    );
    run(&mut q, &mut m);
    q.receive(0.2, create_item(HELD, 1, 1));
    q.receive(
        0.2,
        crate::item(
            2,
            0,
            ServerMessage::Parent(ObjectEvent {
                sender: stamp(HELD, 1, 2),
                action: Attach {
                    parent: stamp(HOLDER, 1, 0),
                    location: 1,
                    orientation: 0,
                },
            }),
        ),
    );
    let after = run(&mut q, &mut m);
    assert!(
        !after
            .iter()
            .any(|i| matches!(i.message, ServerMessage::DeParent(_))),
        "the let-go does not come back"
    );
    assert_eq!(m.object(HELD).and_then(|o| o.parent), Some((HOLDER, 1)));
    for _ in 0..3 {
        q.use_time(10.0);
        run(&mut q, &mut m);
    }
    assert_eq!(m.object(HELD).and_then(|o| o.parent), Some((HOLDER, 1)));
}

/// An unordered message held while entering goes again under the world's rules once the player is
/// created: it waits for the player's object like any other, and takes effect after it.
///
/// Behaviour: model.events.held-messages-wait-for-the-players-object
#[test]
fn a_message_held_while_entering_then_waits_for_the_players_object() {
    const OTHER: u64 = 0x8130_0000_0000_0002;
    let mut q = EventQueue::new();
    q.start_entering();
    q.receive(0.0, create_item(OTHER, 1, 1));
    let mut effect = fx(OTHER, 1, 0, 0x55);
    effect.blob_id = crate::blob(3, 0);
    q.receive(0.0, effect);
    q.receive(0.1, player_object());
    let first = drain(&mut q);
    assert_eq!(first.len(), 2, "the player's creation and the object's");
    assert!(matches!(first[0].message, ServerMessage::PlayerObject(_)));
    assert!(matches!(first[1].message, ServerMessage::CreateObject(_)));
    q.receive(0.2, create_item(PLAYER, 1, 1));
    let then = drain(&mut q);
    assert_eq!(then.len(), 2);
    assert!(matches!(then[0].message, ServerMessage::CreateObject(_)));
    assert_eq!(effect_of(&then[1]), Some(0x55));
}

/// Leaving the world lets go of what was held while entering: entering again starts empty.
///
/// Behaviour: model.events.leaving-the-world-drops-what-was-held
#[test]
fn leaving_the_world_drops_what_was_held_while_entering() {
    let mut q = EventQueue::new();
    q.start_entering();
    q.receive(0.0, create_item(0x8130_0000_0000_0002, 1, 1));
    q.leave_world();
    q.start_entering();
    q.receive(0.1, player_object());
    let got = drain(&mut q);
    assert_eq!(got.len(), 1, "only the player's creation");
    assert!(matches!(got[0].message, ServerMessage::PlayerObject(_)));
}
