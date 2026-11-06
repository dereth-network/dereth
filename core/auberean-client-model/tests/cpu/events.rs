//! The event queue: what waits, and in what order messages take effect.

use auberean_client_model::events::{newer, EventQueue, Item};
use auberean_protocol::login::Empty;
use auberean_protocol::ServerMessage;

use crate::{create_item, fx, item, player_object, private_int, PLAYER};

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

/// The player, in the world, with the private stream not started.
fn in_the_world() -> EventQueue {
    let mut q = EventQueue::new();
    q.start_entering();
    q.receive(0.0, player_object());
    q.receive(0.0, create_item(PLAYER, 1, 1));
    drain(&mut q);
    q
}

fn private_taken(q: &mut EventQueue) -> Vec<u32> {
    drain(q).iter().filter_map(quality_of).collect()
}

/// Twenty messages held and five minutes blocked give up one stamp as a message arrives, not the
/// whole gap. Stamps 2 to 4 are missing: each give-up takes one and starts the five minutes again,
/// and only the last lets what is held through.
///
/// Behaviour: model.events.a-private-gap-is-given-up-one-stamp-at-a-time
#[test]
fn a_private_gap_is_given_up_one_stamp_per_five_minutes() {
    let mut q = in_the_world();
    q.receive(1.0, private_int(1, 1));
    assert_eq!(private_taken(&mut q), vec![1]);
    for stamp in 5..=23 {
        q.receive(2.0, private_int(stamp, u32::from(stamp)));
    }
    assert!(private_taken(&mut q).is_empty(), "nineteen held, none next");
    q.receive(2.0, private_int(24, 24));
    q.receive(302.5, private_int(25, 25));
    assert!(
        private_taken(&mut q).is_empty(),
        "stamp 2 given up; 5 is not next, so the five minutes start again"
    );
    q.receive(602.0, private_int(26, 26));
    assert!(
        private_taken(&mut q).is_empty(),
        "the five minutes run from the first give-up"
    );
    q.receive(603.0, private_int(27, 27));
    assert!(private_taken(&mut q).is_empty(), "stamp 3 given up");
    q.receive(904.0, private_int(28, 28));
    assert_eq!(
        private_taken(&mut q),
        (5..=28).collect::<Vec<u32>>(),
        "stamp 4 given up, and all that was held follows"
    );
    assert_eq!(q.stats().dropped, 0);
}

/// The blob id of a private message under `stamp`.
fn private_id(stamp: u16) -> u64 {
    private_int(stamp, 0).blob_id
}

/// A private message that does not read counts for its stamp: what follows it takes effect at
/// once, whichever order they come in, and nothing is held afterward.
///
/// Behaviour: model.events.a-private-message-that-does-not-read-holds-nothing-back
#[test]
fn a_private_message_that_does_not_read_holds_nothing_back() {
    let mut q = in_the_world();
    q.receive(1.0, private_int(1, 1));
    q.receive_unread(1.0, private_id(2), &[]);
    q.receive(1.0, private_int(3, 3));
    assert_eq!(private_taken(&mut q), vec![1, 3], "it came in its place");
    q.receive(2.0, private_int(5, 5));
    assert!(
        private_taken(&mut q).is_empty(),
        "the stamp before it hasn't come"
    );
    q.receive_unread(2.0, private_id(4), &[]);
    assert_eq!(
        private_taken(&mut q),
        vec![5],
        "it came late, and let go of what it held"
    );
    q.receive(3.0, private_int(6, 6));
    assert_eq!(private_taken(&mut q), vec![6]);
    assert_eq!(q.stats().dropped, 0);
    q.receive_unread(3.0, private_id(6), &[]);
    q.receive(3.0, private_int(7, 7));
    assert_eq!(
        private_taken(&mut q),
        vec![7],
        "a repeat of one taken changes nothing"
    );
}

