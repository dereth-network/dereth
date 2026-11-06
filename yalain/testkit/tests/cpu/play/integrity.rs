//! Nothing a player holds is lost or made twice: the same item taken two ways, an offer that is no
//! longer held, an ingredient gone from a making, a world stopped in the middle of each.

use auberean_script::args::Arg;
use yalain_net::ConnId;
use yalain_store::Store as _;
use yalain_testkit::play::use_of;
use yalain_testkit::{Harness, STEP, TEST_ITEM};
use yalain_world::inventory;
use yalain_world::state::{Held, PlayState};
use yalain_world::things::{Corpse, Kind, Thing};

const USE: u32 = yalain_world::requests::function::USE;
const CLOSE: u32 = inventory::function::CLOSE;

fn world_of(h: &Harness, id: u64) -> ConnId {
    *h.server
        .players
        .iter()
        .find(|(_, p)| p.character == id)
        .expect("in the world")
        .0
}

/// Stand `who` beside `of`.
fn beside(h: &mut Harness, who: u64, of: u64) {
    let at = h.server.players[&world_of(h, of)].position;
    let w = world_of(h, who);
    let p = h.server.players.get_mut(&w).expect("in the world");
    p.position = at;
    p.position.origin[0] += 0.5;
}

/// A corpse holding `n` test items, beside `me`; the corpse and its items.
fn corpse(h: &mut Harness, me: u64, n: usize) -> (u64, Vec<u64>) {
    corpse_of(h, me, TEST_ITEM, n)
}

/// A corpse holding `n` items of a template, beside `me`; the corpse and its items.
fn corpse_of(h: &mut Harness, me: u64, template: u32, n: usize) -> (u64, Vec<u64>) {
    let t = h.server.catalog.get(TEST_ITEM).cloned().expect("template");
    let id = h.server.things.new_id();
    let mut c = Thing::of(id, &t, Kind::Corpse);
    c.at = Some(h.server.players[&world_of(h, me)].position);
    let items: Vec<u64> = (0..n)
        .map(|i| {
            let item = inventory::make_item(&mut h.server, template, 1, 0).expect("made");
            h.server.things.get_mut(item).expect("item").container =
                Some((id, u32::try_from(i).unwrap_or(0)));
            item
        })
        .collect();
    c.corpse = Some(Corpse {
        killer: me,
        originator: 0,
        contents: items.clone(),
        creature: 0,
        butchered: false,
        killer_level: 1,
        expires: 1.0e12,
    });
    h.server.things.insert(c);
    (id, items)
}

fn players() -> (Harness, (usize, u64), (usize, u64)) {
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    let a = h.player_in_world("one", "Alder");
    let b = h.player_in_world("two", "Birch");
    beside(&mut h, b.1, a.1);
    (h, a, b)
}

fn holds(h: &Harness, who: u64, item: u64) -> usize {
    h.server.players[&world_of(h, who)]
        .play
        .items
        .iter()
        .filter(|x| x.id == item)
        .count()
}

fn steps(seconds: f64) -> usize {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let n = (seconds / STEP).ceil() as usize;
    n
}

/// An item of a template in a player's pack.
fn hold(h: &mut Harness, who: u64, template: u32, quantity: u32) -> u64 {
    let id = inventory::make_item(&mut h.server, template, quantity, 0).expect("made");
    let w = world_of(h, who);
    let p = h.server.players.get_mut(&w).expect("in the world");
    let slot = u32::try_from(p.play.items.len()).unwrap_or(0);
    p.play.items.push(Held {
        id,
        template,
        slot,
        location: 0,
        quantity,
    });
    if let Some(t) = h.server.things.get_mut(id) {
        t.container = Some((who, slot));
    }
    id
}

/// One closes the corpse with its take under way, another opens it and takes the same item: one
/// of them gets it.
/// Behaviour: server.inventory.one-item-one-holder
#[test]
fn an_item_taken_by_two_from_one_corpse_goes_to_one_of_them() {
    let (mut h, (a, ida), (b, idb)) = players();
    let (c, items) = corpse(&mut h, ida, 1);
    let item = items[0];
    h.ask(a, USE, &[use_of(c, ida)], 5);
    h.ask(a, USE, &[use_of(item, ida)], 2);
    h.ask(a, CLOSE, &[Arg::LongInt(c)], 2);
    h.ask(b, USE, &[use_of(c, idb)], 2);
    h.ask(b, USE, &[use_of(item, idb)], 2);
    h.run(steps(inventory::TAKING_SECONDS + 1.0));
    assert_eq!(
        holds(&h, ida, item) + holds(&h, idb, item),
        1,
        "one holder of one item"
    );
}

