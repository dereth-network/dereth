//! The pack and its three bags: where an item comes in (at the front of the first segment with
//! room, every item there moving up one, the move naming that slot), moving items about the pack
//! and its bags (a plain move, a split, a merge), selling an item in the money bag, coins used into
//! the money bag, and taking everything from an open corpse. Fixture: a new player given test items by the world, as a
//! quest's reward gives them, and a corpse made beside it holding items.

use auberean_protocol::ServerMessage;
use auberean_script::args::Arg;
use yalain_net::ConnId;
use yalain_testkit::{test_catalog, Harness, STEP, TEST_ITEM};
use yalain_world::calls::{self, Node, Value};
use yalain_world::inventory::{self, move_graph, Move};
use yalain_world::pack::{self, function, status};
use yalain_world::state::Held;
use yalain_world::things::{Corpse, Kind, Thing};

/// A template that stacks to a thousand, and one worth 25 gold each.
const STACKING: u32 = 0x4700_2109;
const VALUED: u32 = 0x4700_210A;
const SCRIPTED: u32 = 0x4700_210B;
const WORTH: i64 = 25;
const CONTAINER: u32 = 258;
const QUANTITY: u32 = 262;
const MONEY: u32 = 3100;

/// The move descriptions the server answered with since `from`, by their slots.
fn answers(h: &Harness, b: usize, from: usize) -> Vec<Vec<(u8, u32)>> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c) if c.function.0 == inventory::function::MOVE_DONE => c
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

/// An item the world gives goes to the front of the pack and the items there each move up one;
/// the move names slot 0 as the slot it was sent to and the slot it went to.
/// Behaviour: server.inventory.items-come-in-at-the-packs-front
#[test]
fn an_item_given_comes_in_at_the_packs_front_and_the_move_names_that_slot() {
    let mut h = Harness::new();
    h.server.catalog = test_catalog();
    let (b, _) = h.player_in_world("one", "Elara");
    let world = *h.server.players.keys().next().expect("a player");
    inventory::give(&mut h.server, &mut h.net, world, TEST_ITEM, 1);
    let ids = |h: &Harness| -> Vec<(u64, u32)> {
        let mut v: Vec<(u64, u32)> = h.server.players[&world]
            .play
            .items
            .iter()
            .filter(|x| x.location == 0)
            .map(|x| (x.id, x.slot))
            .collect();
        v.sort_by_key(|x| x.1);
        v
    };
    let before = ids(&h);
    let from = h.bots[b].inbox.len();
    inventory::give(&mut h.server, &mut h.net, world, TEST_ITEM, 1);
    h.run(5);
    let after = ids(&h);
    assert_eq!(after.len(), before.len() + 1, "one more item in the pack");
    let new = after[0].0;
    assert!(
        !before.iter().any(|(id, _)| *id == new),
        "the new item first"
    );
    let moved: Vec<(u64, u32)> = before.iter().map(|(id, slot)| (*id, slot + 1)).collect();
    assert_eq!(after[1..], moved[..], "the others each one further on");
    let slots = answers(&h, b, from);
    let m = slots.first().expect("the move answered");
    assert_eq!(m[11].1, 0, "sent to slot 0");
    assert_eq!(m[24].1, 0, "went to slot 0");
}

/// The reorganization answers a client was sent since `from`, by their slots.
fn reorganized(h: &Harness, b: usize, from: usize) -> Vec<Vec<(u8, u32)>> {
    h.calls(b, from)
        .into_iter()
        .filter(|(f, _)| *f == function::REORGANIZE_DONE)
        .filter_map(|(_, v)| {
            v.iter().find_map(|x| match x.root() {
                Some(Node::Script { slots, .. }) => Some(slots.clone()),
                _ => None,
            })
        })
        .collect()
}

/// A player in the world with nothing in its pack, and the stacking and valued templates in the
/// catalog; its client, its id and its world connection.
fn player() -> (Harness, usize, u64, ConnId) {
    let mut h = Harness::new();
    h.server.catalog = test_catalog();
    let mut props = yalain_content::Props::default();
    props.set("Item_MaxStackSize", yalain_content::Prop::Int(1000));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(STACKING, 0x200, props));
    let mut props = yalain_content::Props::default();
    props.set("Item_Value", yalain_content::Prop::Int(WORTH));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(VALUED, 0x200, props));
    let (b, me) = h.player_in_world("one", "Elara");
    let world = *h.server.players.keys().next().expect("a player");
    h.server.players.get_mut(&world).unwrap().play.items.clear();
    (h, b, me, world)
}

/// Give a player `n` items of a template one after another; returns the pack front to back.
fn give(h: &mut Harness, world: ConnId, template: u32, n: usize) -> Vec<u64> {
    for _ in 0..n {
        inventory::give(&mut h.server, &mut h.net, world, template, 1);
    }
    h.run(2);
    pack_order(h, world)
}

/// A pack's items in slot order, with their slots.
fn pack_slots(h: &Harness, world: ConnId) -> Vec<(u64, u32)> {
    let mut v: Vec<(u64, u32)> = h.server.players[&world]
        .play
        .items
        .iter()
        .filter(|x| x.location == 0)
        .map(|x| (x.id, x.slot))
        .collect();
    v.sort_by_key(|x| x.1);
    v
}

fn pack_order(h: &Harness, world: ConnId) -> Vec<u64> {
    pack_slots(h, world).into_iter().map(|x| x.0).collect()
}

/// The client's request to move `quantity` of `item` from `from` to `target` in `me`'s pack, as
/// the 2017 clients sent it (the slot counting the places after the item has left its own).
fn move_request(me: u64, item: u64, quantity: u32, from: i32, target: i32) -> Arg {
    move_graph(&Move {
        merge: true,
        actual_from_slot: from,
        target: me,
        actual_target: me,
        unlock: true,
        target_slot: target,
        check_take: true,
        from: me,
        all_taken: true,
        actual_target_slot: target,
        quantity_left: quantity,
        quantity,
        split_item: item,
        merge_container: me,
        grab: true,
        actual_from: me,
        from_slot: from,
        item,
        contain: true,
        auto_merge: true,
        merge_slot: target,
        ..Move::default()
    })
}

