//! Who may loot a corpse: its killer and the killer's fellows, until the first opens it or two
//! minutes after the death; every way of taking from it asks.

use auberean_protocol::login::CharacterRef;
use auberean_protocol::{queue, ClientMessage, ServerMessage};
use auberean_script::args::Arg;
use yalain_testkit::play::{server_call, use_of};
use yalain_testkit::Harness;
use yalain_world::calls::{self, Node, Value};
use yalain_world::fellowship::function as fellowship;
use yalain_world::graph::{tag, Builder, Slot};
use yalain_world::inventory::function as inventory;
use yalain_world::{death, pack, places};

const USE: u32 = 0x8216_0016;
const USE_DONE: u32 = 0x8216_0021;
/// Loot's method when a fellowship is made: not shared, and shared.
const NOT_SHARED: u32 = 0;
const SHARED: u32 = 1;
const TURNS: u32 = 2;
const GROUP_TURNS: u32 = 3;
const DRAWN: u32 = 4;

fn string_arg(s: &str) -> Arg {
    let mut b = Builder::new();
    b.native(
        0x76,
        auberean_script::native_object::NativeObject::Text(s.encode_utf16().collect()),
    );
    Arg::Object(b.graph())
}

/// Two players side by side, and a creature to kill.
fn two() -> (Harness, (usize, u64), (usize, u64)) {
    let mut h = Harness::new();
    h.add_creatures(1);
    let a = h.player_in_world("one", "Killer");
    let b = h.player_in_world("two", "Other");
    h.run(10);
    (h, a, b)
}

fn make_fellowship(h: &mut Harness, leader: (usize, u64), others: &[(usize, u64)], method: u32) {
    h.ask(
        leader.0,
        fellowship::CREATE,
        &[Arg::Int(method), Arg::Int(0), string_arg("Band")],
        10,
    );
    for o in others {
        h.ask(leader.0, fellowship::RECRUIT, &[Arg::LongInt(o.1)], 10);
        h.ask(o.0, fellowship::ACCEPT, &[Arg::LongInt(leader.1)], 10);
    }
}

fn living(h: &Harness) -> u64 {
    h.server
        .things
        .by_id
        .values()
        .find(|t| t.template == yalain_testkit::TEST_CREATURE && t.alive())
        .map(|t| t.id)
        .expect("a creature")
}

/// The creature hurt to death by `by`, its corpse made, and everyone standing at it.
fn corpse_of_kill(h: &mut Harness, by: u64) -> u64 {
    let beetle = living(h);
    {
        let t = h.server.things.get_mut(beetle).expect("the creature");
        t.health = 10;
        t.max_health = 10;
        t.creature.as_mut().expect("a creature").stats.attacks_first = false;
    }
    let clock = h.clock();
    yalain_world::combat::hurt_creature(&mut h.server, &mut h.net, beetle, by, 50, clock);
    let corpse_there = |h: &Harness| {
        h.server
            .things
            .by_id
            .values()
            .find(|t| t.corpse.as_ref().is_some_and(|c| c.originator == beetle))
            .map(|t| t.id)
    };
    assert!(h.run_until(400, |h| corpse_there(h).is_some()), "a corpse");
    let corpse = corpse_there(h).expect("a corpse");
    stand_at(h, corpse);
    corpse
}

fn stand_at(h: &mut Harness, corpse: u64) {
    let at = h
        .server
        .things
        .get(corpse)
        .and_then(|t| t.at)
        .expect("placed");
    for p in h.server.players.values_mut() {
        p.position = at;
    }
}

/// What a use of the corpse came to: opened, and the use answers' statuses.
fn use_corpse(h: &mut Harness, who: (usize, u64), corpse: u64) -> (bool, Vec<u32>) {
    let from = h.bots[who.0].inbox.len();
    h.bots[who.0].send(
        true,
        queue::WEENIE,
        &server_call(USE, &[use_of(corpse, who.1)]),
    );
    h.run(6);
    let opened = h
        .calls(who.0, from)
        .iter()
        .any(|(f, v)| *f == inventory::OPEN_DONE && v.last() == Some(&Value::Long(corpse)));
    (opened, statuses(h, who.0, from))
}

