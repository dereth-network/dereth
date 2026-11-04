//! An item's own numbers on a stack: each piece worth its own value however it is sold, a stack
//! joined only by what carries its numbers, and a part split off holding all the stack holds.

use auberean_script::args::Arg;
use yalain_net::ConnId;
use yalain_testkit::{test_catalog, Harness};
use yalain_world::inventory::{self, move_graph, Move};
use yalain_world::pack::function;

const STACKING: u32 = 0x4700_2109;

fn player() -> (Harness, usize, u64, ConnId) {
    let mut h = Harness::new();
    h.server.catalog = test_catalog();
    let mut props = yalain_content::Props::default();
    props.set("Item_MaxStackSize", yalain_content::Prop::Int(1000));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(STACKING, 0x200, props));
    let (b, me) = h.player_in_world("one", "Elara");
    let world = *h.server.players.keys().next().expect("a player");
    h.server.players.get_mut(&world).unwrap().play.items.clear();
    (h, b, me, world)
}

/// A stack in the pack, each piece worth `value`.
fn valued(h: &mut Harness, world: ConnId, n: u32, value: u32) -> u64 {
    let id = inventory::give(&mut h.server, &mut h.net, world, STACKING, n).expect("given");
    h.server.things.get_mut(id).expect("made").value = value;
    h.run(2);
    id
}

fn money(h: &Harness, world: ConnId) -> u32 {
    yalain_world::store::money(&h.server.players[&world].qualities)
}

fn sell(h: &mut Harness, b: usize, me: u64, item: u64, n: u32) {
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
}

fn slot_of(h: &Harness, world: ConnId, item: u64) -> i32 {
    let slot = h.server.players[&world]
        .play
        .items
        .iter()
        .find(|x| x.id == item)
        .expect("held")
        .slot;
    i32::try_from(slot).expect("a slot")
}

/// `n` of `item` moved onto the slot of `onto`, as the clients ask it.
fn move_onto(h: &mut Harness, b: usize, me: u64, world: ConnId, item: u64, n: u32, onto: u64) {
    let (from, target) = (slot_of(h, world, item), slot_of(h, world, onto));
    // The slot named counts the places after the item has left its own.
    let target = if from < target { target - 1 } else { target };
    let m = move_graph(&Move {
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
        quantity_left: n,
        quantity: n,
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
    });
    h.ask(b, function::REORGANIZE, &[m], 4);
}

fn quantities(h: &Harness, world: ConnId) -> Vec<(u64, u32)> {
    let mut v: Vec<(u64, u32)> = h.server.players[&world]
        .play
        .items
        .iter()
        .map(|x| (x.id, x.quantity))
        .collect();
    v.sort_unstable();
    v
}

/// Each piece of a stack with a value of its own brings that value: sold whole or piece by piece,
/// it pays the same.
/// Behaviour: server.inventory.a-stack-pays-for-each-piece
#[test]
fn a_stack_with_its_own_value_pays_the_same_whole_or_piece_by_piece() {
    let (mut h, b, me, world) = player();
    let stack = valued(&mut h, world, 3, 40);
    for _ in 0..3 {
        sell(&mut h, b, me, stack, 1);
    }
    assert_eq!(money(&h, world), 120, "piece by piece");
    assert!(h.server.players[&world].play.items.is_empty(), "all sold");
    let (mut h, b, me, world) = player();
    let stack = valued(&mut h, world, 3, 40);
    sell(&mut h, b, me, stack, 3);
    assert_eq!(money(&h, world), 120, "whole");
}

/// A piece worth little moved onto a piece worth much does not join it, and the two bring what they
/// were each worth.
/// Behaviour: server.inventory.stacks-join-only-with-like-numbers
#[test]
fn a_piece_worth_little_does_not_join_a_piece_worth_much() {
    let (mut h, b, me, world) = player();
    let dear = valued(&mut h, world, 1, 1000);
    let cheap = valued(&mut h, world, 1, 6);
    move_onto(&mut h, b, me, world, cheap, 1, dear);
    let mut both = [(dear, 1), (cheap, 1)];
    both.sort_unstable();
    assert_eq!(quantities(&h, world), both, "two items still");
    sell(&mut h, b, me, dear, 1);
    sell(&mut h, b, me, cheap, 1);
    assert_eq!(money(&h, world), 1006);
}

/// Pieces carrying the same numbers still join.
/// Behaviour: server.inventory.stacks-join-only-with-like-numbers
#[test]
fn pieces_worth_the_same_join() {
    let (mut h, b, me, world) = player();
    let kept = valued(&mut h, world, 2, 40);
    let moved = valued(&mut h, world, 1, 40);
    move_onto(&mut h, b, me, world, moved, 1, kept);
    assert_eq!(quantities(&h, world), [(kept, 3)]);
}