/// A pack fills its first segment, then the front of the first bag with room: an item given to a
/// pack of 42 goes to slot 42, the first bag's first place; the pack's segments name each one's
/// room and how many items it holds.
/// Behaviour: server.inventory.the-pack-has-three-bags
#[test]
fn an_item_given_to_a_full_pack_goes_to_the_front_of_the_first_bag() {
    let (mut h, b, _me, world) = player();
    let full: Vec<Held> = (0..inventory::PACK_SLOTS)
        .map(|n| Held {
            id: 0x8130_0000_0FFF_0000 + u64::from(n),
            template: TEST_ITEM,
            slot: n,
            location: 0,
            quantity: 1,
        })
        .collect();
    h.server.players.get_mut(&world).unwrap().play.items = full;
    let from = h.bots[b].inbox.len();
    inventory::give(&mut h.server, &mut h.net, world, TEST_ITEM, 1);
    h.run(2);
    let slots = answers(&h, b, from);
    assert_eq!(slots[0][24].1, 42, "into the first bag's first place");
    let items = &h.server.players[&world].play.items;
    assert_eq!(
        inventory::pack_segments(items),
        [(42, 42), (12, 1), (12, 0), (12, 0)],
        "the pack full, one in the first bag"
    );
    inventory::give(&mut h.server, &mut h.net, world, TEST_ITEM, 1);
    h.run(2);
    let bag: Vec<u32> = pack_slots(&h, world)
        .into_iter()
        .map(|x| x.1)
        .filter(|s| *s >= 42)
        .collect();
    assert_eq!(bag, [42, 43], "the next in front, the first moving up one");
}

/// A move about the pack: the item leaves its slot (the segment closing up) and goes in at the
/// slot named, here the second bag's first; the answer names the server's own slot for it (the
/// client's, which may differ, echoed), the
/// target echoed and as the slot it went to, and the slot one before the target as it came from an
/// earlier one (the target itself coming back); the server's checks made, nothing merged; its
/// container cleared and set again.
/// Behaviour: server.inventory.reorganize-moves-an-item
#[test]
fn a_move_about_the_pack_takes_the_item_to_the_slot_named_and_closes_up_behind_it() {
    let (mut h, b, me, world) = player();
    let items = give(&mut h, world, TEST_ITEM, 4);
    let item = items[1];
    let from = h.bots[b].inbox.len();
    h.ask(
        b,
        function::REORGANIZE,
        &[move_request(me, item, 1, 3, 54)],
        4,
    );
    let done = reorganized(&h, b, from);
    assert_eq!(done.len(), 1, "answered once");
    let d = &done[0];
    assert_eq!(d[40].1, 0, "not refused");
    assert_eq!(d[1].1, 1, "from the server's own slot");
    assert_eq!(d[39].1, 3, "the client's slot echoed");
    assert_eq!((d[11].1, d[24].1), (54, 54), "to the slot named");
    assert_eq!(
        d[50].1, 53,
        "one before the target, coming from an earlier slot"
    );
    assert_eq!(
        (d[0].1, d[49].1, d[7].1),
        (0, 0, 1),
        "nothing merged, checked"
    );
    assert_eq!(d[22].1, TEST_ITEM, "its template");
    let containers: Vec<u64> = h
        .world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::UpdateInstanceIdVisual(q)
                if q.object.id == item && q.quality == CONTAINER =>
            {
                Some(q.value)
            }
            _ => None,
        })
        .collect();
    assert_eq!(containers, [0, me], "its container cleared and set again");
    assert_eq!(
        pack_slots(&h, world),
        [(items[0], 0), (items[2], 1), (items[3], 2), (item, 54)],
        "closed up behind it, in the second bag"
    );
    let from = h.bots[b].inbox.len();
    h.ask(
        b,
        function::REORGANIZE,
        &[move_request(me, item, 1, 54, 0)],
        4,
    );
    let d = &reorganized(&h, b, from)[0];
    assert_eq!((d[1].1, d[50].1), (54, 0), "coming back: the target itself");
    assert_eq!(
        pack_order(&h, world),
        [item, items[0], items[2], items[3]],
        "in front again"
    );
}

/// Part of a stack moved splits off as a new stack at the slot named: the answer names the new
/// stack, how many it took and that not all were; the new stack is made, the old one's count
/// sent.
/// Behaviour: server.inventory.reorganize-splits-a-stack
#[test]
fn part_of_a_stack_moved_splits_off_as_a_new_stack_at_the_slot() {
    let (mut h, b, me, world) = player();
    give(&mut h, world, TEST_ITEM, 2);
    inventory::give(&mut h.server, &mut h.net, world, STACKING, 5);
    h.run(2);
    let stack = pack_order(&h, world)[0];
    let from = h.bots[b].inbox.len();
    h.ask(
        b,
        function::REORGANIZE,
        &[move_request(me, stack, 2, 0, 2)],
        4,
    );
    let d = &reorganized(&h, b, from)[0];
    let new = u64::from(d[29].1) << 32 | u64::from(d[28].1);
    assert_ne!(new, stack, "a new stack");
    assert_eq!(
        (d[48].1, d[23].1, d[50].1),
        (2, 0, 2),
        "two split off, not all"
    );
    let msgs = h.world_messages(b, from);
    assert!(
        msgs.iter()
            .any(|m| matches!(m, ServerMessage::CreateObject(o) if o.id == new)),
        "the new stack made"
    );
    assert!(
        msgs.iter().any(|m| matches!(m,
            ServerMessage::UpdateIntVisual(q)
                if q.object.id == stack && q.quality == QUANTITY && q.value == 3)),
        "three left"
    );
    let held: Vec<(u64, u32, u32)> = h.server.players[&world]
        .play
        .items
        .iter()
        .filter(|x| x.template == STACKING)
        .map(|x| (x.id, x.slot, x.quantity))
        .collect();
    assert!(
        held.contains(&(stack, 0, 3)) && held.contains(&(new, 2, 2)),
        "{held:?}"
    );
}

