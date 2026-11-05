//! Trade between players: asked, opened, offers, acceptance and the exchange; closings.

use auberean_protocol::login::CharacterRef;
use auberean_protocol::{queue, ClientMessage, ServerMessage};
use auberean_script::args::Arg;
use yalain_testkit::play::STEPS;
use yalain_testkit::{test_template, Harness};
use yalain_world::calls::Value;
use yalain_world::trade::{error, function, text};

const BOW: u32 = 0x4700_0184;
const COINS: u32 = 0x4700_06C1;

fn beside(h: &mut Harness, who: u64, of: u64, dx: f32) {
    let at = h
        .server
        .players
        .values()
        .find(|p| p.character == of)
        .expect("in the world")
        .position;
    let p = h
        .server
        .players
        .values_mut()
        .find(|p| p.character == who)
        .expect("in the world");
    p.position = at;
    p.position.origin[0] += dx;
}

/// An item of a template in a player's pack, at its end.
fn hold(h: &mut Harness, character: u64, template: u32, quantity: u32) -> u64 {
    if h.server.catalog.get(template).is_none() {
        h.server.catalog.insert(test_template(
            template,
            0x1067,
            yalain_content::Props::default(),
        ));
    }
    let id = yalain_world::inventory::make_item(&mut h.server, template, quantity, 0)
        .expect("in the catalog");
    let p = h
        .server
        .players
        .values_mut()
        .find(|p| p.character == character)
        .expect("in the world");
    let slot = u32::try_from(p.play.items.len()).unwrap_or(0);
    p.play.items.push(yalain_world::state::Held {
        id,
        template,
        slot,
        location: 0,
        quantity,
    });
    if let Some(t) = h.server.things.get_mut(id) {
        t.container = Some((character, slot));
    }
    id
}

fn two() -> (Harness, (usize, u64), (usize, u64)) {
    let mut h = Harness::new();
    let a = h.player_in_world("one", "Trader");
    let b = h.player_in_world("two", "Buyer");
    beside(&mut h, b.1, a.1, 1.0);
    (h, a, b)
}

/// A trade asked of a player beside: both told of it; the other asking back opens it for both.
fn opened(h: &mut Harness, (a, aid): (usize, u64), (b, bid): (usize, u64)) {
    h.ask(a, function::OPEN, &[Arg::LongInt(bid)], 10);
    h.ask(b, function::OPEN, &[Arg::LongInt(aid)], 10);
}

fn pack(h: &Harness, character: u64) -> Vec<(u64, u32, u32)> {
    let p = h
        .server
        .players
        .values()
        .find(|p| p.character == character)
        .expect("in the world");
    let mut v: Vec<(u64, u32, u32)> = p
        .play
        .items
        .iter()
        .filter(|x| x.location == 0)
        .map(|x| (x.id, x.slot, x.quantity))
        .collect();
    v.sort_by_key(|x| x.1);
    v
}

/// Asking a player beside for a trade tells both of it (the asker first, the other second, each
/// with the other's name); the other asking back opens it for both.
/// Behaviour: server.trade.asked-and-opened
#[test]
fn a_trade_asked_is_told_to_both_and_asking_back_opens_it() {
    let (mut h, (a, aid), (b, bid)) = two();
    let (fa, fb) = (h.bots[a].inbox.len(), h.bots[b].inbox.len());
    h.ask(a, function::OPEN, &[Arg::LongInt(bid)], 10);
    for (x, from) in [(a, fa), (b, fb)] {
        let c = h.calls(x, from);
        let (_, v) = c
            .iter()
            .find(|(f, _)| *f == function::REGISTERED)
            .expect("told of the trade");
        let ids: Vec<u64> = v
            .iter()
            .filter_map(|x| match x {
                Value::Long(l) => Some(*l),
                _ => None,
            })
            .collect();
        assert_eq!(
            ids,
            vec![bid, aid],
            "the one asked, then the asker (last first)"
        );
    }
    let (fa, fb) = (h.bots[a].inbox.len(), h.bots[b].inbox.len());
    h.ask(b, function::OPEN, &[Arg::LongInt(aid)], 10);
    assert!(h.functions(a, fa).contains(&function::OPENED));
    assert!(h.functions(b, fb).contains(&function::OPENED));
}