fn statuses(h: &Harness, b: usize, from: usize) -> Vec<u32> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c) if c.function.0 == USE_DONE => {
                let v = c.args.as_ref().and_then(|a| calls::values(&a.0).ok())?;
                let Some(Value::Graph(g)) = v.first() else {
                    return None;
                };
                g.iter().find_map(|(_, n)| match n {
                    Node::Script { slots, .. } => Some(slots[11].1),
                    _ => None,
                })
            }
            _ => None,
        })
        .collect()
}

fn close(h: &mut Harness, who: (usize, u64), corpse: u64) {
    h.ask(who.0, inventory::CLOSE, &[Arg::LongInt(corpse)], 5);
}

fn contents(h: &Harness, corpse: u64) -> Vec<u64> {
    h.server
        .things
        .get(corpse)
        .and_then(|t| t.corpse.as_ref())
        .map(|c| c.contents.clone())
        .unwrap_or_default()
}

/// A character logs off and is gone from the world, its linger over.
fn log_off(h: &mut Harness, who: (usize, u64)) {
    h.bots[who.0].send(
        false,
        queue::LOGON,
        &ClientMessage::LogOff(CharacterRef { character: who.1 }),
    );
    for _ in 0..25 {
        h.advance(1.0);
        h.run(1);
    }
    assert!(
        !h.server.players.values().any(|p| p.character == who.1),
        "gone"
    );
}

/// How long from the corpse's making until its rights end.
fn locked_for(h: &Harness, corpse: u64) -> f64 {
    h.server
        .things
        .get(corpse)
        .and_then(|t| t.corpse.as_ref())
        .map_or(0.0, |c| c.unlocks)
        - h.now.0
}

/// A corpse opens to its killer, and not to anyone else: they are told they may not, and are given
/// nothing to look in.
/// Behaviour: server.loot.locked-to-its-killer
#[test]
fn a_corpse_opens_to_its_killer_and_refuses_anyone_else() {
    let (mut h, a, b) = two();
    let corpse = corpse_of_kill(&mut h, a.1);
    let (opened, said) = use_corpse(&mut h, b, corpse);
    assert!(!opened, "not to another");
    assert_eq!(said, vec![places::USE_STATUS_NO_RIGHT]);
    assert_eq!(
        h.server
            .players
            .values()
            .filter(|p| p.open_container.is_some())
            .count(),
        0
    );
    let (opened, said) = use_corpse(&mut h, a, corpse);
    assert!(opened, "to its killer: {said:?}");
}

/// Who a corpse is kept for is the one the kill went to, whoever's blow it was.
/// Behaviour: server.loot.kept-for-whom-the-kill-went-to
#[test]
fn a_corpse_goes_with_the_kill_to_the_most_damage_and_not_to_the_last_blow() {
    let (mut h, a, b) = two();
    let beetle = living(&h);
    {
        let t = h.server.things.get_mut(beetle).expect("the creature");
        t.health = 100;
        t.max_health = 100;
        t.creature.as_mut().expect("a creature").stats.attacks_first = false;
    }
    let clock = h.clock();
    yalain_world::combat::hurt_creature(&mut h.server, &mut h.net, beetle, a.1, 90, clock);
    yalain_world::combat::hurt_creature(&mut h.server, &mut h.net, beetle, b.1, 10, clock);
    let there = |h: &Harness| {
        h.server
            .things
            .by_id
            .values()
            .find(|t| t.corpse.as_ref().is_some_and(|c| c.originator == beetle))
            .map(|t| t.id)
    };
    assert!(h.run_until(400, |h| there(h).is_some()));
    let corpse = there(&h).expect("a corpse");
    stand_at(&mut h, corpse);
    assert!(!use_corpse(&mut h, b, corpse).0, "not to the last blow");
    assert!(use_corpse(&mut h, a, corpse).0, "to the most damage");
}

/// In a fellowship that shares loot the killer's fellows may open its corpse; in one that does not
/// they may not.
/// Behaviour: server.loot.fellows-if-sharing
#[test]
fn a_kills_fellows_may_open_its_corpse_only_where_the_fellowship_shares_loot() {
    let (mut h, a, b) = two();
    make_fellowship(&mut h, a, &[b], SHARED);
    let corpse = corpse_of_kill(&mut h, a.1);
    assert!(use_corpse(&mut h, b, corpse).0, "a fellow, sharing");

    let (mut h, a, b) = two();
    make_fellowship(&mut h, a, &[b], NOT_SHARED);
    let corpse = corpse_of_kill(&mut h, a.1);
    let (opened, said) = use_corpse(&mut h, b, corpse);
    assert!(!opened, "a fellow, not sharing");
    assert_eq!(said, vec![places::USE_STATUS_NO_RIGHT]);
}