/// A stack moved onto a stack of its kind with room joins it: the answer says merged with no slot
/// to merge at; the kept stack's count sent, the moved one destroyed.
/// Behaviour: server.inventory.reorganize-joins-a-stack
#[test]
fn a_stack_moved_onto_a_stack_of_its_kind_joins_it() {
    let (mut h, b, me, world) = player();
    inventory::give(&mut h.server, &mut h.net, world, STACKING, 4);
    give(&mut h, world, TEST_ITEM, 1);
    inventory::give(&mut h.server, &mut h.net, world, STACKING, 3);
    h.run(2);
    let order = pack_order(&h, world);
    let (moved, kept) = (order[0], order[2]);
    let from = h.bots[b].inbox.len();
    h.ask(
        b,
        function::REORGANIZE,
        &[move_request(me, moved, 3, 0, 1)],
        4,
    );
    let d = &reorganized(&h, b, from)[0];
    assert_eq!(
        (d[0].1, d[50].1),
        (1, u32::MAX),
        "merged, no slot to merge at"
    );
    let msgs = h.world_messages(b, from);
    assert!(
        msgs.iter().any(|m| matches!(m,
            ServerMessage::UpdateIntVisual(q)
                if q.object.id == kept && q.quality == QUANTITY && q.value == 7)),
        "seven in the kept stack"
    );
    assert!(
        msgs.iter()
            .any(|m| matches!(m, ServerMessage::ForgetObject(o) if o.0.id == moved)),
        "the moved one destroyed"
    );
    assert_eq!(pack_order(&h, world), [order[1], kept], "two left");
}

/// Keep a player busy: it starts taking everything from an empty corpse it has open.
fn busy(h: &mut Harness, b: usize, world: ConnId, me: u64) {
    let (corpse, _) = opened_corpse(h, world, me, 0);
    h.ask(b, function::TAKE_ALL, &[take_all_request(corpse, me)], 4);
}

/// A move asked while another action is under way is refused with actions locked, and nothing
/// moves.
/// Behaviour: server.inventory.reorganize-refused-while-busy
#[test]
fn a_move_about_the_pack_while_busy_is_refused_and_nothing_moves() {
    let (mut h, b, me, world) = player();
    let items = give(&mut h, world, TEST_ITEM, 3);
    busy(&mut h, b, world, me);
    let from = h.bots[b].inbox.len();
    h.ask(
        b,
        function::REORGANIZE,
        &[move_request(me, items[0], 1, 0, 2)],
        4,
    );
    let d = &reorganized(&h, b, from)[0];
    assert_eq!(d[40].1, status::ACTIONS_LOCKED, "refused");
    assert_eq!(pack_order(&h, world), items, "nothing moved");
}

/// An item dragged to the money bag is sold for its value: "Adding 25 pieces of Gold." with the
/// gold as its variable, the money up by it, the item gone from the pack and destroyed, the drag
/// answered with no item, the slot and the container.
/// Behaviour: server.inventory.money-bag-sells-an-item-for-its-value
#[test]
fn an_item_dragged_to_the_money_bag_is_sold_for_its_value() {
    let (mut h, b, me, world) = player();
    give(&mut h, world, TEST_ITEM, 1);
    inventory::give(&mut h.server, &mut h.net, world, VALUED, 1);
    h.run(2);
    let item = pack_order(&h, world)[0];
    yalain_world::quals::set_int(
        &mut h.server.players.get_mut(&world).unwrap().qualities,
        MONEY,
        100,
    );
    let from = h.bots[b].inbox.len();
    h.ask(
        b,
        function::TO_MONEY_BAG,
        &[
            Arg::LongInt(me),
            Arg::Int(0),
            Arg::Int(1),
            Arg::LongInt(item),
        ],
        4,
    );
    let calls = h.calls(b, from);
    let text = calls
        .iter()
        .position(|(f, _)| *f == yalain_world::session::function::SHOW_TEXT)
        .expect("told");
    let done = calls
        .iter()
        .position(|(f, _)| *f == function::TO_MONEY_BAG_DONE)
        .expect("answered");
    assert!(text < done, "told first");
    assert_eq!(
        calls[done].1,
        [Value::Long(me), Value::Int(0), Value::Long(0)],
        "no item, the slot, the container"
    );
    let msgs = h.world_messages(b, from);
    assert!(
        msgs.iter().any(|m| matches!(m,
            ServerMessage::UpdateInt(q) if q.quality == MONEY && q.value == 125)),
        "the money up by 25"
    );
    assert!(
        msgs.iter()
            .any(|m| matches!(m, ServerMessage::ForgetObject(o) if o.0.id == item)),
        "destroyed"
    );
    assert_eq!(pack_slots(&h, world).len(), 1, "gone from the pack");
    assert_eq!(pack_slots(&h, world)[0].1, 0, "the other closed up");
}

/// A drag to the money bag while busy: "You are too busy to do that." and the drag answered with
/// the item; nothing paid.
/// Behaviour: server.inventory.money-bag-refused-while-busy
#[test]
fn a_drag_to_the_money_bag_while_busy_is_refused() {
    let (mut h, b, me, world) = player();
    inventory::give(&mut h.server, &mut h.net, world, VALUED, 1);
    h.run(2);
    let item = pack_order(&h, world)[0];
    busy(&mut h, b, world, me);
    let from = h.bots[b].inbox.len();
    h.ask(
        b,
        function::TO_MONEY_BAG,
        &[
            Arg::LongInt(me),
            Arg::Int(0),
            Arg::Int(1),
            Arg::LongInt(item),
        ],
        4,
    );
    let calls = h.calls(b, from);
    let done = calls
        .iter()
        .find(|(f, _)| *f == function::TO_MONEY_BAG_DONE)
        .expect("answered");
    assert_eq!(done.1[2], Value::Long(item), "with the item");
    assert!(
        !h.world_messages(b, from)
            .iter()
            .any(|m| matches!(m, ServerMessage::UpdateInt(q) if q.quality == MONEY)),
        "nothing paid"
    );
    assert_eq!(pack_order(&h, world), [item], "still held");
}

/// A template that stacks, whose value is its class's alone.
fn scripted(h: &mut Harness, value: u32) -> u32 {
    let mut props = yalain_content::Props::default()
        .with_class(&[("Item_Value", yalain_content::Prop::Int(i64::from(value)))]);
    props.set("Item_MaxStackSize", yalain_content::Prop::Int(1000));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(SCRIPTED, 0x200, props));
    SCRIPTED
}

