//! Putting an item of the pack down on the ground, and picking an item up from it: into the pack,
//! into a stack the pack holds, or refused (no room, no way there); the time a drop takes by body.

use auberean_protocol::{queue, ServerMessage};
use yalain_testkit::play::server_call;
use yalain_testkit::{Harness, STEP, TEST_ITEM};
use yalain_world::calls::{self, Node, Value};
use yalain_world::drops::{self, look, status, DROP};
use yalain_world::inventory::{move_graph, Move};

const MOVE_DONE: u32 = 0x820E_005E;
const CONTAINER: u32 = 258;

fn scene_drawn(h: &mut Harness, b: usize) {
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
}

/// The move descriptions the server answered with since `from`, by their slots.
fn answers(h: &Harness, b: usize, from: usize) -> Vec<Vec<(u8, u32)>> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c) if c.function.0 == MOVE_DONE => c
                .args
                .as_ref()
                .and_then(|a| calls::values(&a.0).ok())
                .and_then(|v| {
                    v.iter().find_map(|x: &Value| match x.root() {
                        Some(Node::Script { slots, .. }) => Some(slots.clone()),
                        _ => None,
                    })
                }),
            _ => None,
        })
        .collect()
}

/// How many of the harness's steps make `seconds`.
fn steps(seconds: f64) -> usize {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let n = (seconds / STEP).round() as usize;
    n
}

/// The client's request to put `item` down from slot `slot` of `me`'s pack, as the 2017 clients
/// sent it.
fn put_down(me: u64, item: u64, slot: i32) -> auberean_protocol::ClientMessage {
    let m = Move {
        move_type: DROP,
        merge: true,
        unlock: true,
        check_take: true,
        all_taken: true,
        grab: true,
        contain: true,
        auto_merge: true,
        from: me,
        actual_from: me,
        item,
        split_item: item,
        quantity: 1,
        quantity_left: 1,
        from_slot: slot,
        actual_from_slot: slot,
        ..Move::default()
    };
    server_call(drops::function::MOVE_ITEM, &[move_graph(&m)])
}

/// A player's pack holding one test item, and another player beside it; returns the harness, the
/// dropper's client, the dropper, the watcher's client and the item.
fn two_players_and_an_item() -> (Harness, usize, u64, usize, u64) {
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    let (b, me) = h.player_in_world("one", "Elara");
    scene_drawn(&mut h, b);
    let (w, _) = h.player_in_world("two", "Bran");
    scene_drawn(&mut h, w);
    let world = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .map(|(c, _)| c)
        .expect("the dropper in the world");
    yalain_world::inventory::give(&mut h.server, &mut h.net, world, TEST_ITEM, 1);
    h.run(5);
    let item = h.server.players[&world].play.items[0].id;
    (h, b, me, w, item)
}