/// An offer is told to both and the other side made aware of the item; offering it again is
/// refused (the refusal to the trade's asker); with both accepting, the item moves to the front of
/// the other's pack, the move told to both, the trade reset and both told it finished; the trade
/// stays open.
/// Behaviour: server.trade.offered-accepted-exchanged
#[test]
fn both_accepting_moves_the_offered_item_to_the_front_of_the_others_pack() {
    let (mut h, (a, aid), (b, bid)) = two();
    let kept = hold(&mut h, bid, COINS, 5);
    let bow = hold(&mut h, aid, BOW, 1);
    opened(&mut h, (a, aid), (b, bid));
    let (fa, fb) = (h.bots[a].inbox.len(), h.bots[b].inbox.len());
    h.ask(a, function::OFFER, &[Arg::Int(1), Arg::LongInt(bow)], 10);
    assert!(h.functions(a, fa).contains(&function::OFFERED));
    assert!(h.functions(b, fb).contains(&function::OFFERED));
    assert!(
        h.world_messages(b, fb)
            .iter()
            .any(|m| matches!(m, ServerMessage::CreateObject(o) if o.id == bow)),
        "the other side learns the item"
    );
    let fa = h.bots[a].inbox.len();
    h.ask(a, function::OFFER, &[Arg::Int(1), Arg::LongInt(bow)], 10);
    assert_eq!(h.texts(a, fa), vec![(1, text::OFFER_FAILED.0)]);
    let (fa, fb) = (h.bots[a].inbox.len(), h.bots[b].inbox.len());
    h.ask(a, function::ACCEPT, &[], 10);
    h.ask(b, function::ACCEPT, &[], 10);
    for (x, from) in [(a, fa), (b, fb)] {
        let f = h.functions(x, from);
        let at = |g: u32| f.iter().position(|y| *y == g);
        let moved = at(0x820E_005E).expect("the move");
        let reset = at(function::RESET).expect("reset");
        assert!(moved < reset, "{f:x?}");
        let t = h.texts(x, from);
        assert!(t.contains(&(8, text::TRADING.0)) && t.contains(&(8, text::FINISHED.0)));
    }
    assert!(
        pack(&h, aid).iter().all(|x| x.0 != bow),
        "gone from the giver"
    );
    let p = pack(&h, bid);
    assert_eq!(p[0].0, bow, "at the front of the receiver's pack");
    assert!(
        p.iter().any(|x| x.0 == kept && x.1 == 1),
        "the rest moved down"
    );
    assert!(h.server.trades.list[0].open, "still open");
}

/// When both have accepted and the other's pack has no room for what it would receive, both are
/// told the trade failed for the other's want of room; nothing moves, and once the other has made
/// room both must accept again and the item moves.
/// With the pack itself full and room in a bag, a traded item goes to the front of the first bag
/// with room, as any item arriving does, and no two items share a slot.
/// Behaviour: server.trade.offered-accepted-exchanged
#[test]
fn an_item_traded_into_a_full_pack_goes_to_the_first_bag_with_room() {
    let (mut h, (a, aid), (b, bid)) = two();
    while pack(&h, bid).len() < 43 {
        hold(&mut h, bid, COINS, 1);
    }
    let was_in_the_bag = pack(&h, bid)[42].0;
    let bow = hold(&mut h, aid, BOW, 1);
    opened(&mut h, (a, aid), (b, bid));
    h.ask(a, function::OFFER, &[Arg::Int(1), Arg::LongInt(bow)], 10);
    h.ask(a, function::ACCEPT, &[], 10);
    let from = h.bots[b].inbox.len();
    h.ask(b, function::ACCEPT, &[], 10);
    let held = pack(&h, bid);
    assert_eq!(held.len(), 44, "nothing lost");
    let slots: Vec<u32> = held.iter().map(|x| x.1).collect();
    assert_eq!(
        slots,
        (0..44).collect::<Vec<u32>>(),
        "each in a slot of its own"
    );
    assert_eq!(held[42].0, bow, "at the front of the first bag");
    assert_eq!(held[43].0, was_in_the_bag, "what was there moved up");
    let t = h.server.things.get(bow).expect("in the world");
    assert_eq!(t.container, Some((bid, 42)));
    let (_, told) = h
        .calls(b, from)
        .into_iter()
        .find(|(f, _)| *f == yalain_world::inventory::function::MOVE_DONE)
        .expect("told of the move");
    let Some(Value::Graph(g)) = told.first() else {
        panic!("a move's description: {told:?}");
    };
    let Some((_, yalain_world::calls::Node::Script { slots, .. })) = g.first() else {
        panic!("a move's description: {g:?}");
    };
    assert_eq!(
        (slots[11].1, slots[24].1),
        (42, 42),
        "told the slot it went to"
    );
}