/// `n` of a pack's item dragged to the money bag; what the money reads after it.
fn drag_to_bag(h: &mut Harness, b: usize, me: u64, world: ConnId, item: u64, n: u32) -> i32 {
    h.ask(
        b,
        function::TO_MONEY_BAG,
        &[
            Arg::LongInt(me),
            Arg::Int(0),
            Arg::Int(n),
            Arg::LongInt(item),
        ],
        4,
    );
    yalain_world::quals::int(&h.server.players[&world].qualities, MONEY).unwrap_or(0)
}

/// An item whose data names no value sells for the one its class's code sets.
/// Behaviour: server.inventory.a-scripts-value-stands-where-the-data-names-none
#[test]
fn an_item_only_its_class_values_sells_for_that_value() {
    let (mut h, b, me, world) = player();
    let t = scripted(&mut h, 7);
    inventory::give(&mut h.server, &mut h.net, world, t, 1);
    h.run(2);
    let item = pack_order(&h, world)[0];
    assert_eq!(drag_to_bag(&mut h, b, me, world, item, 1), 7);
    assert!(pack_order(&h, world).is_empty(), "sold");
}

/// A value in the data stands over the class's, though it be nothing.
/// Behaviour: server.inventory.a-scripts-value-stands-where-the-data-names-none
#[test]
fn a_value_in_the_data_stands_over_the_classs() {
    for (data, paid) in [(3, 3), (0, 0)] {
        let (mut h, b, me, world) = player();
        let mut props = yalain_content::Props::default();
        props.set("Item_Value", yalain_content::Prop::Int(data));
        let props = props.with_class(&[("Item_Value", yalain_content::Prop::Int(10))]);
        h.server
            .catalog
            .insert(yalain_testkit::test_template(SCRIPTED, 0x200, props));
        inventory::give(&mut h.server, &mut h.net, world, SCRIPTED, 1);
        h.run(2);
        let item = pack_order(&h, world)[0];
        assert_eq!(drag_to_bag(&mut h, b, me, world, item, 1), paid);
    }
}

/// Part of a stack dragged to the money bag pays for the pieces that went, and the rest stays in the
/// pack, its count told in turn; the rest dragged after it pays the same and goes.
/// Behaviour: server.inventory.money-bag-pays-for-part-of-a-stack
#[test]
fn part_of_a_stack_dragged_to_the_money_bag_pays_for_what_went_and_the_rest_stays() {
    let (mut h, b, me, world) = player();
    let t = scripted(&mut h, 7);
    inventory::give(&mut h.server, &mut h.net, world, t, 5);
    h.run(2);
    let item = pack_order(&h, world)[0];
    let from = h.bots[b].inbox.len();
    assert_eq!(drag_to_bag(&mut h, b, me, world, item, 2), 14);
    let told: Vec<i32> = h
        .world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::UpdateIntVisual(q) if q.object.id == item => Some(q.value),
            _ => None,
        })
        .collect();
    assert_eq!(told, [3], "the count left");
    assert_eq!(pack_order(&h, world), [item], "the rest stays");
    assert_eq!(drag_to_bag(&mut h, b, me, world, item, 3), 35);
    assert!(pack_order(&h, world).is_empty(), "the rest goes");
}

/// An item worth nothing dragged to the money bag is refused and kept: "That item cannot be
/// transformed into money.", the drag answered with the item, nothing paid.
/// Behaviour: server.inventory.money-bag-refuses-an-item-of-no-worth
#[test]
fn an_item_worth_nothing_dragged_to_the_money_bag_is_refused_and_kept() {
    let (mut h, b, me, world) = player();
    inventory::give(&mut h.server, &mut h.net, world, TEST_ITEM, 1);
    h.run(2);
    let item = pack_order(&h, world)[0];
    let from = h.bots[b].inbox.len();
    assert_eq!(drag_to_bag(&mut h, b, me, world, item, 1), 0);
    let calls = h.calls(b, from);
    let done = calls
        .iter()
        .find(|(f, _)| *f == function::TO_MONEY_BAG_DONE)
        .expect("answered");
    assert_eq!(done.1[2], Value::Long(item), "with the item");
    let refusal = 0xD4B3_FC38_u32.to_le_bytes();
    assert!(
        h.bots[b].inbox[from..]
            .iter()
            .any(|r| r.bytes.windows(4).any(|w| w == refusal)),
        "told it cannot"
    );
    assert!(
        !h.world_messages(b, from).iter().any(|m| matches!(
            m,
            ServerMessage::UpdateInt(q) if q.quality == MONEY
        ) || matches!(m, ServerMessage::ForgetObject(_))),
        "nothing paid, nothing destroyed"
    );
    assert_eq!(pack_order(&h, world), [item], "still held");
}

/// A stack with no value of its own keeps taking new pieces once its template's value comes from its
/// class: both are worth the same.
/// Behaviour: server.inventory.a-scripts-value-stands-where-the-data-names-none
#[test]
fn a_stack_kept_with_no_value_of_its_own_still_takes_new_pieces_of_a_template_its_class_values() {
    let (mut h, _, _, world) = player();
    let t = scripted(&mut h, 7);
    inventory::give_stacked(&mut h.server, &mut h.net, world, t, 2);
    inventory::give_stacked(&mut h.server, &mut h.net, world, t, 3);
    h.run(2);
    let held = &h.server.players[&world].play.items;
    assert_eq!(held.len(), 1, "one stack");
    assert_eq!(held[0].quantity, 5);
}

/// A corpse beside the player holding `n` test items, opened by it; returns the corpse and its
/// items in order.
fn opened_corpse(h: &mut Harness, world: ConnId, me: u64, n: usize) -> (u64, Vec<u64>) {
    let t = h.server.catalog.get(TEST_ITEM).cloned().expect("template");
    let id = h.server.things.new_id();
    let mut corpse = Thing::of(id, &t, Kind::Corpse);
    corpse.at = Some(h.server.players[&world].position);
    let items: Vec<u64> = (0..n)
        .map(|i| {
            let item = inventory::make_item(&mut h.server, TEST_ITEM, 1, 0).expect("made");
            h.server.things.get_mut(item).expect("item").container =
                Some((id, u32::try_from(i).unwrap_or(0)));
            item
        })
        .collect();
    corpse.corpse = Some(Corpse {
        killer: me,
        originator: 0,
        contents: items.clone(),
        creature: 0,
        butchered: false,
        killer_level: 1,
        expires: 1.0e12,
        ..Default::default()
    });
    h.server.things.insert(corpse);
    inventory::open(&mut h.server, &mut h.net, world, id);
    h.run(2);
    (id, items)
}