/// Asked to put an item down, the server has the player play its putting-down behavior (seen by
/// those who see the player) with the item's look in its hand and clears the item's container at
/// once; when the behavior is through, 1.75 s later for a human man, the move is answered with the
/// animation played and the item stands straight ahead of the player at its reach (0.785 m for a
/// human man of height 1), half a meter up, facing as it faces: its place sent to the dropper, the
/// item made there for the player beside it. A second drop while one is under way is refused at
/// once, its actions locked.
///
/// Behaviour: server.inventory.an-item-put-down-stands-ahead-of-the-player
#[test]
fn an_item_put_down_stands_ahead_of_the_player() {
    let (mut h, b, me, w, item) = two_players_and_an_item();
    let world = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .map(|(c, _)| c)
        .expect("in the world");
    let at = h.server.players[&world].position;
    let (from, watched) = (h.bots[b].inbox.len(), h.bots[w].inbox.len());
    h.bots[b].send(true, queue::WEENIE, &put_down(me, item, 0));
    h.run(4);
    let started = h.world_messages(b, from);
    assert!(
        started.iter().any(|m| matches!(m,
            ServerMessage::PlayBehavior(e) if e.sender.id == me
                && e.action.behavior == look::PUTTING_DOWN
                && e.action.fx_script == look::PUT_DOWN_SCRIPT
                && e.action.target == item)),
        "the putting-down behavior"
    );
    assert!(
        h.world_messages(w, watched).iter().any(|m| matches!(m,
            ServerMessage::PlayBehavior(e) if e.action.behavior == look::PUTTING_DOWN)),
        "seen by the player beside it"
    );
    assert!(
        started.iter().any(|m| matches!(m,
            ServerMessage::UpdateInstanceIdVisual(q) if q.object.id == item
                && q.quality == CONTAINER && q.value == 0)),
        "the container cleared"
    );
    assert!(answers(&h, b, from).is_empty(), "not answered yet");
    assert!(
        h.server.players[&world].play.items.is_empty(),
        "out of the pack"
    );

    // A second drop while the first is under way.
    let again = h.bots[b].inbox.len();
    h.bots[b].send(true, queue::WEENIE, &put_down(me, item, 0));
    h.run(4);
    let refused = answers(&h, b, again);
    assert_eq!(refused.len(), 1, "answered at once");
    assert_eq!(refused[0][40].1, status::ACTIONS_LOCKED, "actions locked");

    // 0.4 s gone: on to 1.6 s, then past 1.75 s.
    h.run(steps(1.2));
    assert_eq!(
        answers(&h, b, from).len(),
        1,
        "only the refusal before 1.75 s"
    );
    h.run(steps(0.4));
    let done = answers(&h, b, from);
    assert_eq!(done.len(), 2, "answered when the behavior is through");
    let done = &done[1];
    assert_eq!(done[12].1, DROP);
    assert_eq!(done[40].1, 0, "not refused");
    assert_eq!(done[41].1, 1, "the animation played");
    let place = h
        .world_messages(b, from)
        .iter()
        .find_map(|m| match m {
            ServerMessage::Position(p) if p.sender.id == item => Some(*p),
            _ => None,
        })
        .expect("the item's place");
    let heading = yalain_world::world::heading_of_rotation(at.rotation);
    let h_rad = (f32::from(heading) * 360.0 / 255.0).to_radians();
    let want = (
        at.origin[0] + 0.785 * dereth_primitives::num::math::sinf(h_rad),
        at.origin[1] + 0.785 * dereth_primitives::num::math::cosf(h_rad),
    );
    assert!(
        (place.origin.x - want.0).abs() < 1e-3 && (place.origin.y - want.1).abs() < 1e-3,
        "0.785 m straight ahead: {:?}, want {want:?}",
        place.origin
    );
    assert!(
        (place.origin.z - (at.origin[2] + 0.5)).abs() < 1e-4,
        "half a meter up"
    );
    assert_eq!(
        place.heading_motion >> 24,
        u32::from(heading),
        "facing as the player faces"
    );
    assert_eq!(place.cell, at.cell);
    assert!(
        h.world_messages(w, watched).iter().any(|m| matches!(m,
            ServerMessage::CreateObject(o) if o.id == item
                && o.physics.position.cell == at.cell)),
        "made where it stands for the player beside it"
    );
    h.run(steps(0.4));
    let rested = h
        .world_messages(b, from)
        .iter()
        .rev()
        .find_map(|m| match m {
            ServerMessage::Position(p) if p.sender.id == item => Some(*p),
            _ => None,
        })
        .expect("the item's places");
    assert_eq!(
        rested.position_stamp,
        place.position_stamp.wrapping_add(1),
        "a second place"
    );
    assert!(
        (rested.origin.z - (at.origin[2] + 0.05)).abs() < 1e-4,
        "at rest just above the feet: {}",
        rested.origin.z
    );
}

/// The use of an item lying in the world, as the client asks for it: the item and the player.
fn use_item(item: u64, me: u64) -> auberean_protocol::ClientMessage {
    server_call(
        yalain_world::requests::function::USE,
        &[yalain_testkit::play::use_of(item, me)],
    )
}

/// The movement orders a client was sent for a player since `from`: (flags, desired distance,
/// target).
fn walks(h: &Harness, b: usize, from: usize, me: u64) -> Vec<(u32, f32, u64)> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::MoveTo(e) if e.sender.id == me => {
                let p = &e.action.params;
                let target = match p.target {
                    auberean_protocol::types::MoveTarget::Object(o) => o,
                    _ => 0,
                };
                Some((p.flags, p.desired_distance, target))
            }
            _ => None,
        })
        .collect()
}

