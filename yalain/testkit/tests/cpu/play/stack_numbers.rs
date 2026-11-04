//! An item's own numbers on a stack: each piece worth its own value however it is sold.

use auberean_script::args::Arg;
use yalain_net::ConnId;
use yalain_testkit::{test_catalog, Harness};
use yalain_world::inventory;
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