/// The client's request to take everything from `corpse`, as the 2017 clients sent it.
fn take_all_request(corpse: u64, me: u64) -> Arg {
    use yalain_world::graph::{tag, Builder, Slot};
    #[allow(clippy::cast_possible_truncation)]
    let long = |v: u64| {
        [
            Slot::Tagged(tag::LONG, v as u32),
            Slot::Tagged(tag::HIGH, (v >> 32) as u32),
        ]
    };
    let mut slots = vec![
        Slot::Int(0),
        Slot::Int(0),
        Slot::Int(1),
        Slot::Int(0),
        Slot::Int(0),
        Slot::Int(0),
    ];
    slots.extend(long(corpse));
    slots.extend([Slot::Ref(None), Slot::Int(0), Slot::Int(0), Slot::Ref(None)]);
    slots.extend(long(me));
    let mut g = Builder::new();
    g.object(pack::TAKE_ALL_DESC, &slots);
    Arg::Object(g.graph())
}

/// The take-all answers a client was sent since `from`: (status, items taken, items left).
fn took(h: &Harness, b: usize, from: usize) -> Vec<(u32, usize, usize)> {
    h.calls(b, from)
        .into_iter()
        .filter(|(f, _)| *f == function::TAKE_ALL_DONE)
        .filter_map(|(_, v)| {
            v.iter().find_map(|x| {
                let Some(Node::Script { slots, .. }) = x.root() else {
                    return None;
                };
                let count = |slot: usize| match x.entry(slots[slot].1) {
                    Some(Node::Native { bytes, .. }) => bytes.len() / 8,
                    _ => 0,
                };
                Some((slots[5].1, count(11), count(8)))
            })
        })
        .collect()
}

/// Taking everything from the corpse the player has open: it plays its picking-up behavior at
/// once; 1.72 s later each item comes into the pack in the corpse's order, each to the front and
/// answered as a take with no animation, then the request is answered with the items taken.
/// Another take-all meanwhile is refused (actions locked).
/// Behaviour: server.inventory.take-all-takes-every-item
#[test]
fn taking_all_from_an_open_corpse_brings_every_item_into_the_pack_in_its_order() {
    let (mut h, b, me, world) = player();
    let mine = give(&mut h, world, TEST_ITEM, 1);
    let (corpse, items) = opened_corpse(&mut h, world, me, 2);
    let from = h.bots[b].inbox.len();
    h.ask(b, function::TAKE_ALL, &[take_all_request(corpse, me)], 4);
    assert!(
        h.world_messages(b, from).iter().any(|m| matches!(m,
            ServerMessage::PlayBehavior(e) if e.sender.id == me
                && e.action.behavior == pack::TAKING
                && e.action.fx_script == pack::TAKE_ALL_SCRIPT)),
        "the picking-up behavior"
    );
    assert!(took(&h, b, from).is_empty(), "not answered yet");
    let again = h.bots[b].inbox.len();
    h.ask(b, function::TAKE_ALL, &[take_all_request(corpse, me)], 4);
    assert_eq!(
        took(&h, b, again),
        [(status::ACTIONS_LOCKED, 0, 0)],
        "a second refused at once"
    );
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    h.run((pack::TAKE_ALL_SECONDS / STEP).ceil() as usize);
    let moves = answers(&h, b, from);
    assert_eq!(moves.len(), 2, "each item answered");
    for m in &moves {
        assert_eq!(
            (m[12].1, m[24].1, m[31].1),
            (20, 0, 1),
            "a take to the front, no animation"
        );
    }
    assert_eq!(
        took(&h, b, from).last(),
        Some(&(0, 2, 0)),
        "both taken, none left"
    );
    assert_eq!(
        pack_order(&h, world),
        [items[1], items[0], mine[0]],
        "each in front of the one before"
    );
    assert!(
        h.server
            .things
            .get(corpse)
            .and_then(|t| t.corpse.as_ref())
            .is_some_and(|c| c.contents.is_empty()),
        "the corpse empty"
    );
}

/// A take-all from a container the player does not have open is refused (not a viewer).
/// Behaviour: server.inventory.take-all-needs-the-container-open
#[test]
fn taking_all_from_a_container_not_open_is_refused() {
    let (mut h, b, me, world) = player();
    let (corpse, _) = opened_corpse(&mut h, world, me, 1);
    inventory::close(&mut h.server, &mut h.net, world, corpse);
    h.run(2);
    let from = h.bots[b].inbox.len();
    h.ask(b, function::TAKE_ALL, &[take_all_request(corpse, me)], 4);
    assert_eq!(took(&h, b, from), [(status::NOT_VIEWER, 0, 0)], "refused");
    assert!(pack_order(&h, world).is_empty(), "nothing taken");
}

/// Coins dragged out of the money bag come into the pack as one stack of that many, the money goes
/// down by as many, and the drag is answered with the container the client named.
/// Behaviour: server.inventory.money-bag-gives-coins
#[test]
fn coins_dragged_out_of_the_money_bag_come_into_the_pack_and_the_money_goes_down() {
    let (mut h, b, me, world) = player();
    h.server.catalog.insert(yalain_testkit::test_template(
        pack::COINS,
        0x181,
        yalain_content::Props::default(),
    ));
    yalain_world::quals::set_int(
        &mut h.server.players.get_mut(&world).unwrap().qualities,
        MONEY,
        100,
    );
    let from = h.bots[b].inbox.len();
    h.ask(
        b,
        function::FROM_MONEY_BAG,
        &[Arg::LongInt(me), Arg::Int(0), Arg::Int(30)],
        4,
    );
    let calls = h.calls(b, from);
    assert!(
        calls
            .iter()
            .any(|(f, v)| *f == function::FROM_MONEY_BAG_DONE && v == &[Value::Long(me)]),
        "answered with the container"
    );
    assert!(
        h.world_messages(b, from).iter().any(|m| matches!(m,
            ServerMessage::UpdateInt(q) if q.quality == MONEY && q.value == 70)),
        "the money down by 30"
    );
    let coins: Vec<u32> = h.server.players[&world]
        .play
        .items
        .iter()
        .filter(|i| i.template == pack::COINS)
        .map(|i| i.quantity)
        .collect();
    assert_eq!(coins, [30], "one stack of thirty");
}