/// An item lying in the world is picked up by using it: the player is ordered to walk to it (to
/// 2 m, the order sent even when it is there already) and, there, stops and plays its picking-up
/// behavior with the item's look in its hand; when the behavior is through (1.79 s for a human
/// man) the item is contained in the pack, the move answered (picked up, the animation played),
/// its holder told the player beside it, and it leaves the world. An item farther off is walked to
/// first and picked up on arrival.
///
/// Behaviour: server.inventory.an-item-lying-in-the-world-is-walked-to-and-picked-up
#[test]
fn an_item_lying_in_the_world_is_walked_to_and_picked_up() {
    let (mut h, b, me, w, item) = two_players_and_an_item();
    let world = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .map(|(c, _)| c)
        .expect("in the world");
    h.bots[b].send(true, queue::WEENIE, &put_down(me, item, 0));
    h.run(steps(2.5));
    assert!(h.server.players[&world].play.items.is_empty(), "put down");

    let (from, watched) = (h.bots[b].inbox.len(), h.bots[w].inbox.len());
    h.bots[b].send(true, queue::WEENIE, &use_item(item, me));
    h.run(4);
    let orders = walks(&h, b, from, me);
    assert_eq!(orders.first(), Some(&(0x61, 2.0, item)), "walked to it");
    assert_eq!(orders.get(1).map(|o| o.0), Some(0x40), "and stopped there");
    let behaves = h.world_messages(b, from).iter().any(|m| {
        matches!(m, ServerMessage::PlayBehavior(e) if e.sender.id == me
            && e.action.behavior == 0x4100_0020
            && e.action.fx_script == 0x5700_0008
            && e.action.target == item)
    });
    assert!(behaves, "the picking-up behavior");
    assert!(answers(&h, b, from).is_empty(), "not yet");
    h.run(steps(1.9));
    let done = answers(&h, b, from);
    assert_eq!(done.len(), 1, "answered when the behavior is through");
    assert_eq!(done[0][12].1, 12, "picked up");
    assert_eq!(done[0][41].1, 1, "the animation played");
    let msgs = h.world_messages(b, from);
    let at = |pred: &dyn Fn(&ServerMessage) -> bool| msgs.iter().position(pred);
    let contained = at(&|m| matches!(m, ServerMessage::Contain(o) if o.0.id == item));
    let left = at(&|m| matches!(m, ServerMessage::ObjectLeaves(e) if e.sender.id == item));
    assert!(
        contained.is_some() && contained < left,
        "contained, then out of the world"
    );
    assert!(
        h.world_messages(w, watched).iter().any(|m| matches!(m,
            ServerMessage::UpdateInstanceIdVisual(q) if q.object.id == item
                && q.quality == CONTAINER && q.value == me)),
        "its holder told the player beside it"
    );
    assert_eq!(
        h.server.players[&world]
            .play
            .items
            .iter()
            .map(|i| i.id)
            .collect::<Vec<_>>(),
        [item],
        "back in the pack"
    );

    // Put down again, then 10 m away: walked to, picked up only on arrival.
    h.bots[b].send(true, queue::WEENIE, &put_down(me, item, 0));
    h.run(steps(2.5));
    if let Some(t) = h.server.things.get_mut(item) {
        if let Some(at) = t.at.as_mut() {
            at.origin[0] += 10.0;
        }
    }
    let from = h.bots[b].inbox.len();
    h.bots[b].send(true, queue::WEENIE, &use_item(item, me));
    h.run(steps(2.5));
    assert_eq!(walks(&h, b, from, me).len(), 1, "walking, not there");
    assert!(answers(&h, b, from).is_empty());
    let there = h.server.things.get(item).and_then(|t| t.at).expect("lying");
    if let Some(p) = h.server.players.get_mut(&world) {
        p.position.origin = there.origin;
    }
    let clock = h.clock();
    yalain_world::places::arrived(&mut h.server, &mut h.net, world, clock);
    h.run(steps(2.0));
    assert_eq!(answers(&h, b, from).len(), 1, "picked up on arrival");
}

/// An item picked up goes to the front of the pack, the items there each moving up one, and the
/// move names slot 0 as both where it was sent and where it went.
///
/// Behaviour: server.inventory.a-picked-up-item-goes-to-the-packs-front
#[test]
fn a_picked_up_item_goes_to_the_front_of_the_pack() {
    let (mut h, b, me, _, item) = two_players_and_an_item();
    let world = world_of(&h, me);
    yalain_world::inventory::give(&mut h.server, &mut h.net, world, TEST_ITEM, 1);
    h.run(5);
    let slot_of = |h: &Harness, id: u64| {
        h.server.players[&world]
            .play
            .items
            .iter()
            .find(|i| i.id == id)
            .map(|i| i.slot)
    };
    let other = h.server.players[&world]
        .play
        .items
        .iter()
        .map(|i| i.id)
        .find(|id| *id != item)
        .expect("a second item");
    let at = slot_of(&h, item).expect("held");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &put_down(me, item, i32::try_from(at).unwrap()),
    );
    h.run(steps(2.5));
    assert_eq!(
        slot_of(&h, other),
        Some(0),
        "the other item alone at the front"
    );
    let from = h.bots[b].inbox.len();
    h.bots[b].send(true, queue::WEENIE, &use_item(item, me));
    h.run(4);
    h.run(steps(1.9));
    let done = answers(&h, b, from);
    assert_eq!(done.len(), 1, "picked up");
    assert_eq!(done[0][12].1, 12, "a pickup");
    assert_eq!(done[0][11].1, 0, "sent to slot 0");
    assert_eq!(done[0][24].1, 0, "went to slot 0");
    assert_eq!(slot_of(&h, item), Some(0), "at the front");
    assert_eq!(slot_of(&h, other), Some(1), "the other moved up one");
}

/// A template that stacks (up to a thousand to a stack), made up for the tests.
const STACKING: u32 = 0x4700_2109;