/// Only the player who has a corpse open closes it: another naming it from anywhere neither ends
/// the viewer's look nor sets the corpse rotting.
/// Behaviour: server.inventory.closing-only-what-the-player-has-open
#[test]
fn a_corpse_another_player_has_open_is_not_closed_by_a_stranger() {
    let (mut h, (a, ida), (b, _)) = players();
    let (c, _) = corpse(&mut h, ida, 1);
    h.ask(a, USE, &[use_of(c, ida)], 5);
    let until = |h: &Harness| {
        h.server
            .things
            .get(c)
            .and_then(|t| t.corpse.as_ref())
            .expect("a corpse")
            .expires
    };
    let before = until(&h);
    let from = h.bots[b].inbox.len();
    h.ask(b, CLOSE, &[Arg::LongInt(c)], 2);
    assert!(!h
        .functions(b, from)
        .contains(&inventory::function::CLOSE_DONE));
    assert!((until(&h) - before).abs() < f64::EPSILON, "no sooner gone");
    assert_eq!(
        inventory::open_corpse_of(&h.server, world_of(&h, ida)),
        Some(c),
        "still open to its viewer"
    );
    h.ask(a, CLOSE, &[Arg::LongInt(c)], 2);
    assert_eq!(
        inventory::open_corpse_of(&h.server, world_of(&h, ida)),
        None
    );
}

/// The request to turn a corpse into money, as the client packs it.
fn transmute_request(corpse: u64) -> Arg {
    use yalain_world::graph::{Builder, Slot};
    let mut g = Builder::new();
    let mut slots = vec![Slot::Int(0); 8];
    slots.push(Slot::Long(corpse));
    slots.extend(std::iter::repeat_n(Slot::Int(0), 6));
    g.object(inventory::TRANSMUTE_DESC, &slots);
    Arg::Object(g.graph())
}

fn money(h: &Harness, who: u64) -> u32 {
    yalain_world::store::money(&h.server.players[&world_of(h, who)].qualities)
}

/// A corpse turned into money by its viewer alone: not by another, whatever it knows of its id,
/// and from anywhere.
/// Behaviour: server.inventory.money-from-a-corpse-open
#[test]
fn a_corpse_the_player_has_not_open_is_not_turned_into_money() {
    let (mut h, (a, ida), (b, idb)) = players();
    let mut props = yalain_content::Props::default();
    props.set("Item_Value", yalain_content::Prop::Int(25));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(TEST_ITEM, 0x1067, props));
    let (c, _) = corpse(&mut h, ida, 2);
    let f = inventory::function::TRANSMUTE_ALL;
    h.ask(b, f, &[transmute_request(c)], 5);
    assert_eq!(money(&h, idb), 0, "not open to it");
    h.ask(a, USE, &[use_of(c, ida)], 5);
    h.ask(b, f, &[transmute_request(c)], 5);
    assert_eq!(money(&h, idb), 0, "open to another");
    h.ask(a, f, &[transmute_request(c)], 5);
    assert_eq!(money(&h, ida), 50, "its viewer's");
}

/// A taker gone before its taking is through leaves the item in the corpse.
/// Behaviour: server.inventory.one-item-one-holder
#[test]
fn an_item_being_taken_by_one_who_leaves_stays_in_the_corpse() {
    let (mut h, (a, ida), _) = players();
    let (c, items) = corpse(&mut h, ida, 1);
    h.ask(a, USE, &[use_of(c, ida)], 5);
    h.ask(a, USE, &[use_of(items[0], ida)], 2);
    let w = world_of(&h, ida);
    yalain_world::world::leave(&mut h.server, &mut h.net, w);
    h.run(steps(inventory::TAKING_SECONDS + 1.0));
    let held = h
        .server
        .things
        .get(c)
        .and_then(|t| t.corpse.as_ref())
        .expect("the corpse");
    assert_eq!(held.contents, items, "still in the corpse");
    assert!(
        h.server.things.get(items[0]).is_some(),
        "still in the world"
    );
}