/// The segments of a pack's contents as the server sends them: each segment's room and how many
/// items it holds.
fn windows(h: &Harness, world: ConnId) -> (Vec<(u32, u32)>, Vec<u64>) {
    let m = inventory::reset_contents(&h.server, world);
    let ServerMessage::PlayerCall(c) = m else {
        panic!("a call");
    };
    let v = c
        .args
        .as_ref()
        .and_then(|a| calls::values(&a.0).ok())
        .expect("values");
    let mut segments = Vec::new();
    let mut contents = Vec::new();
    for x in &v {
        let Value::Graph(g) = x else { continue };
        for (_, n) in g {
            match n {
                Node::Script { package, slots } if *package == 0x1AF6 => {
                    segments.push((slots[0].1, slots[1].1));
                }
                Node::Native { bytes, .. } if bytes.len() >= 4 => {
                    let n = u32::from_le_bytes(bytes[..4].try_into().expect("a count")) as usize;
                    if bytes.len() == 4 + 8 * n && n > contents.len() {
                        contents = (0..n)
                            .map(|i| {
                                let at = 4 + 8 * i;
                                u64::from_le_bytes(bytes[at..at + 8].try_into().expect("an id"))
                            })
                            .collect();
                    }
                }
                _ => {}
            }
        }
    }
    (segments, contents)
}

/// The pack's segments are windows of one list: the main pack's room and count, then each bag's;
/// the counts tile the contents in order, each window holding the items of its segment; an item
/// coming into a bag lengthens that window and moves the later ones on by one, the slots it and
/// the others are numbered by staying the segments' own.
/// Behaviour: server.inventory.pack-segments-are-windows-of-the-contents
#[test]
fn the_packs_segments_are_windows_that_tile_its_contents_in_order() {
    let (mut h, _, _, world) = player();
    let main = give(&mut h, world, TEST_ITEM, 42);
    let (segments, contents) = windows(&h, world);
    assert_eq!(segments, [(42, 42), (12, 0), (12, 0), (12, 0)]);
    assert_eq!(contents, main, "the main window is the whole list");
    inventory::give(&mut h.server, &mut h.net, world, STACKING, 1);
    h.run(2);
    let bag_item = pack_slots(&h, world)
        .iter()
        .find(|(_, slot)| *slot == 42)
        .expect("into the first bag at its first slot")
        .0;
    let (segments, contents) = windows(&h, world);
    assert_eq!(segments, [(42, 42), (12, 1), (12, 0), (12, 0)]);
    let total: u32 = segments.iter().map(|(_, n)| n).sum();
    assert_eq!(total as usize, contents.len(), "the counts tile the list");
    assert_eq!(contents[..42], main[..], "the main window first");
    assert_eq!(contents[42], bag_item, "the first bag's window after it");
}

/// Past every segment an item goes to the pack itself, at its front, and no bag grows: the move
/// names slot 0 and says it ran past the room, the pack's window lengthens, the bags' stay as they
/// were and are told after it. The pack's own items past its places go into a bag with room like any
/// other, and the pack closes up.
/// Behaviour: server.inventory.past-every-segment-the-pack-itself-grows
#[test]
fn an_item_past_every_segment_goes_to_the_packs_front_and_no_bag_grows() {
    let (mut h, b, me, world) = player();
    let before = give(&mut h, world, TEST_ITEM, inventory::PACK_ROOM as usize);
    let from = h.bots[b].inbox.len();
    inventory::give(&mut h.server, &mut h.net, world, STACKING, 1);
    h.run(2);
    let slots = answers(&h, b, from);
    assert_eq!(
        (slots[0][11].1, slots[0][24].1, slots[0][34].1),
        (0, 0, 1),
        "to the front, past the room"
    );
    let (segments, contents) = windows(&h, world);
    assert_eq!(segments, [(42, 43), (12, 12), (12, 12), (12, 12)]);
    assert_eq!(contents.len(), 79, "the counts tile the list");
    assert_eq!(contents[1..43], before[..42], "the pack's own moved on");
    assert_eq!(
        contents[43..],
        before[42..],
        "the bags' after them, in order"
    );
    let first = contents[0];
    inventory::give(&mut h.server, &mut h.net, world, TEST_ITEM, 1);
    h.run(2);
    let (segments, contents) = windows(&h, world);
    assert_eq!(segments[0], (42, 44), "and on");
    assert_eq!(contents[1], first);
    let last = contents[43];
    // A bag with room takes the pack's last.
    let gone = contents[44];
    h.server
        .players
        .get_mut(&world)
        .unwrap()
        .play
        .items
        .retain(|x| x.id != gone);
    h.ask(
        b,
        function::REORGANIZE,
        &[move_request(me, last, 1, 43, 42)],
        4,
    );
    let (segments, contents) = windows(&h, world);
    assert_eq!(
        segments[..2],
        [(42, 43), (12, 12)],
        "out of the pack, into the bag"
    );
    assert_eq!(contents[43], last, "the bag's first");
    assert_eq!(contents.len(), 79);
}

/// A move to the slot just past a full last bag is refused: that slot is no way into the pack, though
/// the pack has room.
/// Behaviour: server.inventory.past-every-segment-the-pack-itself-grows
#[test]
fn a_move_past_a_full_last_bag_is_refused_though_the_pack_has_room() {
    let (mut h, b, me, world) = player();
    let items = give(&mut h, world, TEST_ITEM, 13);
    let p = h.server.players.get_mut(&world).unwrap();
    for (n, id) in items[1..].iter().enumerate() {
        p.play.items.iter_mut().find(|x| x.id == *id).unwrap().slot =
            66 + u32::try_from(n).unwrap_or(0);
    }
    let from = h.bots[b].inbox.len();
    h.ask(
        b,
        function::REORGANIZE,
        &[move_request(me, items[0], 1, 0, 78)],
        4,
    );
    assert_eq!(reorganized(&h, b, from)[0][40].1, status::CONTAINER_FULL);
    assert_eq!(pack_slots(&h, world)[0], (items[0], 0), "where it was");
}