/// The use answers (their statuses) a client was sent since `from`.
fn use_statuses(h: &Harness, b: usize, from: usize) -> Vec<u32> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c)
                if c.function.0 == yalain_world::places::function::USE_DONE =>
            {
                let v = calls::values(&c.args.as_ref()?.0).ok()?;
                let Value::Graph(g) = v.first()? else {
                    return None;
                };
                g.iter().find_map(|(_, n)| match n {
                    Node::Script {
                        package: 0x245,
                        slots,
                    } => slots.get(11).map(|(_, s)| *s),
                    _ => None,
                })
            }
            _ => None,
        })
        .collect()
}

/// The player's world connection.
fn world_of(h: &Harness, me: u64) -> yalain_net::ConnId {
    *h.server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .map(|(c, _)| c)
        .expect("in the world")
}

/// A picked up item with other numbers than the stack the pack holds stays an item of its own.
/// Behaviour: server.inventory.stacks-join-only-with-like-numbers
#[test]
fn a_picked_up_item_with_other_numbers_stays_its_own() {
    let (mut h, b, me, _, _) = two_players_and_an_item();
    let mut props = yalain_content::Props::default();
    props.set("Item_MaxStackSize", yalain_content::Prop::Int(1000));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(STACKING, 0x200, props));
    let world = world_of(&h, me);
    h.server.players.get_mut(&world).unwrap().play.items.clear();
    yalain_world::inventory::give(&mut h.server, &mut h.net, world, STACKING, 1);
    yalain_world::inventory::give(&mut h.server, &mut h.net, world, STACKING, 4);
    h.run(5);
    let (one, stack) = {
        let items = &h.server.players[&world].play.items;
        (items[0].id, items[1].id)
    };
    h.server.things.get_mut(one).expect("made").value = 500;
    h.bots[b].send(true, queue::WEENIE, &put_down(me, one, 0));
    h.run(steps(2.5));
    h.bots[b].send(true, queue::WEENIE, &use_item(one, me));
    h.run(4);
    h.run(steps(1.9));
    let mut held: Vec<(u64, u32)> = h.server.players[&world]
        .play
        .items
        .iter()
        .map(|x| (x.id, x.quantity.max(1)))
        .collect();
    held.sort_unstable();
    let mut want = [(one, 1), (stack, 4)];
    want.sort_unstable();
    assert_eq!(held, want);
    assert_eq!(h.server.things.get(one).expect("kept").value, 500);
}

/// An item of a template the player holds a stack of, picked up, joins the stack: the move
/// answered as merged (no item split off, no slot to merge at), the stack's new quantity, the
/// item destroyed and out of the world for every client that knew it; the pack still holds the
/// one stack. Another use while the pickup runs is refused at once, actions locked.
///
/// Behaviour: server.inventory.a-picked-up-item-joins-the-stack-it-matches
#[test]
fn a_picked_up_item_joins_the_stack_the_pack_holds_of_it() {
    let (mut h, b, me, w, _) = two_players_and_an_item();
    let mut props = yalain_content::Props::default();
    props.set("Item_MaxStackSize", yalain_content::Prop::Int(1000));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(STACKING, 0x200, props));
    let world = world_of(&h, me);
    h.server.players.get_mut(&world).unwrap().play.items.clear();
    yalain_world::inventory::give(&mut h.server, &mut h.net, world, STACKING, 1);
    yalain_world::inventory::give(&mut h.server, &mut h.net, world, STACKING, 4);
    h.run(5);
    let (one, stack) = {
        let items = &h.server.players[&world].play.items;
        (items[0].id, items[1].id)
    };
    h.bots[b].send(true, queue::WEENIE, &put_down(me, one, 0));
    h.run(steps(2.5));
    let (from, watched) = (h.bots[b].inbox.len(), h.bots[w].inbox.len());
    h.bots[b].send(true, queue::WEENIE, &use_item(one, me));
    h.run(4);
    let again = h.bots[b].inbox.len();
    h.bots[b].send(true, queue::WEENIE, &use_item(one, me));
    h.run(4);
    assert_eq!(use_statuses(&h, b, again), [1029], "busy: actions locked");
    h.run(steps(1.9));
    let done = answers(&h, b, from);
    assert_eq!(done.len(), 1, "answered once");
    let done = &done[0];
    assert_eq!(done[12].1, 12, "picked up");
    assert_eq!(done[40].1, 0, "not refused");
    assert_eq!(done[0].1, 1, "merged");
    assert_eq!(done[50].1, u32::MAX, "no slot to merge at");
    assert_eq!((done[28].1, done[29].1), (0, 0), "nothing split off");
    let msgs = h.world_messages(b, from);
    let at = |pred: &dyn Fn(&ServerMessage) -> bool| msgs.iter().position(pred);
    let moved = at(&|m| matches!(m, ServerMessage::PlayerCall(c) if c.function.0 == MOVE_DONE));
    let more = at(&|m| {
        matches!(m, ServerMessage::UpdateIntVisual(q)
            if q.object.id == stack && q.quality == 262 && q.value == 5)
    });
    let gone = at(&|m| matches!(m, ServerMessage::ForgetObject(o) if o.0.id == one));
    let left = at(&|m| matches!(m, ServerMessage::ObjectLeaves(e) if e.sender.id == one));
    assert!(
        moved.is_some() && moved < more && more < gone && gone < left,
        "answered, the stack grown, the item destroyed, out of the world: {moved:?} {more:?} {gone:?} {left:?}"
    );
    assert!(
        !msgs
            .iter()
            .any(|m| matches!(m, ServerMessage::Contain(o) if o.0.id == one)),
        "never contained"
    );
    assert!(
        h.world_messages(w, watched)
            .iter()
            .any(|m| matches!(m, ServerMessage::ForgetObject(o) if o.0.id == one)),
        "destroyed for the player beside it"
    );
    let items: Vec<(u64, u32)> = h.server.players[&world]
        .play
        .items
        .iter()
        .map(|i| (i.id, i.quantity))
        .collect();
    assert_eq!(items, [(stack, 5)], "one stack of five");
    assert!(h.server.things.get(one).is_none(), "the item is gone");
}