/// Behaviour: server.trade.no-room-fails-the-trade
#[test]
fn a_pack_without_room_for_the_offer_fails_the_trade_and_nothing_moves() {
    let (mut h, (a, aid), (b, bid)) = two();
    let filler: Vec<u64> = (0..yalain_world::inventory::PACK_ROOM)
        .map(|_| hold(&mut h, bid, COINS, 1))
        .collect();
    let bow = hold(&mut h, aid, BOW, 1);
    opened(&mut h, (a, aid), (b, bid));
    h.ask(a, function::OFFER, &[Arg::Int(1), Arg::LongInt(bow)], 10);
    let (fa, fb) = (h.bots[a].inbox.len(), h.bots[b].inbox.len());
    h.ask(a, function::ACCEPT, &[], 10);
    h.ask(b, function::ACCEPT, &[], 10);
    for (x, from) in [(a, fa), (b, fb)] {
        let c = h.calls(x, from);
        let failed = c
            .iter()
            .position(|(f, _)| *f == function::FAILED)
            .expect("the trade fails");
        assert!(c[failed].1.contains(&Value::Int(error::OTHER_NO_ROOM)));
        assert!(
            c[..failed].iter().any(|(f, _)| *f == function::ACCEPTED),
            "the acceptance first"
        );
        let f = h.functions(x, from);
        assert!(
            !f.contains(&0x820E_005E) && !f.contains(&function::RESET),
            "{f:x?}"
        );
        assert!(h.texts(x, from).is_empty(), "no text from the server");
    }
    assert!(pack(&h, aid).iter().any(|x| x.0 == bow), "nothing moved");
    assert!(h.server.trades.list[0].open, "still open");
    // Room made: both accept again and the bow moves.
    if let Some(p) = h.server.players.values_mut().find(|p| p.character == bid) {
        p.play.items.retain(|x| x.id != filler[0]);
    }
    h.ask(a, function::ACCEPT, &[], 10);
    h.ask(b, function::ACCEPT, &[], 10);
    assert!(
        pack(&h, bid).iter().any(|x| x.0 == bow),
        "received once there is room"
    );
}

/// An attuned item offered stops the trade once both accept: both told items are traded, its move
/// refused, the trade reset and the trouble; nothing moves, the rest of the offers with it; the
/// trade stays open, and what may be traded then goes.
/// Behaviour: server.trade.an-attuned-item-is-refused
#[test]
fn an_attuned_item_offered_fails_the_trade_and_nothing_moves() {
    const BOUND: u32 = 0x4700_7A40;
    let (mut h, (a, aid), (b, bid)) = two();
    let mut props = yalain_content::Props::default();
    props.set("Item_IsAttunable", yalain_content::Prop::Bool(true));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(BOUND, 0x200, props));
    let bow = hold(&mut h, aid, BOW, 1);
    let bound = hold(&mut h, aid, BOUND, 1);
    opened(&mut h, (a, aid), (b, bid));
    h.ask(a, function::OFFER, &[Arg::Int(1), Arg::LongInt(bow)], 10);
    h.ask(a, function::OFFER, &[Arg::Int(1), Arg::LongInt(bound)], 10);
    let (fa, fb) = (h.bots[a].inbox.len(), h.bots[b].inbox.len());
    h.ask(a, function::ACCEPT, &[], 10);
    h.ask(b, function::ACCEPT, &[], 10);
    for (x, from) in [(a, fa), (b, fb)] {
        let f = h.functions(x, from);
        assert_eq!(f.iter().filter(|c| **c == 0x820E_005E).count(), 1, "{f:x?}");
        let calls = h.calls(x, from);
        let moved = calls.iter().find(|(c, _)| *c == 0x820E_005E).expect("told");
        let told = moved.1.iter().find_map(|v| match v.root() {
            Some(yalain_world::calls::Node::Script { slots, .. }) => {
                Some((slots[40].1, slots[20].1))
            }
            _ => None,
        });
        assert_eq!(
            told,
            Some((yalain_world::drops::status::ATTUNED, 1)),
            "refused, as a trade's"
        );
        assert!(f.contains(&function::RESET), "{f:x?}");
        let t = h.texts(x, from);
        assert_eq!(t.len(), 2, "{t:?}");
        assert_eq!(t[0], (8, yalain_world::trade::text::TRADING.0));
        assert_eq!(t[1], (1, yalain_world::trade::text::PROBLEM.0));
    }
    let held = pack(&h, aid);
    assert!(held.iter().any(|x| x.0 == bow), "nothing moved");
    assert!(held.iter().any(|x| x.0 == bound), "nothing moved");
    assert!(pack(&h, bid).iter().all(|x| x.0 != bow && x.0 != bound));
    let t = &h.server.trades.list[0];
    assert!(t.open, "still open");
    assert!(t.master_items.is_empty() && t.slave_items.is_empty());
    // The bow alone goes.
    h.ask(a, function::OFFER, &[Arg::Int(1), Arg::LongInt(bow)], 10);
    h.ask(a, function::ACCEPT, &[], 10);
    h.ask(b, function::ACCEPT, &[], 10);
    assert!(pack(&h, bid).iter().any(|x| x.0 == bow), "the bow went");
    assert!(
        pack(&h, aid).iter().any(|x| x.0 == bound),
        "the other stayed"
    );
}