/// Only a private message's stamp counts: one of another kind that does not read leaves the
/// private stream as it was.
///
/// Behaviour: model.events.a-private-message-that-does-not-read-holds-nothing-back
#[test]
fn a_message_of_another_kind_that_does_not_read_counts_for_no_private_stamp() {
    let mut q = in_the_world();
    q.receive(1.0, private_int(1, 1));
    q.receive_unread(
        1.0,
        item(3, 2, ServerMessage::DownloadEnds(Empty)).blob_id,
        &[],
    );
    q.receive_unread(
        1.0,
        item(2, 2, ServerMessage::DownloadEnds(Empty)).blob_id,
        &[],
    );
    q.receive(1.0, private_int(3, 3));
    assert_eq!(private_taken(&mut q), vec![1], "stamp 2 is still missing");
    q.receive(1.0, private_int(2, 2));
    assert_eq!(private_taken(&mut q), vec![2, 3]);
}

/// While the player is being made, a private message that does not read waits as a readable one
/// would; out of the world, and past leaving it, it counts for nothing.
///
/// Behaviour: model.events.a-private-message-that-does-not-read-holds-nothing-back
#[test]
fn a_private_message_that_does_not_read_waits_for_the_player_as_any_would() {
    let mut q = EventQueue::new();
    q.receive_unread(0.0, private_id(1), &[]);
    q.start_entering();
    q.receive_unread(0.0, private_id(2), &[]);
    q.receive(0.0, private_int(1, 1));
    q.receive(0.0, private_int(3, 3));
    q.receive(0.0, player_object());
    assert!(private_taken(&mut q).is_empty(), "no player's object yet");
    q.receive(0.0, create_item(PLAYER, 1, 1));
    assert_eq!(private_taken(&mut q), vec![1, 3]);

    let mut q = EventQueue::new();
    q.start_entering();
    q.receive_unread(0.0, private_id(2), &[]);
    q.leave_world();
    q.start_entering();
    q.receive(0.0, player_object());
    q.receive(0.0, create_item(PLAYER, 1, 1));
    drain(&mut q);
    q.receive(0.0, private_int(1, 1));
    q.receive(0.0, private_int(2, 2));
    assert_eq!(
        private_taken(&mut q),
        vec![1, 2],
        "the stamp left with the world"
    );
    assert_eq!(q.stats().dropped, 0);
}

/// It takes twenty held and more than five minutes: nineteen held after any wait give up nothing,
/// and neither do twenty held after exactly five minutes.
///
/// Behaviour: model.events.a-private-gap-is-given-up-one-stamp-at-a-time
#[test]
fn a_private_give_up_takes_twenty_held_and_more_than_five_minutes() {
    let mut late = in_the_world();
    late.receive(1.0, private_int(1, 1));
    drain(&mut late);
    for stamp in 3..=20 {
        late.receive(2.0, private_int(stamp, u32::from(stamp)));
    }
    late.receive(900.0, private_int(21, 21));
    assert!(private_taken(&mut late).is_empty(), "nineteen held");
    late.receive(900.0, private_int(22, 22));
    assert_eq!(private_taken(&mut late), (3..=22).collect::<Vec<u32>>());

    let mut exact = in_the_world();
    exact.receive(1.0, private_int(1, 1));
    drain(&mut exact);
    for stamp in 3..=21 {
        exact.receive(2.0, private_int(stamp, u32::from(stamp)));
    }
    exact.receive(302.0, private_int(22, 22));
    assert!(private_taken(&mut exact).is_empty(), "exactly five minutes");
    exact.receive(302.5, private_int(23, 23));
    assert_eq!(private_taken(&mut exact), (3..=23).collect::<Vec<u32>>());
}