/// An item picked up into a pack with no room left is refused when the picking-up behavior is
/// through: the move answered with the pack full (the animation played), and the player puts it
/// back down; it stays where it lay, the pack unchanged.
///
/// Behaviour: server.inventory.a-pickup-the-pack-has-no-room-for-is-put-back
#[test]
fn a_pickup_into_a_full_pack_is_refused_and_the_item_put_back_down() {
    let (mut h, b, me, _w, item) = two_players_and_an_item();
    let world = world_of(&h, me);
    h.bots[b].send(true, queue::WEENIE, &put_down(me, item, 0));
    h.run(steps(2.5));
    let lay = h.server.things.get(item).and_then(|t| t.at).expect("lying");
    let full: Vec<yalain_world::state::Held> = (0..yalain_world::inventory::PACK_ROOM)
        .map(|n| yalain_world::state::Held {
            id: 0x8130_0000_0FFF_0000 + u64::from(n),
            template: TEST_ITEM,
            slot: n,
            location: 0,
            quantity: 1,
        })
        .collect();
    h.server.players.get_mut(&world).unwrap().play.items = full.clone();
    let from = h.bots[b].inbox.len();
    h.bots[b].send(true, queue::WEENIE, &use_item(item, me));
    h.run(4);
    h.run(steps(1.9));
    let done = answers(&h, b, from);
    assert_eq!(done.len(), 1, "answered");
    assert_eq!(
        done[0][40].1,
        yalain_world::inventory::refused::CONTAINER_FULL,
        "the pack is full"
    );
    assert_eq!(done[0][41].1, 1, "the animation played");
    let msgs = h.world_messages(b, from);
    let answered = msgs
        .iter()
        .position(|m| matches!(m, ServerMessage::PlayerCall(c) if c.function.0 == MOVE_DONE));
    let put_back = msgs.iter().position(|m| {
        matches!(m, ServerMessage::PlayBehavior(e) if e.sender.id == me
            && e.action.behavior == look::PUTTING_DOWN
            && e.action.target == item)
    });
    assert!(
        answered.is_some() && answered < put_back,
        "then put back down"
    );
    let t = h.server.things.get(item).expect("still there");
    assert!(t.container.is_none() && t.at == Some(lay), "where it lay");
    assert_eq!(
        h.server.players[&world].play.items, full,
        "the pack unchanged"
    );
}

/// Walked toward an item to pick it up, a player that reports itself at rest twice at the same
/// place, out of reach, could not get there: the move is answered as such (no animation played)
/// and the walk stopped; the player is free to act again.
///
/// Behaviour: server.inventory.a-pickup-the-player-cannot-get-to-is-refused
#[test]
fn a_pickup_the_player_cannot_get_to_is_refused_and_the_walk_stopped() {
    let (mut h, b, me, _w, item) = two_players_and_an_item();
    let world = world_of(&h, me);
    h.bots[b].send(true, queue::WEENIE, &put_down(me, item, 0));
    h.run(steps(2.5));
    if let Some(at) = h.server.things.get_mut(item).and_then(|t| t.at.as_mut()) {
        at.origin[0] += 20.0;
    }
    let from = h.bots[b].inbox.len();
    h.bots[b].send(true, queue::WEENIE, &use_item(item, me));
    h.run(4);
    let at = h.server.players[&world].position;
    let still = |x: f32, heading: u8| {
        yalain_testkit::play::report(
            at.cell,
            auberean_primitives::Vec3::new(x, at.origin[1], at.origin[2]),
            heading,
            yalain_world::world::IDLE_MOTION & 0x00FF_FFFF,
            0,
        )
    };
    // Walking a little, then standing: one report at rest is not enough.
    h.bots[b].send(true, queue::WEENIE, &still(at.origin[0] + 1.0, 10));
    h.run(4);
    assert!(answers(&h, b, from).is_empty(), "still walking");
    // At rest again where it stood, turned only.
    h.bots[b].send(true, queue::WEENIE, &still(at.origin[0] + 1.0, 40));
    h.run(4);
    let done = answers(&h, b, from);
    assert_eq!(done.len(), 1, "answered");
    assert_eq!(
        done[0][40].1,
        yalain_world::inventory::refused::CANT_GET_THERE,
        "could not get there"
    );
    assert_eq!(done[0][41].1, 0, "no animation played");
    assert_eq!(
        walks(&h, b, from, me)
            .iter()
            .map(|w| w.0)
            .collect::<Vec<_>>(),
        [0x61, 0x40],
        "the walk, then stopped"
    );
    let t = h.server.things.get(item).expect("still there");
    assert!(t.container.is_none() && t.at.is_some(), "where it lay");
    assert!(
        !yalain_world::places::actions_locked(&h.server, world),
        "free to act again"
    );
}