/// Part of a stack offered moves as a new item; the rest stays with the giver.
/// Behaviour: server.trade.offered-accepted-exchanged
#[test]
fn part_of_a_stack_moves_as_a_new_item_and_the_rest_stays() {
    let (mut h, (a, aid), (b, bid)) = two();
    let coins = hold(&mut h, aid, COINS, 181);
    opened(&mut h, (a, aid), (b, bid));
    h.ask(
        a,
        function::OFFER,
        &[Arg::Int(150), Arg::LongInt(coins)],
        10,
    );
    h.ask(a, function::ACCEPT, &[], 10);
    h.ask(b, function::ACCEPT, &[], 10);
    assert!(pack(&h, aid).iter().any(|x| x.0 == coins && x.2 == 31));
    let p = pack(&h, bid);
    assert!(p[0].0 != coins && p[0].2 == 150, "{p:?}");
}

/// Part of a stack offered holds what the stack held of its own.
/// Behaviour: server.inventory.a-part-split-off-holds-the-stacks-numbers
#[test]
fn part_of_a_stack_traded_keeps_the_stacks_numbers() {
    let (mut h, (a, aid), (b, bid)) = two();
    let coins = hold(&mut h, aid, COINS, 5);
    {
        let t = h.server.things.get_mut(coins).expect("held");
        t.value = 41;
        t.own.min_level = Some(9);
    }
    opened(&mut h, (a, aid), (b, bid));
    h.ask(a, function::OFFER, &[Arg::Int(2), Arg::LongInt(coins)], 10);
    h.ask(a, function::ACCEPT, &[], 10);
    h.ask(b, function::ACCEPT, &[], 10);
    let p = pack(&h, bid);
    assert_eq!(p[0].2, 2, "{p:?}");
    let t = h.server.things.get(p[0].0).expect("traded");
    assert_eq!((t.value, t.own.min_level), (41, Some(9)));
    assert_eq!(h.server.things.get(coins).expect("kept").quantity, 3);
}

/// A changed offer clears both acceptances: an acceptance before the change and one after do not
/// trade.
/// Behaviour: server.trade.offered-accepted-exchanged
#[test]
fn a_changed_offer_clears_an_earlier_acceptance() {
    let (mut h, (a, aid), (b, bid)) = two();
    let bow = hold(&mut h, aid, BOW, 1);
    let coins = hold(&mut h, bid, COINS, 3);
    opened(&mut h, (a, aid), (b, bid));
    h.ask(a, function::OFFER, &[Arg::Int(1), Arg::LongInt(bow)], 10);
    h.ask(a, function::ACCEPT, &[], 10);
    h.ask(b, function::OFFER, &[Arg::Int(3), Arg::LongInt(coins)], 10);
    let fb = h.bots[b].inbox.len();
    h.ask(b, function::ACCEPT, &[], 10);
    assert!(!h.functions(b, fb).contains(&function::RESET), "not traded");
    h.ask(a, function::ACCEPT, &[], 10);
    assert_eq!(pack(&h, bid)[0].0, bow);
    assert_eq!(pack(&h, aid)[0].0, coins);
}