/// A pack kept with an item past the last bag holds it in the pack itself, in its window: no slot
/// past the bags names a bag.
/// Behaviour: server.inventory.past-every-segment-the-pack-itself-grows
#[test]
fn an_item_kept_past_the_last_bag_is_told_in_the_pack_itself() {
    let (mut h, _, _, world) = player();
    let items = give(&mut h, world, TEST_ITEM, 3);
    let p = h.server.players.get_mut(&world).unwrap();
    p.play
        .items
        .iter_mut()
        .find(|x| x.id == items[0])
        .unwrap()
        .slot = inventory::PACK_ROOM;
    p.play
        .items
        .iter_mut()
        .find(|x| x.id == items[1])
        .unwrap()
        .slot = 66;
    assert_eq!(inventory::segment_of(inventory::PACK_ROOM), 0);
    let (segments, contents) = windows(&h, world);
    assert_eq!(segments, [(42, 2), (12, 0), (12, 0), (12, 1)]);
    assert_eq!(contents, [items[2], items[0], items[1]], "the pack's first");
    let p = h.server.players.get_mut(&world).unwrap();
    inventory::close_up(&mut p.play.items);
    assert_eq!(
        pack_slots(&h, world),
        [(items[2], 0), (items[0], 1), (items[1], 66)],
        "closed up in its own place"
    );
}

/// Coins used from the pack go into the money bag whole, one gold each, as a drag of them would:
/// "Adding %1 pieces of Gold.", the money up by as many, the coins gone, and the drag's answer
/// from the player at slot 0, with no answer to the use.
/// Behaviour: server.inventory.coins-used-into-money
#[test]
fn coins_used_from_the_pack_go_into_the_money_bag_whole() {
    const USE_DONE: u32 = 0x8216_0021;
    let (mut h, b, me, world) = player();
    h.server.catalog.insert(yalain_testkit::test_template(
        pack::COINS,
        0x181,
        yalain_content::Props::default(),
    ));
    inventory::give(&mut h.server, &mut h.net, world, pack::COINS, 500);
    h.run(2);
    let coins = pack_order(&h, world)[0];
    yalain_world::quals::set_int(
        &mut h.server.players.get_mut(&world).unwrap().qualities,
        MONEY,
        100,
    );
    let from = h.bots[b].inbox.len();
    h.ask(
        b,
        yalain_world::requests::function::USE,
        &[yalain_testkit::play::use_of(coins, me)],
        4,
    );
    let calls = h.calls(b, from);
    let text = calls
        .iter()
        .position(|(f, _)| *f == yalain_world::session::function::SHOW_TEXT)
        .expect("told");
    let done = calls
        .iter()
        .position(|(f, _)| *f == function::TO_MONEY_BAG_DONE)
        .expect("answered as a drag");
    assert!(text < done, "told first");
    assert_eq!(
        calls[done].1,
        [Value::Long(me), Value::Int(0), Value::Long(0)],
        "from the player, slot 0, nothing left"
    );
    assert!(
        !calls.iter().any(|(f, _)| *f == USE_DONE),
        "the use not answered"
    );
    assert!(
        h.world_messages(b, from).iter().any(|m| matches!(m,
            ServerMessage::UpdateInt(q) if q.quality == MONEY && q.value == 600)),
        "the money up by 500"
    );
    assert!(pack_order(&h, world).is_empty(), "the coins gone");
}

/// Coins taken out of the money bag into a full pack: nothing moves, the money stays.
/// Behaviour: server.inventory.coins-out-of-the-bag-need-room
#[test]
fn coins_out_of_the_money_bag_into_a_full_pack_do_nothing() {
    let (mut h, b, me, world) = player();
    h.server.catalog.insert(yalain_testkit::test_template(
        yalain_world::pack::COINS,
        0x1067,
        yalain_content::Props::default(),
    ));
    let room = inventory::PACK_ROOM as usize;
    let have = h.server.players[&world].play.items.len();
    give(&mut h, world, TEST_ITEM, room - have);
    yalain_world::quals::set_int(
        &mut h
            .server
            .players
            .get_mut(&world)
            .expect("in the world")
            .qualities,
        yalain_world::store::MONEY,
        500,
    );
    h.ask(
        b,
        function::FROM_MONEY_BAG,
        &[Arg::LongInt(me), Arg::Int(0), Arg::Int(100)],
        5,
    );
    let p = &h.server.players[&world];
    assert_eq!(p.play.items.len(), room, "no more than it holds");
    assert_eq!(yalain_world::store::money(&p.qualities), 500);
}

/// A gift to a pack holding all its room is held all the same, past it, and its move says it took a
/// place past the room; a move with room says it did not. What comes from a person may always.
/// Behaviour: server.inventory.a-gift-runs-past-a-full-pack
#[test]
fn a_gift_past_a_full_pack_is_held_and_its_move_says_so() {
    let (mut h, b, _me, world) = player();
    let from = h.bots[b].inbox.len();
    inventory::give_from(&mut h.server, &mut h.net, world, TEST_ITEM, 1, 7);
    h.run(2);
    let slots = answers(&h, b, from);
    assert_eq!(
        (slots[0][43].1, slots[0][34].1),
        (1, 0),
        "may, and did not need to"
    );
    let full: Vec<Held> = (0..inventory::PACK_ROOM)
        .map(|n| Held {
            id: 0x8130_0000_0FFF_0000 + u64::from(n),
            template: TEST_ITEM,
            slot: n,
            location: 0,
            quantity: 1,
        })
        .collect();
    h.server.players.get_mut(&world).unwrap().play.items = full;
    let from = h.bots[b].inbox.len();
    inventory::give(&mut h.server, &mut h.net, world, TEST_ITEM, 1);
    inventory::give_from(&mut h.server, &mut h.net, world, TEST_ITEM, 1, 7);
    h.run(2);
    let slots = answers(&h, b, from);
    assert_eq!(slots.len(), 2);
    for m in &slots {
        assert_eq!(
            (m[40].1, m[43].1, m[34].1),
            (0, 1, 1),
            "held, past the room"
        );
    }
    assert_eq!(
        h.server.players[&world].play.items.len(),
        inventory::PACK_ROOM as usize + 2
    );
}