/// A stamp of the gap that comes late, before the give-ups reach it, still takes effect; one they
/// have passed is dropped.
///
/// Behaviour: model.events.a-private-gap-is-given-up-one-stamp-at-a-time
#[test]
fn a_private_stamp_the_give_ups_have_not_reached_still_takes_effect() {
    let mut q = in_the_world();
    q.receive(1.0, private_int(1, 1));
    drain(&mut q);
    for stamp in 5..=24 {
        q.receive(2.0, private_int(stamp, u32::from(stamp)));
    }
    q.receive(303.0, private_int(25, 25));
    assert!(private_taken(&mut q).is_empty(), "stamp 2 given up");
    q.receive(304.0, private_int(2, 2));
    assert!(private_taken(&mut q).is_empty(), "stamp 2 has gone");
    assert_eq!(q.stats().dropped, 1);
    q.receive(305.0, private_int(3, 3));
    assert_eq!(private_taken(&mut q), vec![3], "stamp 3 had not gone");
    q.receive(306.0, private_int(4, 4));
    assert_eq!(
        private_taken(&mut q),
        (4..=25).collect::<Vec<u32>>(),
        "stamp 4 closes the gap"
    );
    assert_eq!(q.stats().dropped, 1);
}

/// When a late message lets some of what is held through and the next held one is still not next,
/// the five minutes start again from then.
///
/// Behaviour: model.events.a-private-gap-is-given-up-one-stamp-at-a-time
#[test]
fn taking_what_a_late_message_lets_through_starts_the_five_minutes_again() {
    let mut q = in_the_world();
    q.receive(1.0, private_int(1, 1));
    drain(&mut q);
    q.receive(2.0, private_int(3, 3));
    for stamp in 5..=23 {
        q.receive(2.0, private_int(stamp, u32::from(stamp)));
    }
    q.receive(290.0, private_int(2, 2));
    assert_eq!(private_taken(&mut q), vec![2, 3], "stamp 4 still missing");
    q.receive(303.0, private_int(24, 24));
    assert!(
        private_taken(&mut q).is_empty(),
        "twenty held, but only 13 seconds since the head was last looked at"
    );
    q.receive(591.0, private_int(25, 25));
    assert_eq!(
        private_taken(&mut q),
        (5..=25).collect::<Vec<u32>>(),
        "more than five minutes on: stamp 4 given up"
    );
}

/// A stamp already held, past a gap, is held again: each takes effect, the later arrival first.
/// A stamp already taken is still dropped.
///
/// Behaviour: model.events.a-repeated-private-stamp-past-a-gap-takes-effect-twice
#[test]
fn a_private_stamp_repeated_past_a_gap_takes_effect_twice_later_arrival_first() {
    let mut q = in_the_world();
    q.receive(1.0, private_int(1, 1));
    drain(&mut q);
    q.receive(1.1, private_int(3, 30));
    q.receive(1.2, private_int(3, 31));
    q.receive(1.3, private_int(4, 40));
    assert!(private_taken(&mut q).is_empty());
    q.receive(1.4, private_int(2, 20));
    assert_eq!(private_taken(&mut q), vec![20, 31, 30, 40]);
    assert_eq!(q.stats().dropped, 0);
    q.receive(1.5, private_int(4, 41));
    assert!(private_taken(&mut q).is_empty());
    assert_eq!(q.stats().dropped, 1);
}