/// A fellow's right is as things stand when it asks: gone once either has left, or the fellowship
/// is no more, and there for one who joined after the kill.
/// Behaviour: server.loot.fellows-as-they-stand
#[test]
fn a_fellows_right_to_a_corpse_goes_with_the_fellowship_as_it_stands_when_it_asks() {
    // The killer leaves.
    let (mut h, a, b) = two();
    make_fellowship(&mut h, a, &[b], SHARED);
    let corpse = corpse_of_kill(&mut h, a.1);
    h.ask(a.0, fellowship::LEAVE, &[], 10);
    assert!(!use_corpse(&mut h, b, corpse).0, "the killer left");
    assert!(use_corpse(&mut h, a, corpse).0, "still the killer's");

    // The fellow leaves.
    let (mut h, a, b) = two();
    make_fellowship(&mut h, a, &[b], SHARED);
    let corpse = corpse_of_kill(&mut h, a.1);
    h.ask(b.0, fellowship::LEAVE, &[], 10);
    assert!(!use_corpse(&mut h, b, corpse).0, "the fellow left");

    // Disbanded.
    let (mut h, a, b) = two();
    make_fellowship(&mut h, a, &[b], SHARED);
    let corpse = corpse_of_kill(&mut h, a.1);
    h.ask(a.0, fellowship::DISBAND, &[], 10);
    assert!(!use_corpse(&mut h, b, corpse).0, "disbanded");

    // Joined after the kill.
    let (mut h, a, b) = two();
    let corpse = corpse_of_kill(&mut h, a.1);
    assert!(!use_corpse(&mut h, b, corpse).0, "not yet a fellow");
    make_fellowship(&mut h, a, &[b], SHARED);
    stand_at(&mut h, corpse);
    assert!(use_corpse(&mut h, b, corpse).0, "a fellow now");
}

/// The first to open a corpse ends the rights in it: closed again, it opens to anyone.
/// Behaviour: server.loot.first-opening-frees-it
#[test]
fn a_corpse_once_opened_and_closed_opens_to_anyone() {
    let (mut h, a, b) = two();
    let corpse = corpse_of_kill(&mut h, a.1);
    assert!(!use_corpse(&mut h, b, corpse).0);
    assert!(use_corpse(&mut h, a, corpse).0);
    close(&mut h, a, corpse);
    assert!(
        use_corpse(&mut h, b, corpse).0,
        "freed by its first opening"
    );
}

/// Two minutes after the death a corpse nobody opened opens to anyone, and not before.
/// Behaviour: server.loot.freed-two-minutes-after-the-death
#[test]
fn a_corpse_nobody_opened_opens_to_anyone_two_minutes_after_the_death() {
    let (mut h, a, b) = two();
    let corpse = corpse_of_kill(&mut h, a.1);
    let left = locked_for(&h, corpse);
    assert!(
        (death::LOOT_LOCK - death::CORPSE_AFTER - 2.0..=death::LOOT_LOCK - death::CORPSE_AFTER)
            .contains(&left),
        "two minutes from the death, the corpse two seconds after: {left}"
    );
    h.advance(left - 1.0);
    h.run(1);
    assert!(!use_corpse(&mut h, b, corpse).0, "a moment before");
    h.advance(2.0);
    h.run(1);
    assert!(use_corpse(&mut h, b, corpse).0, "after");
}

/// A corpse nobody may open yet leaves the world when it would have, its killer gone or not, and
/// what it holds with it; the lock never keeps it.
/// Behaviour: server.loot.locked-corpses-still-rot
#[test]
fn a_locked_corpse_nobody_touches_leaves_the_world_on_time() {
    let (mut h, a, _b) = two();
    let corpse = corpse_of_kill(&mut h, a.1);
    let items = contents(&h, corpse);
    assert!(!items.is_empty());
    log_off(&mut h, a);
    let expires = h
        .server
        .things
        .get(corpse)
        .and_then(|t| t.corpse.as_ref())
        .map_or(0.0, |c| c.expires);
    assert!(expires - h.now.0 <= yalain_content::CorpseRot::CREATURE.untouched);
    h.advance(expires - h.now.0 + 1.0);
    h.run(10);
    assert!(h.server.things.get(corpse).is_none(), "gone");
    assert!(items.iter().all(|i| h.server.things.get(*i).is_none()));
}