/// How long putting an item down takes depends on the body: a Drudge's drop is answered 3.57 s
/// after its behavior began, well after a human's 1.75 s.
///
/// Behaviour: server.inventory.putting-down-takes-the-bodys-time
#[test]
fn a_drudges_drop_takes_its_bodys_longer_time() {
    assert!((drops::putting_down_seconds(1, 0x1000) - 1.75).abs() < 1e-9);
    assert!((drops::putting_down_seconds(16, 0x2000) - 1.56).abs() < 1e-9);
    let (mut h, b, me, _w, item) = two_players_and_an_item();
    let world = world_of(&h, me);
    h.server.players.get_mut(&world).unwrap().race = 8;
    let from = h.bots[b].inbox.len();
    h.bots[b].send(true, queue::WEENIE, &put_down(me, item, 0));
    h.run(steps(3.3));
    assert!(answers(&h, b, from).is_empty(), "not at 3.3 s");
    h.run(steps(0.7));
    assert_eq!(answers(&h, b, from).len(), 1, "answered by 4 s");
}

/// A quest item of the test world with a value: one a camp keeps lying.
const LYING: u32 = 0x4700_1B8A;

/// The ids of the items of the lying template made for a client since `from`, with where.
fn lying_made(h: &Harness, b: usize, from: usize) -> Vec<(u64, u32, [f32; 3])> {
    h.world_messages(b, from)
        .into_iter()
        .filter_map(|m| match m {
            ServerMessage::CreateObject(o) if o.weenie.entity_did == LYING => {
                let p = &o.physics.position;
                let at = &p.frame.origin;
                Some((o.id, p.cell, [at.x, at.y, at.z]))
            }
            _ => None,
        })
        .collect()
}

/// A camp whose template is an item keeps it lying at its spot: the item is made for the player
/// in view there; once picked up, nothing lies there until the camp's period is over, and then a
/// new one (an object of its own) lies at the same spot.
///
/// Behaviour: server.inventory.an-item-a-camp-keeps-lies-again-after-its-taking
#[test]
fn an_item_a_camp_keeps_lies_again_at_its_spot_once_its_period_after_the_taking_is_over() {
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    let mut props = yalain_content::Props::default();
    props.set("Item_Value", yalain_content::Prop::Int(5));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(LYING, 0x200, props));
    let (b, me) = h.player_in_world("one", "Elara");
    scene_drawn(&mut h, b);
    let world = world_of(&h, me);
    let stand = h.server.players[&world].position;
    let spot = [stand.origin[0] + 1.0, stand.origin[1], stand.origin[2]];
    let mut camp = yalain_testkit::test_camp(1);
    camp.id = "lying".into();
    camp.cell = stand.cell;
    camp.origin = spot;
    camp.regen = 20.0;
    camp.templates = vec![(LYING, 1)];
    camp.spots = vec![(stand.cell, spot)];
    h.server.spawns.camps.push(camp);
    h.server
        .camps
        .push(yalain_world::spawn::CampState::default());
    let from = h.bots[b].inbox.len();
    h.run(5);
    let made = lying_made(&h, b, from);
    assert_eq!(made.len(), 1, "the item made for the player in view");
    let (item, cell, at) = made[0];
    assert_eq!((cell, at), (stand.cell, spot), "at the camp's spot");

    h.bots[b].send(true, queue::WEENIE, &use_item(item, me));
    h.run(steps(2.5));
    assert_eq!(
        h.server.players[&world]
            .play
            .items
            .iter()
            .map(|i| i.id)
            .collect::<Vec<_>>(),
        [item],
        "picked up"
    );
    let from = h.bots[b].inbox.len();
    h.run(steps(19.0));
    assert!(
        lying_made(&h, b, from).is_empty(),
        "none before the period is over"
    );
    h.run(steps(1.5));
    let again = lying_made(&h, b, from);
    assert_eq!(again.len(), 1, "a new one once it is over");
    assert_ne!(again[0].0, item, "an object of its own");
    assert_eq!(
        (again[0].1, again[0].2),
        (stand.cell, spot),
        "at the same spot"
    );
}