/// Closing tells both who closed it; walking apart closes it as too far; leaving the world closes
/// it for the other.
/// Behaviour: server.trade.closed
#[test]
fn a_trade_is_closed_by_either_side_by_distance_and_by_leaving() {
    let closed_by = |h: &Harness, x: usize, from: usize| -> Option<Vec<Value>> {
        h.calls(x, from)
            .into_iter()
            .find(|(f, _)| *f == function::CLOSED)
            .map(|(_, v)| v)
    };
    // Closed by the one asked.
    let (mut h, (a, aid), (b, bid)) = two();
    opened(&mut h, (a, aid), (b, bid));
    let fa = h.bots[a].inbox.len();
    h.ask(b, function::CLOSE, &[], 10);
    let v = closed_by(&h, a, fa).expect("told");
    assert!(v.contains(&Value::Long(bid)) && v.contains(&Value::Int(0)));
    // Too far apart.
    opened(&mut h, (a, aid), (b, bid));
    beside(&mut h, bid, aid, 12.0);
    let fb = h.bots[b].inbox.len();
    h.run(30);
    let v = closed_by(&h, b, fb).expect("told");
    assert!(v.contains(&Value::Int(error::TOO_FAR)) && v.contains(&Value::Long(aid)));
    // Leaving the world.
    beside(&mut h, bid, aid, 1.0);
    opened(&mut h, (a, aid), (b, bid));
    let fb = h.bots[b].inbox.len();
    let before = h.bots[a].inbox.len();
    h.bots[a].send(
        false,
        queue::LOGON,
        &ClientMessage::LogOff(CharacterRef { character: aid }),
    );
    assert!(h.run_until(STEPS, |h| h.bots[a].inbox[before..]
        .iter()
        .any(|r| matches!(r.message(), Some(ServerMessage::CharacterList(_))))));
    let v = closed_by(&h, b, fb).expect("told");
    assert!(v.contains(&Value::Int(error::LOGGED_OUT)) && v.contains(&Value::Long(aid)));
    assert!(h.server.trades.list.is_empty());
}

/// A bound item is no attuned one: it may be traded, and whom it is bound to goes with it, so its
/// new holder may not equip it, and its examination says so. A recipe's attunement stops the trade as a template's does.
/// Behaviour: server.trade.a-bound-item-may-be-traded
#[test]
fn a_bound_item_goes_in_a_trade_still_bound_and_an_attuned_one_stops_it() {
    const WORN: u32 = 0x4700_7A42;
    let (mut h, (a, aid), (b, bid)) = two();
    let bound = hold(&mut h, aid, WORN, 1);
    h.server.things.get_mut(bound).expect("made").own.bound_to = aid;
    let attuned = hold(&mut h, aid, WORN, 1);
    h.server.things.get_mut(attuned).expect("made").own.attuned = true;
    opened(&mut h, (a, aid), (b, bid));
    h.ask(
        a,
        function::OFFER,
        &[Arg::Int(1), Arg::LongInt(attuned)],
        10,
    );
    h.ask(a, function::ACCEPT, &[], 10);
    h.ask(b, function::ACCEPT, &[], 10);
    assert!(pack(&h, bid).iter().all(|(i, _, _)| *i != attuned));
    h.ask(a, function::OFFER, &[Arg::Int(1), Arg::LongInt(bound)], 10);
    h.ask(a, function::ACCEPT, &[], 10);
    h.ask(b, function::ACCEPT, &[], 10);
    assert!(pack(&h, bid).iter().any(|(i, _, _)| *i == bound));
    assert_eq!(
        h.server.things.get(bound).map(|t| t.own.bound_to),
        Some(aid)
    );
}

/// An item under an effect that attunes it stops a trade like one attuned for good, and goes when the
/// effect has run out.
/// Behaviour: server.trade.an-item-under-an-attuning-effect-stays
#[test]
fn an_item_under_an_attuning_effect_stops_a_trade_until_it_ends() {
    const STATE: u32 = 0x7000_7A02;
    const WORN: u32 = 0x4700_7A42;
    let (mut h, (a, aid), (b, bid)) = two();
    h.server
        .effect_table
        .effects
        .insert(STATE, yalain_testkit::attuning_effect(STATE));
    let item = hold(&mut h, aid, WORN, 1);
    let until = h.server.game_now + 100.0;
    h.server
        .things
        .get_mut(item)
        .expect("made")
        .timed
        .push(yalain_world::things::TimedEffect {
            state: STATE,
            spellcraft: 1.0,
            until,
        });
    opened(&mut h, (a, aid), (b, bid));
    let trade = |h: &mut Harness| {
        h.ask(a, function::OFFER, &[Arg::Int(1), Arg::LongInt(item)], 10);
        h.ask(a, function::ACCEPT, &[], 10);
        h.ask(b, function::ACCEPT, &[], 10);
    };
    trade(&mut h);
    assert!(pack(&h, bid).iter().all(|(i, _, _)| *i != item), "refused");
    assert!(pack(&h, aid).iter().any(|(i, _, _)| *i == item), "kept");
    h.advance(101.0);
    h.run(1);
    trade(&mut h);
    assert!(
        pack(&h, bid).iter().any(|(i, _, _)| *i == item),
        "gone over"
    );
    assert!(pack(&h, aid).iter().all(|(i, _, _)| *i != item));
}