/// A message repeated past a gap counts toward the twenty held.
///
/// Behaviour: model.events.a-repeated-private-stamp-past-a-gap-takes-effect-twice
#[test]
fn a_repeated_private_stamp_counts_toward_the_twenty_held() {
    let mut q = in_the_world();
    q.receive(1.0, private_int(1, 1));
    drain(&mut q);
    for n in 0..19 {
        q.receive(2.0, private_int(3, n));
    }
    assert!(private_taken(&mut q).is_empty());
    q.receive(303.0, private_int(3, 19));
    assert_eq!(
        private_taken(&mut q),
        (0..=19).rev().collect::<Vec<u32>>(),
        "twenty held under one stamp: stamp 2 given up, the newest arrival first"
    );
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

/// The bytes of a visual message of a type this client doesn't read, about `id`.
fn unread_visual(id: u64, instance: u16, visual: u16) -> Vec<u8> {
    let mut raw = 0x1_FFFF_u32.to_le_bytes().to_vec();
    raw.extend_from_slice(&id.to_le_bytes());
    raw.extend_from_slice(&instance.to_le_bytes());
    raw.extend_from_slice(&visual.to_le_bytes());
    raw
}

/// A visual message that does not read counts for its object's stamp, read from its head: what
/// follows takes effect at once, whichever order they come in. About an object not here yet it
/// waits with what waits for it, and one too short to name an object counts for nothing.
///
/// Behaviour: model.events.a-visual-message-that-does-not-read-holds-nothing-back
#[test]
fn a_visual_message_that_does_not_read_holds_nothing_back() {
    let id = 0x8130_0000_0000_0007;
    let visual_blob = fx(id, 1, 0, 0).blob_id;
    let mut q = EventQueue::new();
    q.receive(0.0, create_item(id, 1, 1));
    drain(&mut q);
    q.receive_unread(1.0, visual_blob, &unread_visual(id, 1, 2));
    q.receive(1.0, fx(id, 1, 3, 0x3));
    let got: Vec<u32> = drain(&mut q).iter().filter_map(effect_of).collect();
    assert_eq!(got, vec![0x3], "it came in its place");
    q.receive(2.0, fx(id, 1, 5, 0x5));
    assert!(drain(&mut q).is_empty(), "the stamp before it hasn't come");
    q.receive_unread(2.0, visual_blob, &unread_visual(id, 1, 4));
    let got: Vec<u32> = drain(&mut q).iter().filter_map(effect_of).collect();
    assert_eq!(got, vec![0x5], "it came late, and let go of what it held");
    assert_eq!(q.stats().force_creates, 0);

    q.receive_unread(3.0, visual_blob, &[0; 8]);
    q.receive(3.0, fx(id, 1, 7, 0x7));
    assert!(
        drain(&mut q).is_empty(),
        "too short: stamp 6 is still missing"
    );

    let later = 0x8130_0000_0000_0008;
    q.receive_unread(4.0, visual_blob, &unread_visual(later, 1, 2));
    assert_eq!(q.waiting_objects(), 1, "it waits for its object");
    q.receive(4.0, fx(later, 1, 3, 0x33));
    q.receive(4.0, create_item(later, 1, 1));
    let got: Vec<u32> = drain(&mut q).iter().filter_map(effect_of).collect();
    assert_eq!(got, vec![0x33], "its stamp came with the creation");
    assert_eq!(q.waiting_objects(), 0);
}

/// A visual message of a type that reads but names no object of its own takes its object and stamps
/// from its head too, so it holds its place in that object's order.
///
/// Behaviour: model.events.a-visual-message-that-does-not-read-holds-nothing-back
#[test]
fn a_visual_message_of_any_type_keeps_its_place_by_its_head() {
    use auberean_protocol::quality::PrivateQuality;
    let id: u64 = 0x0000_0002_0000_0007;
    let mut q = EventQueue::new();
    q.receive(0.0, create_item(id, 1, 1));
    drain(&mut q);
    let value = (id >> 32) | (1 << 32) | (2 << 48);
    let quality = u32::try_from(id & 0xFFFF_FFFF).expect("its low half");
    let of_any_type = item(
        2,
        0,
        ServerMessage::UpdateInstanceId(PrivateQuality { quality, value }),
    );
    q.receive(1.0, fx(id, 1, 3, 0x3));
    assert!(drain(&mut q).is_empty(), "stamp 2 hasn't come");
    q.receive(1.0, of_any_type);
    let got = drain(&mut q);
    assert!(matches!(got[0].message, ServerMessage::UpdateInstanceId(_)));
    assert_eq!(effect_of(&got[1]), Some(0x3));
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
                    parent: stamp(HOLDER, 1, 1),
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

/// Two stamps are ordered round the circle: the larger is newer while they are less than half the
/// circle apart and the smaller from there on. At exactly half, the smaller is newer, so of two
/// stamps that far apart one is always newer and never both.
///
/// Behaviour: model.events.stamps-exactly-half-the-circle-apart-favor-the-smaller
#[test]
fn stamps_exactly_half_the_circle_apart_favor_the_smaller() {
    assert!(newer(0, 0x8000));
    assert!(!newer(0x8000, 0));
    assert!(newer(0x1234, 0x9234));
    assert!(!newer(0x9234, 0x1234));
    // One step either side of half.
    assert!(newer(0x7FFF, 0));
    assert!(!newer(0, 0x7FFF));
    assert!(newer(0, 0x8001));
    assert!(!newer(0x8001, 0));
    // Equal is never newer, and an ordinary step and the wrap read as they should.
    assert!(!newer(7, 7));
    assert!(newer(8, 7));
    assert!(newer(0, 0xFFFF));
    assert!(!newer(0xFFFF, 0));
    // Any two different stamps: exactly one of them is newer.
    for b in [0u16, 1, 0x7FFF, 0x8000, 0xFFFF] {
        for gap in 1..=0xFFFFu16 {
            let a = b.wrapping_add(gap);
            assert!(newer(a, b) != newer(b, a), "{a:#x} and {b:#x}");
        }
    }
}

/// A visual message exactly half the circle from what the queue wants next waits when its stamp is
/// the smaller of the two and is dropped when it is the larger.
///
/// Behaviour: model.events.stamps-exactly-half-the-circle-apart-favor-the-smaller
#[test]
fn a_visual_message_exactly_half_the_circle_ahead_waits_or_drops_by_the_smaller_stamp() {
    let id = 0x8130_0000_0000_0002;
    // The queue wants 0x8000, the message is 0: the smaller, so it is newer and it waits.
    let mut q = EventQueue::new();
    q.receive(0.0, create_item(id, 1, 0x7FFF));
    drain(&mut q);
    q.receive(0.1, fx(id, 1, 0, 0xA));
    assert!(drain(&mut q).is_empty());
    assert_eq!(q.stats().held_visual, 1, "it was counted as held");
    assert_eq!(q.stats().dropped, 0, "and it was not dropped");
    // The queue wants 1, the message is 0x8001: the larger, so it is older and it is dropped.
    let mut q = EventQueue::new();
    q.receive(0.0, create_item(id, 1, 0));
    drain(&mut q);
    q.receive(0.1, fx(id, 1, 0x8001, 0xC));
    assert_eq!(q.stats().dropped, 1);
    q.receive(0.2, fx(id, 1, 1, 0xD));
    assert_eq!(
        drain(&mut q)
            .iter()
            .filter_map(effect_of)
            .collect::<Vec<_>>(),
        vec![0xD]
    );
}

/// A creation exactly half the circle of incarnations away replaces the object when its stamp is
/// the smaller and is ignored when it is the larger; a message about that incarnation waits or is
/// dropped alike.
///
/// Behaviour: model.events.stamps-exactly-half-the-circle-apart-favor-the-smaller
#[test]
fn an_incarnation_exactly_half_the_circle_away_replaces_or_is_ignored_by_the_smaller_stamp() {
    let id = 0x8130_0000_0000_0002;
    let mut q = EventQueue::new();
    q.receive(0.0, create_item(id, 0x8000, 1));
    drain(&mut q);
    q.receive(0.1, fx(id, 0, 1, 0xA));
    assert!(drain(&mut q).is_empty());
    assert_eq!(q.waiting_objects(), 1, "about the newer one: it waits");
    q.receive(0.2, create_item(id, 0, 1));
    assert_eq!(q.stats().replaced, 1);

    let mut q = EventQueue::new();
    q.receive(0.0, create_item(id, 0, 1));
    drain(&mut q);
    q.receive(0.1, fx(id, 0x8000, 1, 0xA));
    assert_eq!(q.waiting_objects(), 0, "about an older one: dropped");
    q.receive(0.2, create_item(id, 0x8000, 1));
    assert!(drain(&mut q).is_empty(), "an older creation is ignored");
    assert_eq!(q.stats().replaced, 0);
}

/// Brings a queue to stamp `at`, which a creation can only reach in steps of under half the circle.
fn object_at_stamp(q: &mut EventQueue, id: u64, at: u16) {
    q.receive(0.0, create_item(id, 1, at.wrapping_sub(0x7FFF)));
    q.receive(0.0, create_item(id, 1, at));
    drain(q);
}

/// Stamps held across the wrap of the circle take effect round it: 1 waits for 0xFFFF and 0, and
/// 0xFFFF, the one wanted, is not held up behind it, so nothing is given up on.
///
/// Behaviour: model.events.visual-stamps-held-across-the-wrap-go-round-the-circle
#[test]
fn visual_stamps_held_across_the_wrap_take_effect_round_the_circle() {
    let id = 0x8130_0000_0000_0002;
    let mut q = EventQueue::new();
    object_at_stamp(&mut q, id, 0xFFFE);
    q.receive(0.1, fx(id, 1, 1, 0x1));
    assert!(drain(&mut q).is_empty(), "1 waits for the stamps before it");
    q.receive(0.2, fx(id, 1, 0xFFFF, 0xF));
    assert_eq!(
        drain(&mut q)
            .iter()
            .filter_map(effect_of)
            .collect::<Vec<_>>(),
        vec![0xF],
        "the wanted stamp goes, with 1 still held"
    );
    q.receive(0.3, fx(id, 1, 0, 0x0));
    assert_eq!(
        drain(&mut q)
            .iter()
            .filter_map(effect_of)
            .collect::<Vec<_>>(),
        vec![0x0, 0x1],
    );
    q.receive(30.0, fx(id, 1, 2, 0x2));
    assert_eq!(q.take_force_creates(), Vec::<u64>::new());
    assert_eq!(q.stats().dropped, 0);
}

/// Messages held under one stamp come off in the order the queue's heap leaves them, not the order
/// they came: the later of two goes and the earlier, by then older than the one wanted, is
/// dropped.
///
/// Behaviour: model.events.of-two-visual-messages-held-under-one-stamp-the-heap-picks
#[test]
fn of_two_visual_messages_held_under_one_stamp_the_heap_picks_the_later() {
    let id = 0x8130_0000_0000_0002;
    let mut q = EventQueue::new();
    q.receive(0.0, create_item(id, 1, 0));
    drain(&mut q);
    q.receive(0.1, fx(id, 1, 3, 0xA));
    q.receive(0.1, fx(id, 1, 3, 0xB));
    q.receive(0.1, fx(id, 1, 2, 0xC));
    assert!(drain(&mut q).is_empty());
    q.receive(0.1, fx(id, 1, 1, 0xD));
    assert_eq!(
        drain(&mut q)
            .iter()
            .filter_map(effect_of)
            .collect::<Vec<_>>(),
        vec![0xD, 0xC, 0xB],
    );
    assert_eq!(q.stats().dropped, 1);
    // The sinking message goes to the right of two equal children: the later again.
    let mut q = EventQueue::new();
    q.receive(0.0, create_item(id, 1, 0));
    drain(&mut q);
    q.receive(0.1, fx(id, 1, 2, 0x2));
    q.receive(0.1, fx(id, 1, 3, 0xA));
    q.receive(0.1, fx(id, 1, 3, 0xB));
    q.receive(0.1, fx(id, 1, 4, 0x4));
    assert!(drain(&mut q).is_empty());
    q.receive(0.1, fx(id, 1, 1, 0x1));
    assert_eq!(
        drain(&mut q)
            .iter()
            .filter_map(effect_of)
            .collect::<Vec<_>>(),
        vec![0x1, 0x2, 0xB, 0x4],
    );
    assert_eq!(q.stats().dropped, 1);
}