/// A killer gone from the world leaves its corpse to the time: nobody else may open it till then,
/// and anyone may after; and it may open the corpse itself if it comes back first.
/// Behaviour: server.loot.a-killer-gone
#[test]
fn a_corpse_whose_killer_left_the_world_is_freed_by_the_time_and_is_still_its_if_it_returns() {
    let (mut h, a, b) = two();
    make_fellowship(&mut h, a, &[b], SHARED);
    let corpse = corpse_of_kill(&mut h, a.1);
    log_off(&mut h, a);
    stand_at(&mut h, corpse);
    assert!(!use_corpse(&mut h, b, corpse).0, "not a fellow's now");
    let back = h.at_character_list("one");
    h.enter_world(back, "one", a.1);
    h.run(5);
    stand_at(&mut h, corpse);
    assert!(use_corpse(&mut h, (back, a.1), corpse).0, "its own again");

    let (mut h, a, b) = two();
    make_fellowship(&mut h, a, &[b], SHARED);
    let corpse = corpse_of_kill(&mut h, a.1);
    log_off(&mut h, a);
    stand_at(&mut h, corpse);
    let left = locked_for(&h, corpse);
    assert!(left > 1.0, "{left}");
    h.advance(left + 0.5);
    h.run(1);
    assert!(use_corpse(&mut h, b, corpse).0, "anyone's, in time");
}

/// A killer dead when it could have looted leaves its corpse to its fellows, who may still open it.
/// Behaviour: server.loot.a-killer-dead
#[test]
fn a_dead_killers_fellow_may_open_its_corpse() {
    let (mut h, a, b) = two();
    make_fellowship(&mut h, a, &[b], SHARED);
    let corpse = corpse_of_kill(&mut h, a.1);
    let conn = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == a.1)
        .expect("in the world")
        .0;
    let clock = h.clock();
    death::player_dies(&mut h.server, &mut h.net, conn, None, clock);
    h.run(3);
    assert!(use_corpse(&mut h, b, corpse).0, "its fellow's");
}