/// An inscribed piece keeps its words to itself: nothing joins it, and it joins nothing.
/// Behaviour: server.inventory.stacks-join-only-with-like-numbers
#[test]
fn an_inscribed_piece_does_not_join_a_plain_stack() {
    let (mut h, b, me, world) = player();
    let plain = valued(&mut h, world, 2, 0);
    let words = valued(&mut h, world, 1, 0);
    h.server.things.get_mut(words).expect("made").inscription =
        Some(("Mine".to_owned(), "Elara".to_owned()));
    move_onto(&mut h, b, me, world, words, 1, plain);
    let mut both = [(plain, 2), (words, 1)];
    both.sort_unstable();
    assert_eq!(quantities(&h, world), both);
}

/// Units of a template gathered go to a plain stack of it, never one with numbers of its own.
/// Behaviour: server.inventory.stacks-join-only-with-like-numbers
#[test]
fn units_gathered_do_not_join_a_stack_with_its_own_value() {
    let (mut h, _, _, world) = player();
    let dear = valued(&mut h, world, 1, 1000);
    inventory::give_stacked(&mut h.server, &mut h.net, world, STACKING, 5);
    let q = quantities(&h, world);
    assert!(q.contains(&(dear, 1)), "{q:?}");
    let plain = q
        .iter()
        .find(|x| x.0 != dear)
        .expect("a stack of its own")
        .0;
    inventory::give_stacked(&mut h.server, &mut h.net, world, STACKING, 5);
    assert!(quantities(&h, world).contains(&(plain, 10)), "and it grows");
}

/// Units gathered fill a stack to its template's limit and the rest make new stacks, none past the
/// limit, and not a unit is lost.
/// Behaviour: server.inventory.units-gathered-stay-within-a-stacks-limit
#[test]
fn units_gathered_fill_a_stack_to_its_limit_and_the_rest_make_new_stacks() {
    let (mut h, _, _, world) = player();
    inventory::give_stacked(&mut h.server, &mut h.net, world, STACKING, 995);
    let first = quantities(&h, world)[0].0;
    inventory::give_stacked(&mut h.server, &mut h.net, world, STACKING, 10);
    let q = quantities(&h, world);
    assert!(q.contains(&(first, 1000)), "{q:?}");
    assert_eq!(q.iter().map(|x| x.1).sum::<u32>(), 1005, "{q:?}");
    assert_eq!(q.len(), 2, "{q:?}");
    inventory::give_stacked(&mut h.server, &mut h.net, world, STACKING, 2500);
    let q = quantities(&h, world);
    assert_eq!(q.iter().map(|x| x.1).sum::<u32>(), 3505, "{q:?}");
    assert!(q.iter().all(|x| x.1 <= 1000), "{q:?}");
    assert_eq!(q.len(), 4, "the full ones are left as they are: {q:?}");
    for (id, n) in q {
        assert_eq!(h.server.things.get(id).expect("made").quantity, n);
    }
}

/// Part of a stack split off in the pack holds all the stack held, and the pieces together pay what
/// they did.
/// Behaviour: server.inventory.a-part-split-off-holds-the-stacks-numbers
#[test]
fn a_part_split_off_in_the_pack_keeps_the_stacks_numbers() {
    let (mut h, b, me, world) = player();
    let stack = valued(&mut h, world, 5, 40);
    h.server.things.get_mut(stack).expect("made").own.min_level = Some(9);
    let from = slot_of(&h, world, stack);
    let m = move_graph(&Move {
        merge: true,
        actual_from_slot: from,
        target: me,
        actual_target: me,
        unlock: true,
        target_slot: 4,
        check_take: true,
        from: me,
        all_taken: true,
        actual_target_slot: 4,
        quantity_left: 2,
        quantity: 2,
        split_item: stack,
        merge_container: me,
        grab: true,
        actual_from: me,
        from_slot: from,
        item: stack,
        contain: true,
        auto_merge: true,
        merge_slot: 4,
        ..Move::default()
    });
    h.ask(b, function::REORGANIZE, &[m], 4);
    let q = quantities(&h, world);
    let part = q.iter().find(|x| x.0 != stack).expect("a new stack").0;
    assert!(q.contains(&(stack, 3)) && q.contains(&(part, 2)), "{q:?}");
    let t = h.server.things.get(part).expect("in the world");
    assert_eq!((t.value, t.own.min_level), (40, Some(9)));
    sell(&mut h, b, me, stack, 3);
    sell(&mut h, b, me, part, 2);
    assert_eq!(money(&h, world), 200);
}