/// An item taken into a full pack stays where it is, and the taker is told there is no room.
/// Behaviour: server.inventory.one-item-one-holder
#[test]
fn an_item_taken_into_a_full_pack_stays_in_the_corpse() {
    let (mut h, (a, ida), _) = players();
    let (c, items) = corpse(&mut h, ida, 1);
    while inventory::fits_in_pack(&h.server.players[&world_of(&h, ida)].play.items, 1) {
        hold(&mut h, ida, TEST_ITEM, 1);
    }
    h.ask(a, USE, &[use_of(c, ida)], 5);
    h.ask(a, USE, &[use_of(items[0], ida)], 2);
    let from = h.bots[a].inbox.len();
    h.run(steps(inventory::TAKING_SECONDS + 1.0));
    assert_eq!(holds(&h, ida, items[0]), 0, "not over the pack's room");
    let refused = h.calls(a, from).iter().any(|(f, v)| {
        *f == inventory::function::MOVE_DONE
            && v.iter().any(|x| match x.root() {
                Some(yalain_world::calls::Node::Script { slots, .. }) => {
                    slots[40].1 == yalain_world::pack::status::CONTAINER_FULL
                }
                _ => false,
            })
    });
    assert!(refused, "told there is no room");
}

/// An item that stacks needs no place of its own: taken into a pack holding all its room it joins
/// its stack, is gone from the corpse and from the world, and the pack holds no more items than it
/// did. A stack with no room for it leaves it in the corpse.
/// Behaviour: server.inventory.a-taken-item-joins-its-stack
#[test]
fn a_taken_item_joins_its_stack_even_in_a_full_pack() {
    const GEM: u32 = 0x4700_7002;
    let (mut h, (a, ida), _) = players();
    let mut stacks = yalain_content::Props::default();
    stacks.set("Item_MaxStackSize", yalain_content::Prop::Int(5));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(GEM, 0x1067, stacks));
    let gems = hold(&mut h, ida, GEM, 3);
    while inventory::fits_in_pack(&h.server.players[&world_of(&h, ida)].play.items, 1) {
        hold(&mut h, ida, TEST_ITEM, 1);
    }
    let in_pack = h.server.players[&world_of(&h, ida)].play.items.len();
    let (c, items) = corpse_of(&mut h, ida, GEM, 3);
    h.ask(a, USE, &[use_of(c, ida)], 5);
    let quantity = |h: &Harness| {
        h.server.players[&world_of(h, ida)]
            .play
            .items
            .iter()
            .find(|x| x.id == gems)
            .map_or(0, |x| x.quantity)
    };
    let contents = |h: &Harness| {
        h.server
            .things
            .get(c)
            .and_then(|t| t.corpse.as_ref())
            .map_or(0, |x| x.contents.len())
    };
    for (n, item) in items.iter().take(2).enumerate() {
        h.ask(a, USE, &[use_of(*item, ida)], 2);
        h.run(steps(inventory::TAKING_SECONDS + 1.0));
        assert_eq!(quantity(&h), 4 + u32::try_from(n).unwrap_or(0), "joined");
        assert!(h.server.things.get(*item).is_none(), "gone from the world");
        assert_eq!(contents(&h), 2 - n, "gone from the corpse");
    }
    let from = h.bots[a].inbox.len();
    h.ask(a, USE, &[use_of(items[2], ida)], 2);
    h.run(steps(inventory::TAKING_SECONDS + 1.0));
    assert_eq!(quantity(&h), 5, "at its limit");
    assert_eq!(contents(&h), 1, "left where it was");
    assert!(
        h.server.things.get(items[2]).is_some(),
        "still in the world"
    );
    assert_eq!(
        h.server.players[&world_of(&h, ida)].play.items.len(),
        in_pack,
        "no more items held"
    );
    let refused = h.calls(a, from).iter().any(|(f, v)| {
        *f == inventory::function::MOVE_DONE
            && v.iter().any(|x| match x.root() {
                Some(yalain_world::calls::Node::Script { slots, .. }) => {
                    slots[40].1 == yalain_world::pack::status::CONTAINER_FULL
                }
                _ => false,
            })
    });
    assert!(refused, "told there is no room");
}

