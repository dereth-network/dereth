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
/// waits for that incarnation's creation.
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
    q.receive(0.2, create_item(id, 2, 0));
    let got: Vec<u32> = drain(&mut q).iter().filter_map(effect_of).collect();
    assert_eq!(got, vec![0x21]);
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

/// A visual stream blocked on a missing stamp for more than 20 seconds is given up: the client asks
/// for the object again and discards what was held.
///
/// Behaviour: model.events.a-visual-stream-blocked-20-seconds-is-given-up
#[test]
fn a_visual_stream_blocked_for_20_seconds_is_given_up() {
    let id = 0x8130_0000_0000_0004;
    let mut q = EventQueue::new();
    q.receive(0.0, create_item(id, 1, 1));
    drain(&mut q);
    q.receive(1.0, fx(id, 1, 3, 0x3));
    q.use_time(20.9);
    assert!(q.take_force_creates().is_empty());
    q.use_time(21.1);
    assert_eq!(q.take_force_creates(), vec![id]);
    q.receive(22.0, fx(id, 1, 2, 0x2));
    let got: Vec<u32> = drain(&mut q).iter().filter_map(effect_of).collect();
    assert_eq!(got, vec![0x2], "the held stamp 3 went with the give-up");
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