/// The client's request to take everything from a container.
fn take_all(corpse: u64, me: u64) -> Arg {
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

fn transmute(corpse: u64, me: u64) -> Arg {
    let mut g = Builder::new();
    g.object(
        0x1B59,
        &[
            Slot::Int(0),
            Slot::Int(0),
            Slot::Ref(None),
            Slot::Int(1),
            Slot::Int(0),
            Slot::Int(0),
            Slot::Int(0),
            Slot::Int(0),
            Slot::Long(corpse),
            Slot::Ref(None),
            Slot::Int(0),
            Slot::Int(0),
            Slot::Long(me),
        ],
    );
    Arg::Object(g.graph())
}

/// Aimed by id at a corpse it was refused, a client gets nothing out of it by any road: taking
/// everything, turning it into money, taking an item, wearing one, dropping one.
/// Behaviour: server.loot.every-road-asks
#[test]
fn a_client_refused_a_corpse_takes_nothing_from_it_by_any_road() {
    let (mut h, a, b) = two();
    let corpse = corpse_of_kill(&mut h, a.1);
    let items = contents(&h, corpse);
    assert!(!items.is_empty());
    let money = |h: &Harness| h.server.players.values().map(|p| p.play.money).sum::<u64>();
    let before = money(&h);
    assert!(!use_corpse(&mut h, b, corpse).0);
    h.ask(b.0, pack::function::TAKE_ALL, &[take_all(corpse, b.1)], 10);
    h.advance(3.0);
    h.run(5);
    h.ask(b.0, inventory::TRANSMUTE_ALL, &[transmute(corpse, b.1)], 10);
    for item in &items {
        h.bots[b.0].send(
            true,
            queue::WEENIE,
            &server_call(USE, &[use_of(*item, b.1)]),
        );
        h.bots[b.0].send(
            true,
            queue::WEENIE,
            &server_call(
                yalain_world::equip::function::DIRECTIVE_EQUIP,
                &[Arg::LongInt(*item)],
            ),
        );
    }
    h.advance(3.0);
    h.run(10);
    assert_eq!(contents(&h, corpse), items, "all still in the corpse");
    assert_eq!(money(&h), before, "no money from it");
    let pack: Vec<u64> = h
        .server
        .players
        .values()
        .flat_map(|p| p.play.items.iter().map(|i| i.id))
        .collect();
    assert!(items.iter().all(|i| !pack.contains(i)), "none held");
}

/// A client far from a corpse it may not open is refused where it stands, and nobody walks it there.
/// Behaviour: server.loot.refused-where-it-stands
#[test]
fn a_far_client_refused_a_corpse_is_not_walked_to_it() {
    let (mut h, a, b) = two();
    let corpse = corpse_of_kill(&mut h, a.1);
    let p = h
        .server
        .players
        .values_mut()
        .find(|p| p.character == b.1)
        .expect("in the world");
    p.position.origin[0] += 20.0;
    let (opened, said) = use_corpse(&mut h, b, corpse);
    assert!(!opened);
    assert_eq!(said, vec![places::USE_STATUS_NO_RIGHT]);
    let p = h
        .server
        .players
        .values()
        .find(|p| p.character == b.1)
        .expect("in the world");
    assert!(p.walking_to.is_none(), "no walk");
}

/// A right lost on the walk is asked again on arrival.
/// Behaviour: server.loot.asked-again-at-the-corpse
#[test]
fn a_fellow_who_left_on_the_way_to_a_corpse_is_refused_at_it() {
    let (mut h, a, b) = two();
    make_fellowship(&mut h, a, &[b], SHARED);
    let corpse = corpse_of_kill(&mut h, a.1);
    let p = h
        .server
        .players
        .values_mut()
        .find(|p| p.character == b.1)
        .expect("in the world");
    p.position.origin[0] += 4.0;
    let from = h.bots[b.0].inbox.len();
    h.bots[b.0].send(
        true,
        queue::WEENIE,
        &server_call(USE, &[use_of(corpse, b.1)]),
    );
    h.run(3);
    assert!(
        h.server
            .players
            .values()
            .find(|p| p.character == b.1)
            .is_some_and(|p| p.walking_to.is_some()),
        "walking there"
    );
    h.ask(a.0, fellowship::LEAVE, &[], 3);
    stand_at(&mut h, corpse);
    let conn = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == b.1)
        .expect("in the world")
        .0;
    let clock = h.clock();
    places::arrived(&mut h.server, &mut h.net, conn, clock);
    h.run(5);
    assert_eq!(statuses(&h, b.0, from), vec![places::USE_STATUS_NO_RIGHT]);
    assert!(
        !h.calls(b.0, from)
            .iter()
            .any(|(f, _)| *f == inventory::OPEN_DONE),
        "not opened"
    );
}

fn claimant_of(h: &Harness, corpse: u64) -> u64 {
    h.server
        .things
        .get(corpse)
        .and_then(|t| t.corpse.as_ref())
        .map_or(0, |c| c.claimant)
}

/// A creature to kill, again.
fn another(h: &mut Harness) {
    h.add_creatures(1);
    h.run(10);
}

fn sorted(mut ids: Vec<u64>) -> Vec<u64> {
    ids.sort_unstable();
    ids
}

/// In a fellowship that takes turns each kill's corpse goes to the next fellow by its id, round and
/// round, whoever killed it; only that fellow may open it, until it is opened; and it is told as its
/// claimant.
/// Behaviour: server.loot.turns-by-id
#[test]
fn a_fellowship_taking_turns_gives_each_corpse_to_the_next_fellow_by_id() {
    let (mut h, a, b) = two();
    let c = h.player_in_world("three", "Third");
    make_fellowship(&mut h, a, &[b, c], TURNS);
    let ids = sorted(vec![a.1, b.1, c.1]);
    let mut corpses = Vec::new();
    for _ in 0..4 {
        another(&mut h);
        corpses.push(corpse_of_kill(&mut h, a.1));
    }
    let gone_to: Vec<u64> = corpses.iter().map(|c| claimant_of(&h, *c)).collect();
    assert_eq!(gone_to, vec![ids[0], ids[1], ids[2], ids[0]]);
    // One that went to a fellow other than its killer.
    let corpse = corpses[..3]
        .iter()
        .copied()
        .find(|c| claimant_of(&h, *c) != a.1)
        .expect("one went to another");
    let holder = if claimant_of(&h, corpse) == b.1 { b } else { c };
    stand_at(&mut h, corpse);
    let (opened, said) = use_corpse(&mut h, a, corpse);
    assert!(!opened, "not the next fellow's, even the killer's");
    assert_eq!(said, vec![places::USE_STATUS_NO_RIGHT]);
    let told = yalain_world::death::corpse_block(h.server.things.get(corpse).expect("the corpse"));
    assert!(told.iter().any(|m| matches!(
        m,
        ServerMessage::UpdateInstanceIdVisual(q)
            if q.quality == death::q::CLAIMANT && q.value == holder.1
    )));
    assert!(use_corpse(&mut h, holder, corpse).0);
    close(&mut h, holder, corpse);
    assert!(
        use_corpse(&mut h, a, corpse).0,
        "freed by its first opening"
    );
}