/// A corpse turned into money by a player whose pack holds all its room still turns: each item only
/// passes through, its move says it ran past the room, and the pack ends as it was.
/// Behaviour: server.inventory.money-from-a-corpse-past-a-full-pack
#[test]
fn a_corpse_turned_into_money_with_a_full_pack_says_it_ran_past_the_room() {
    let (mut h, (a, ida), _) = players();
    let mut props = yalain_content::Props::default();
    props.set("Item_Value", yalain_content::Prop::Int(25));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(TEST_ITEM, 0x1067, props));
    while inventory::fits_in_pack(&h.server.players[&world_of(&h, ida)].play.items, 1) {
        hold(&mut h, ida, TEST_ITEM, 1);
    }
    let in_pack = h.server.players[&world_of(&h, ida)].play.items.len();
    let (c, _) = corpse(&mut h, ida, 2);
    h.ask(a, USE, &[use_of(c, ida)], 5);
    let from = h.bots[a].inbox.len();
    h.ask(
        a,
        inventory::function::TRANSMUTE_ALL,
        &[transmute_request(c)],
        5,
    );
    assert_eq!(money(&h, ida), 50);
    let moves: Vec<(u32, u32)> = h
        .calls(a, from)
        .iter()
        .filter(|(f, _)| *f == inventory::function::MOVE_DONE)
        .filter_map(|(_, v)| {
            v.iter().find_map(|x| match x.root() {
                Some(yalain_world::calls::Node::Script { slots, .. }) => {
                    Some((slots[43].1, slots[34].1))
                }
                _ => None,
            })
        })
        .collect();
    assert_eq!(moves, [(1, 1), (1, 1)], "may, and did");
    assert_eq!(
        h.server.players[&world_of(&h, ida)].play.items.len(),
        in_pack
    );
}

/// An offered item that is gone by the time both have accepted: the trade does not go through for
/// the other side alone.
/// Behaviour: server.trade.an-offer-no-longer-held-moves-nothing
#[test]
fn an_offer_dropped_after_accepting_moves_nothing() {
    use yalain_world::trade::function as tf;
    let (mut h, (a, ida), (b, idb)) = players();
    let bow = hold(&mut h, ida, TEST_ITEM, 1);
    let gold = hold(&mut h, idb, TEST_ITEM, 100);
    h.ask(a, tf::OPEN, &[Arg::LongInt(idb)], 10);
    h.ask(b, tf::OPEN, &[Arg::LongInt(ida)], 10);
    h.ask(a, tf::OFFER, &[Arg::Int(1), Arg::LongInt(bow)], 5);
    h.ask(b, tf::OFFER, &[Arg::Int(100), Arg::LongInt(gold)], 5);
    h.ask(a, tf::ACCEPT, &[], 5);
    // A puts its offer down; its acceptance stands.
    let wa = world_of(&h, ida);
    let m = inventory::Move {
        move_type: yalain_world::drops::DROP,
        item: bow,
        quantity: 1,
        ..inventory::Move::default()
    };
    h.ask(
        a,
        yalain_world::drops::function::MOVE_ITEM,
        &[inventory::move_graph(&m)],
        steps(3.0),
    );
    let _ = wa;
    h.ask(b, tf::ACCEPT, &[], 5);
    assert_eq!(
        holds(&h, ida, gold),
        0,
        "A gets nothing for what it no longer offers"
    );
    assert_eq!(holds(&h, idb, gold), 1, "B keeps its gold");
    let open = &h.server.trades.list[0];
    assert!(open.master_items.is_empty(), "the offer is gone");
    assert!(
        !open.master_accepted && !open.slave_accepted,
        "both must accept again"
    );
    assert_eq!(open.slave_items, vec![(gold, 100)], "the other's stands");
}