/// Run a world's work on a thread of its own: `true` when it ends.
fn ends(work: impl FnOnce() + Send + 'static) -> bool {
    let (done, ended) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        work();
        let _ = done.send(());
    });
    ended
        .recv_timeout(std::time::Duration::from_secs(30))
        .is_ok()
}

/// Stepping a point onto its landblock, a block at a time: what any point on the land comes to.
fn stepped(landblock: u32, x: f32, y: f32) -> (u32, f32, f32) {
    let side = yalain_world::spawn::BLOCK_SIDE;
    let (mut bx, mut by) = (
        i64::from(landblock >> 8 & 0xFF),
        i64::from(landblock & 0xFF),
    );
    let (mut x, mut y) = (x, y);
    while x < 0.0 {
        x += side;
        bx -= 1;
    }
    while x >= side {
        x -= side;
        bx += 1;
    }
    while y < 0.0 {
        y += side;
        by -= 1;
    }
    while y >= side {
        y -= side;
        by += 1;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (cx, cy) = ((x / 20.0) as u32, (y / 20.0) as u32);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let block = ((bx.clamp(0, 255) as u32) << 8) | by.clamp(0, 255) as u32;
    (block << 16 | (cx.min(7) * 8 + cy.min(7) + 1), x, y)
}

/// Any point within the land's side of its block is placed as stepping it there block by block
/// places it: across the whole span, and on either side of every block's edge.
/// Behaviour: server.places.a-place-off-the-land-is-no-place
#[test]
fn a_point_on_the_land_is_placed_as_stepping_it_block_by_block_places_it() {
    let side = yalain_world::spawn::BLOCK_SIDE;
    let land = yalain_world::spawn::LAND_SIDE;
    let mut points = Vec::new();
    let mut x = -land;
    while x <= land {
        points.push(x);
        x += 1.37;
    }
    let mut edge = -land;
    while edge <= land {
        points.extend([edge.next_down(), edge, edge.next_up()]);
        edge += side;
    }
    points.push(land);
    points.retain(|x| x.abs() <= land);
    for block in [0u32, 0x0299, 0x7F7F, 0xFFFF] {
        for (i, x) in points.iter().enumerate() {
            let y = points[(i * 7) % points.len()];
            let got = yalain_world::spawn::outdoor(block, *x, y);
            let want = stepped(block, *x, y);
            assert_eq!(
                (got.0, got.1.to_bits(), got.2.to_bits()),
                (want.0, want.1.to_bits(), want.2.to_bits()),
                "{block:04X} {x} {y}"
            );
        }
    }
}

/// A point no land could hold still comes to a place on it, whatever its numbers.
/// Behaviour: server.places.a-place-off-the-land-is-no-place
#[test]
fn a_point_far_off_the_land_comes_to_a_place_at_its_edge() {
    assert!(ends(|| {
        for x in [1.0e30, -1.0e30, f32::MAX, f32::MIN, 1.0e10, -1.0e10] {
            let (cell, px, py) = yalain_world::spawn::outdoor(0x0299, x, 5.0);
            assert!(!yalain_world::presence::indoors(cell), "{cell:08X}");
            assert!(
                (0.0..=yalain_world::spawn::BLOCK_SIDE).contains(&px),
                "{px}"
            );
            assert!(
                (0.0..=yalain_world::spawn::BLOCK_SIDE).contains(&py),
                "{py}"
            );
        }
        for x in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let _ = yalain_world::spawn::outdoor(0x0299, x, x);
        }
    }));
}

/// A place reported beyond the land's side from its block is not taken: the player stays where it
/// was. Further than any of the 1,827,746 reports recorded (at most 720 m, indoors).
/// Behaviour: server.places.a-place-off-the-land-is-no-place
#[test]
fn a_place_reported_far_off_the_land_is_not_taken() {
    let mut h = Harness::new();
    let (b, me) = h.player_in_world("one", "Far");
    let world = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .map(|(c, _)| c)
        .expect("in the world");
    let stamp = h.server.players[&world].teleport_stamp;
    let report = |x: f32| {
        yalain_testkit::play::report(
            0x0299_0009,
            auberean_primitives::Vec3::new(x, 10.0, 10.0),
            0,
            0x4100_0003,
            stamp,
        )
    };
    h.bots[b].send(true, queue::WEENIE, &report(100.0));
    h.run(3);
    assert_eq!(h.server.players[&world].position.origin[0], 100.0, "on it");
    for x in [
        yalain_world::spawn::LAND_SIDE.next_up(),
        -yalain_world::spawn::LAND_SIDE.next_up(),
        1.0e10,
        1.0e30,
        f32::MAX,
    ] {
        h.bots[b].send(true, queue::WEENIE, &report(x));
        h.run(3);
        assert_eq!(h.server.players[&world].position.origin[0], 100.0, "{x}");
    }
    h.bots[b].send(true, queue::WEENIE, &report(719.8));
    h.run(3);
    assert_eq!(
        h.server.players[&world].position.origin[0], 719.8,
        "the furthest recorded"
    );
}