/// Taking turns for group creatures alone leaves any other creature's corpse shared, and gives a
/// group creature's in turn.
/// Behaviour: server.loot.turns-for-group-creatures
#[test]
fn turns_for_group_creatures_leave_other_kills_shared() {
    let (mut h, a, b) = two();
    make_fellowship(&mut h, a, &[b], GROUP_TURNS);
    let corpse = corpse_of_kill(&mut h, a.1);
    assert_eq!(claimant_of(&h, corpse), 0);
    assert!(use_corpse(&mut h, b, corpse).0, "shared");
    close(&mut h, b, corpse);
    another(&mut h);
    let beetle = living(&h);
    h.server
        .things
        .get_mut(beetle)
        .expect("the creature")
        .facts
        .group = true;
    let corpse = corpse_of_kill(&mut h, a.1);
    assert_eq!(claimant_of(&h, corpse), sorted(vec![a.1, b.1])[0]);
}

/// Drawn at random, a corpse goes to one of the fellows, and to each in time.
/// Behaviour: server.loot.drawn-at-random
#[test]
fn a_fellowship_drawing_lots_gives_each_corpse_to_one_of_its_fellows() {
    let (mut h, a, b) = two();
    make_fellowship(&mut h, a, &[b], DRAWN);
    let mut gone_to = std::collections::BTreeSet::new();
    for _ in 0..30 {
        another(&mut h);
        let corpse = corpse_of_kill(&mut h, a.1);
        gone_to.insert(claimant_of(&h, corpse));
    }
    assert_eq!(
        gone_to.into_iter().collect::<Vec<_>>(),
        sorted(vec![a.1, b.1])
    );
}

/// A fellow a corpse was given to and who has gone leaves it to the time, like any other right.
/// Behaviour: server.loot.a-claimant-gone
#[test]
fn a_corpse_given_to_a_fellow_who_left_is_freed_by_the_time() {
    let (mut h, a, b) = two();
    make_fellowship(&mut h, a, &[b], TURNS);
    let corpse = corpse_of_kill(&mut h, a.1);
    let (holder, not) = if claimant_of(&h, corpse) == a.1 {
        (a, b)
    } else {
        (b, a)
    };
    log_off(&mut h, holder);
    stand_at(&mut h, corpse);
    assert!(!use_corpse(&mut h, not, corpse).0);
    let left = locked_for(&h, corpse);
    h.advance(left + 0.5);
    h.run(1);
    assert!(use_corpse(&mut h, not, corpse).0);
}

/// Which of a corpse's profile's lines an examination says.
fn page(h: &mut Harness, who: (usize, u64), corpse: u64) -> Vec<u8> {
    use yalain_world::profile;
    let from = h.bots[who.0].inbox.len();
    let mut bytes = profile::request::OBJECT.to_le_bytes().to_vec();
    bytes.extend(0u32.to_le_bytes());
    bytes.extend(corpse.to_le_bytes());
    bytes.extend(0x3FFu32.to_le_bytes());
    let mut g = Builder::new();
    g.native(
        profile::REQUEST_TYPE,
        auberean_script::native_object::NativeObject::Packed {
            ty: u32::from(profile::REQUEST_TYPE),
            bytes,
        },
    );
    h.bots[who.0].send(
        true,
        queue::WEENIE,
        &server_call(
            yalain_world::requests::function::EXAMINE,
            &[Arg::Object(g.graph())],
        ),
    );
    h.run(5);
    h.world_messages(who.0, from)
        .into_iter()
        .find_map(|m| match m {
            ServerMessage::PlayerCall(c) if c.function.0 == profile::EXAMINATION_PROFILE => {
                Some(c.args?.0)
            }
            _ => None,
        })
        .expect("the profile")
}