/// An effect on an item goes with it to its new holder, on its own time: what it raises is raised
/// for the new holder until then and no longer, and the item is one item throughout.
/// Behaviour: server.skills.an-effect-on-an-item-ends-and-does-not-stack
#[test]
fn an_effect_on_a_traded_item_keeps_its_time_and_ends_for_the_new_holder() {
    use yalain_world::effect_defs::{EffectDef, Kind, Scale};
    const STATE: u32 = 0x7000_7A04;
    const SWORD: u32 = 0x4700_7A43;
    let (mut h, (a, aid), (b, bid)) = two();
    let mut props = yalain_content::Props::default();
    props.set("Weapon_Damage", yalain_content::Prop::Int(92));
    h.server.catalog.insert(test_template(SWORD, 0x1067, props));
    h.server.effect_table.effects.insert(
        STATE,
        EffectDef {
            kind: Kind::Numeric {
                flags: 166,
                stats: vec![yalain_world::made::stat::DAMAGE],
                magnitude: Scale(vec![(0.0, 61.0)]),
            },
            duration: Scale(vec![(0.0, 60.0)]),
            eq_class: 31,
            ..yalain_testkit::plain_effect(STATE)
        },
    );
    let item = hold(&mut h, aid, SWORD, 1);
    let until = h.server.game_now + 60.0;
    h.server
        .things
        .get_mut(item)
        .expect("made")
        .timed
        .push(yalain_world::things::TimedEffect {
            state: STATE,
            spellcraft: 30.0,
            until,
        });
    opened(&mut h, (a, aid), (b, bid));
    h.ask(a, function::OFFER, &[Arg::Int(1), Arg::LongInt(item)], 10);
    h.ask(a, function::ACCEPT, &[], 10);
    h.ask(b, function::ACCEPT, &[], 10);
    let most = |h: &Harness| {
        yalain_world::combat::weapon_damage(&h.server, item)
            .expect("a weapon")
            .most
    };
    assert!(pack(&h, bid).iter().any(|(i, _, _)| *i == item));
    assert!(pack(&h, aid).iter().all(|(i, _, _)| *i != item));
    assert_eq!(most(&h), 92 + 61);
    assert_eq!(h.server.things.get(item).expect("it").timed[0].until, until);
    h.advance(61.0);
    h.run(1);
    assert_eq!(most(&h), 92, "over, for its new holder too");
    assert!(pack(&h, bid).iter().any(|(i, _, _)| *i == item));
}

/// A trade cannot complete while a use under way counts on a side's pack: it fails, nothing moves,
/// and the two may accept again.
/// Behaviour: server.trade.waits-for-a-use-under-way
#[test]
fn a_trade_fails_while_a_trip_counts_on_a_sides_pack_and_nothing_moves() {
    let (mut h, (a, aid), (b, bid)) = two();
    let bow = hold(&mut h, aid, BOW, 1);
    opened(&mut h, (a, aid), (b, bid));
    h.ask(a, function::OFFER, &[Arg::Int(1), Arg::LongInt(bow)], 10);
    h.ask(b, function::ACCEPT, &[], 10);
    let world = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == aid)
        .expect("in")
        .0;
    let to = h.server.players[&world].position;
    h.server.players.get_mut(&world).expect("in").trip = Some(yalain_world::places::Trip {
        to,
        used: None,
        step: None,
        resurrect: false,
        sent: false,
        said: None,
    });
    let from = h.bots[a].inbox.len();
    h.ask(a, function::ACCEPT, &[], 10);
    assert!(h.functions(a, from).contains(&function::FAILED), "it fails");
    assert!(pack(&h, aid).iter().any(|i| i.0 == bow), "nothing moved");
    assert!(!pack(&h, bid).iter().any(|i| i.0 == bow), "nothing moved");
}