/// A world where this skill is a transmutation perk of this value and priority.
fn perk(h: &mut yalain_testkit::Harness, skill: u32, value: f64, priority: i32) {
    h.server.skill_table.skills.insert(
        skill,
        yalain_world::skills::SkillDef {
            id: skill,
            perk: Some(yalain_world::skills::Perk {
                kind: yalain_world::skills::perk_kind::TRANSMUTATION,
                value,
                priority,
            }),
            ..yalain_world::skills::SkillDef::default()
        },
    );
}

/// A player comes to hold a skill.
fn holds(h: &mut yalain_testkit::Harness, world: yalain_net::ConnId, skill: u32) {
    h.server
        .players
        .get_mut(&world)
        .unwrap()
        .play
        .skills
        .push(yalain_world::state::SkillState {
            skill,
            granted: 0,
            mask: 0,
            cost: 0,
            children: 0,
            dependents: 0,
            spent: 0,
        });
}

/// What dragging this many of the first thing in the pack to the money bag brings.
fn bag_pays(
    h: &mut yalain_testkit::Harness,
    b: usize,
    me: u64,
    world: yalain_net::ConnId,
    n: u32,
) -> i32 {
    yalain_world::quals::set_int(
        &mut h.server.players.get_mut(&world).unwrap().qualities,
        MONEY,
        0,
    );
    let item = pack_order(h, world)[0];
    h.ask(
        b,
        function::TO_MONEY_BAG,
        &[
            Arg::LongInt(me),
            Arg::Int(0),
            Arg::Int(n),
            Arg::LongInt(item),
        ],
        4,
    );
    yalain_world::quals::int(&h.server.players[&world].qualities, MONEY).unwrap_or(0)
}

/// An item sold to the money bag pays each piece's worth times its seller's transmutation perk,
/// rounded down: 25 at 1.03 is 25, 25 at 1.09 is 27, the higher perk held counting. Selling a stack
/// whole pays as selling it piece by piece does.
/// Behaviour: server.inventory.money-bag-pays-a-perk
#[test]
fn an_item_sold_to_the_money_bag_pays_its_sellers_transmutation_perk() {
    let (mut h, b, me, world) = player();
    perk(&mut h, 9001, 1.03, 1);
    perk(&mut h, 9002, 1.09, 3);
    inventory::give(&mut h.server, &mut h.net, world, VALUED, 1);
    h.run(2);
    assert_eq!(bag_pays(&mut h, b, me, world, 1), 25, "no perk: its worth");
    holds(&mut h, world, 9001);
    holds(&mut h, world, 9002);
    inventory::give(&mut h.server, &mut h.net, world, VALUED, 1);
    h.run(2);
    assert_eq!(
        bag_pays(&mut h, b, me, world, 1),
        27,
        "the higher, rounded down"
    );
    h.server
        .players
        .get_mut(&world)
        .unwrap()
        .play
        .skills
        .reverse();
    inventory::give(&mut h.server, &mut h.net, world, VALUED, 1);
    h.run(2);
    assert_eq!(
        bag_pays(&mut h, b, me, world, 1),
        27,
        "whichever came first"
    );
}

/// Coins dragged to the money bag count one each whatever perk their owner holds: what comes out of the
/// bag goes back in for no more.
/// Behaviour: server.inventory.money-bag-pays-a-perk
#[test]
fn coins_dragged_to_the_money_bag_are_not_raised_by_a_perk() {
    let (mut h, b, me, world) = player();
    perk(&mut h, 9001, 1.18, 6);
    holds(&mut h, world, 9001);
    h.server.catalog.insert(yalain_testkit::test_template(
        pack::COINS,
        0x1067,
        yalain_content::Props::default(),
    ));
    inventory::give(&mut h.server, &mut h.net, world, pack::COINS, 1000);
    h.run(2);
    assert_eq!(bag_pays(&mut h, b, me, world, 1000), 1000);
}

/// An item whose template the world no longer has is lost to nobody: its holder enters with it held
/// and kept but unseen, and when the template is back it is there again, the same item in the same
/// number.
/// Behaviour: server.items.template-gone-stays-kept
#[test]
fn an_item_whose_template_left_stays_kept_unseen_and_comes_back_with_it() {
    const SWORD: u32 = 0x4700_051C;
    let mut h = Harness::new();
    let (b, me) = h.player_in_world("one", "Elara");
    h.run(5);
    h.arm(b, me, SWORD);
    let held = |h: &Harness| -> Vec<Held> {
        h.server
            .players
            .values()
            .find(|p| p.character == me)
            .map(|p| p.play.items.clone())
            .expect("in the world")
    };
    let items = held(&h);
    let sword = items
        .iter()
        .find(|i| i.template == SWORD)
        .copied()
        .expect("the sword");
    let log_off = |h: &mut Harness| {
        h.bots[b].send(
            false,
            auberean_protocol::queue::LOGON,
            &auberean_protocol::ClientMessage::LogOff(auberean_protocol::login::CharacterRef {
                character: me,
            }),
        );
        for _ in 0..4000 {
            h.step();
            if !h.server.players.values().any(|p| p.character == me) {
                break;
            }
        }
        h.run(20);
    };
    let sees_it = |h: &Harness, from: usize| {
        h.world_messages(b, from)
            .iter()
            .any(|m| matches!(m, ServerMessage::CreateObject(o) if o.id == sword.id))
    };
    log_off(&mut h);
    let template = h
        .server
        .catalog
        .templates
        .remove(&SWORD)
        .expect("the template");
    let from = h.bots[b].inbox.len();
    h.enter_world(b, "one", me);
    assert!(!sees_it(&h, from), "not shown");
    assert!(h.server.things.get(sword.id).is_none(), "not in the world");
    assert_eq!(held(&h), items, "held");
    // Kept as it is while it is unseen, through another leaving.
    log_off(&mut h);
    h.server.catalog.insert(template);
    let from = h.bots[b].inbox.len();
    h.enter_world(b, "one", me);
    assert!(sees_it(&h, from), "shown again");
    assert_eq!(held(&h), items, "the same, in the same number");
}