/// Part of a stack offered, the stack then split so that less is left: the other is not given less
/// than it accepted.
/// Behaviour: server.trade.an-offer-no-longer-held-moves-nothing
#[test]
fn an_offer_split_after_accepting_moves_nothing() {
    use yalain_world::trade::function as tf;
    let (mut h, (a, ida), (b, idb)) = players();
    let mut props = yalain_content::Props::default();
    props.set("Item_MaxStackSize", yalain_content::Prop::Int(1000));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(0x4700_7001, 0x1067, props));
    let gems = hold(&mut h, ida, 0x4700_7001, 100);
    let gold = hold(&mut h, idb, TEST_ITEM, 1);
    h.ask(a, tf::OPEN, &[Arg::LongInt(idb)], 10);
    h.ask(b, tf::OPEN, &[Arg::LongInt(ida)], 10);
    h.ask(a, tf::OFFER, &[Arg::Int(100), Arg::LongInt(gems)], 5);
    h.ask(b, tf::OFFER, &[Arg::Int(1), Arg::LongInt(gold)], 5);
    h.ask(a, tf::ACCEPT, &[], 5);
    let m = inventory::Move {
        item: gems,
        quantity: 99,
        target: ida,
        from: ida,
        target_slot: 5,
        ..inventory::Move::default()
    };
    h.ask(
        a,
        yalain_world::pack::function::REORGANIZE,
        &[inventory::move_graph(&m)],
        5,
    );
    h.ask(b, tf::ACCEPT, &[], 5);
    assert_eq!(
        holds(&h, ida, gold),
        0,
        "A gets nothing for what it split off"
    );
    assert_eq!(holds(&h, idb, gold), 1);
}

/// A world stopping while an item is being put down: the item is not lost.
/// Behaviour: server.inventory.an-item-being-put-down-is-kept-at-a-stop
#[test]
fn a_world_stopped_while_an_item_is_put_down_keeps_it() {
    let (mut h, (a, ida), _) = players();
    let bow = hold(&mut h, ida, TEST_ITEM, 1);
    let m = inventory::Move {
        move_type: yalain_world::drops::DROP,
        item: bow,
        quantity: 1,
        ..inventory::Move::default()
    };
    h.ask(
        a,
        yalain_world::drops::function::MOVE_ITEM,
        &[inventory::move_graph(&m)],
        2,
    );
    assert_eq!(holds(&h, ida, bow), 0, "under way");
    yalain_world::shutdown::save_everyone(&mut h.server, &mut h.net);
    h.run(2);
    let st = h
        .server
        .store
        .world_state(ida)
        .expect("reads")
        .expect("kept");
    let kept = PlayState::from_bytes(&st.play).expect("reads");
    assert!(
        kept.item_states.iter().any(|k| k.id == bow),
        "with what it carries"
    );
    assert!(
        kept.items.iter().any(|x| x.id == bow),
        "it is back in the pack"
    );
}

/// An item that has landed is out of its dropper's kept pack at once: picked up by another and the
/// server dying, it has one holder.
/// Behaviour: server.inventory.an-item-being-put-down-is-kept-at-a-stop
#[test]
fn an_item_put_down_and_picked_up_by_another_has_one_holder_in_the_store() {
    let (mut h, (a, ida), (b, idb)) = players();
    let bow = hold(&mut h, ida, TEST_ITEM, 1);
    // It is in the pack as the store last had it.
    let w = world_of(&h, ida);
    yalain_world::world::keep_player(&mut h.server, w);
    h.run(2);
    let m = inventory::Move {
        move_type: yalain_world::drops::DROP,
        item: bow,
        quantity: 1,
        ..inventory::Move::default()
    };
    h.ask(
        a,
        yalain_world::drops::function::MOVE_ITEM,
        &[inventory::move_graph(&m)],
        steps(3.0),
    );
    h.ask(b, USE, &[use_of(bow, idb)], steps(3.0));
    assert_eq!(holds(&h, idb, bow), 1, "picked up");
    let kept = |h: &mut Harness, who: u64| -> Vec<u64> {
        let st = h
            .server
            .store
            .world_state(who)
            .expect("reads")
            .expect("kept");
        let play = PlayState::from_bytes(&st.play).expect("reads");
        play.items.iter().map(|x| x.id).collect()
    };
    assert!(!kept(&mut h, ida).contains(&bow), "not the dropper's");
    assert!(kept(&mut h, idb).contains(&bow), "the taker's");
}