fn says(page: &[u8], id: u32) -> bool {
    page.windows(4).any(|w| w == id.to_le_bytes())
}

/// A corpse's examination says who killed it, and whether the one examining it may loot it; what
/// it holds is unknown until it is opened, when it also says who first did.
/// Behaviour: server.loot.an-examination-says-who-may-loot
#[test]
fn a_corpses_examination_says_who_may_loot_it_and_who_first_did() {
    use yalain_world::profile::corpse_line as l;
    let (mut h, a, b) = two();
    let corpse = corpse_of_kill(&mut h, a.1);
    let theirs = page(&mut h, b, corpse);
    assert!(says(&theirs, l::KILLED_BY));
    assert!(says(&theirs, l::YOU_CANNOT_LOOT) && !says(&theirs, l::YOU_CAN_LOOT));
    assert!(says(&theirs, l::CONTENTS_UNKNOWN) && !says(&theirs, l::CONTAINS));
    assert!(!says(&theirs, l::FIRST_LOOTED_BY));
    let own = page(&mut h, a, corpse);
    assert!(says(&own, l::YOU_CAN_LOOT) && !says(&own, l::YOU_CANNOT_LOOT));
    assert!(use_corpse(&mut h, a, corpse).0);
    let theirs = page(&mut h, b, corpse);
    assert!(says(&theirs, l::YOU_CAN_LOOT), "freed");
    assert!(says(&theirs, l::FIRST_LOOTED_BY));
    assert!(says(&theirs, l::CONTAINS) && !says(&theirs, l::CONTENTS_UNKNOWN));
}

/// A corpse with nothing in it says so at once, whoever is looking and even if it may not be looted.
/// Behaviour: server.loot.an-empty-corpse-says-so
#[test]
fn an_empty_corpses_examination_says_it_holds_nothing_before_it_is_opened() {
    use yalain_world::profile::corpse_line as l;
    let (mut h, a, b) = two();
    let corpse = corpse_of_kill(&mut h, a.1);
    for item in contents(&h, corpse) {
        h.server.things.remove(item);
    }
    if let Some(c) = h
        .server
        .things
        .get_mut(corpse)
        .and_then(|t| t.corpse.as_mut())
    {
        c.contents.clear();
    }
    let theirs = page(&mut h, b, corpse);
    assert!(says(&theirs, l::YOU_CANNOT_LOOT));
    assert!(says(&theirs, l::CONTAINS) && !says(&theirs, l::CONTENTS_UNKNOWN));
    assert!(!says(&theirs, l::FIRST_LOOTED_BY));
}

/// A corpse no player's kill made opens to anyone, and so does a player's own corpse, which holds
/// nothing.
/// Behaviour: server.loot.no-players-kill-opens-to-anyone
#[test]
fn a_corpse_of_no_players_kill_and_a_players_own_corpse_open_to_anyone() {
    let (mut h, a, b) = two();
    // Killed by another creature.
    let beetle = living(&h);
    h.add_creatures(1);
    h.run(10);
    let other = h
        .server
        .things
        .by_id
        .values()
        .find(|t| t.template == yalain_testkit::TEST_CREATURE && t.alive() && t.id != beetle)
        .map(|t| t.id)
        .expect("another creature");
    let corpse = corpse_of_kill(&mut h, other);
    let now = h.now.0;
    assert!(
        death::may_loot(&h.server, b.1, corpse, now),
        "a creature's kill"
    );
    // Its corpse's killer named, nobody's.
    if let Some(c) = h
        .server
        .things
        .get_mut(corpse)
        .and_then(|t| t.corpse.as_mut())
    {
        c.killer = 0;
    }
    assert!(
        death::may_loot(&h.server, b.1, corpse, now),
        "nobody's kill"
    );
    // A player killed by another.
    let conn = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == a.1)
        .expect("in the world")
        .0;
    let clock = h.clock();
    death::player_dies(&mut h.server, &mut h.net, conn, Some(b.1), clock);
    h.advance(death::PLAYER_CORPSE_AFTER + 0.5);
    h.run(5);
    let own = h
        .server
        .things
        .by_id
        .values()
        .find(|t| t.corpse.as_ref().is_some_and(|c| c.originator == a.1))
        .map(|t| t.id)
        .expect("its corpse");
    let now = h.now.0;
    assert!(death::may_loot(&h.server, a.1, own, now), "its own");
}