/// An item put down by a player kept far off the land (from before such a place was refused) ends
/// on the land's edge.
/// Behaviour: server.places.a-place-off-the-land-is-no-place
#[test]
fn an_item_put_down_from_far_off_the_land_lands_on_its_edge() {
    assert!(ends(|| {
        let (mut h, b, me, _, item) = two_players_and_an_item();
        let world = *h
            .server
            .players
            .iter()
            .find(|(_, p)| p.character == me)
            .map(|(c, _)| c)
            .expect("in the world");
        let p = h.server.players.get_mut(&world).expect("in the world");
        p.position.cell = 0x0299_0009;
        p.position.origin = [1.0e30, 10.0, 10.0];
        h.bots[b].send(true, queue::WEENIE, &put_down(me, item, 0));
        h.run(steps(5.0));
        assert!(h.server.players[&world]
            .play
            .items
            .iter()
            .all(|i| i.id != item));
    }));
}

/// An item picked up moves the quest its template names on being acquired.
/// Behaviour: server.quests.an-item-acquired-moves-its-quest
#[test]
fn an_item_picked_up_moves_the_quest_its_template_names() {
    const QUEST: u32 = 0x4000_0123;
    let (mut h, b, me, _w, item) = two_players_and_an_item();
    let world = world_of(&h, me);
    let template = h.server.things.get(item).expect("the item").template;
    let mut t = h
        .server
        .catalog
        .get(template)
        .cloned()
        .expect("its template");
    t.props.set(
        "Item_AcquireEffect1",
        yalain_content::Prop::Text("Eff_AdvanceQuestHuntPhase2".into()),
    );
    h.server.catalog.insert(t);
    h.server.quest_ids.insert("QuestHunt".into(), QUEST);
    h.server.quests.quests.push(super::kill_quests::quest(3));
    h.server
        .players
        .get_mut(&world)
        .expect("there")
        .play
        .quests
        .push(yalain_world::state::QuestState {
            quest: QUEST,
            status: yalain_world::quests::status::UNDERWAY,
            phase: 1,
            started: 0,
            updated: 0,
        });
    h.bots[b].send(true, queue::WEENIE, &put_down(me, item, 0));
    h.run(steps(2.5));
    h.bots[b].send(true, queue::WEENIE, &use_item(item, me));
    h.run(4);
    h.run(steps(1.9));
    let p = &h.server.players[&world];
    assert!(p.play.items.iter().any(|i| i.id == item), "picked up");
    assert_eq!(yalain_world::quests::phase_of(&p.play.quests, QUEST), 2);
}

/// A template its script makes attunable counts as one that says so, with nothing saved to tell.
/// Behaviour: server.inventory.an-attunable-item-is-not-put-down
#[test]
fn a_template_attunable_by_its_script_says_so() {
    let catalog = yalain_content::Catalog::default();
    for template in [0x4700_11C8, 0x4700_187E, 0x4700_08CF, 0x4700_132D] {
        assert!(yalain_world::inventory::attunable(&catalog, template));
    }
    assert!(!yalain_world::inventory::attunable(&catalog, 0x4700_051C));
}

/// An attunable item stays with its owner: asked to put it down, the server refuses at once, plays
/// nothing, and the item stays in the pack. So its taking can't be repeated to move a quest again.
/// Behaviour: server.inventory.an-attunable-item-is-not-put-down
#[test]
fn an_attunable_item_is_refused_at_once_and_stays_in_the_pack() {
    let (mut h, b, me, _w, item) = two_players_and_an_item();
    let world = world_of(&h, me);
    let template = h.server.things.get(item).expect("the item").template;
    let mut t = h
        .server
        .catalog
        .get(template)
        .cloned()
        .expect("its template");
    t.props
        .set("Item_IsAttunable", yalain_content::Prop::Bool(true));
    h.server.catalog.insert(t);
    let from = h.bots[b].inbox.len();
    h.bots[b].send(true, queue::WEENIE, &put_down(me, item, 0));
    h.run(4);
    let refused = answers(&h, b, from);
    assert_eq!(refused.len(), 1, "answered at once");
    assert_eq!(refused[0][40].1, status::ATTUNED);
    assert!(
        !h.world_messages(b, from).iter().any(|m| matches!(m,
            ServerMessage::PlayBehavior(e) if e.action.behavior == look::PUTTING_DOWN)),
        "nothing played"
    );
    h.run(steps(2.5));
    let p = &h.server.players[&world];
    assert!(p.play.items.iter().any(|i| i.id == item), "still held");
    assert_eq!(answers(&h, b, from).len(), 1, "and nothing more");
}
