//! How long a corpse lasts: by its creature's template's times, whether it can hold anything,
//! and how it was left.

use yalain_content::{CorpseRot, Prop, Props};
use yalain_testkit::Harness;

/// A template's three times, as a template writes them.
fn times(untouched: f32, items_left: f32, empty: f32) -> CorpseRot {
    let mut p = Props::default();
    p.set("NPC_BaseRotTime", Prop::Float(untouched))
        .set("NPC_MaxAcceleratedRotTime", Prop::Float(items_left))
        .set("NPC_MinAcceleratedRotTime", Prop::Float(empty));
    CorpseRot::from_props(&p)
}

/// A player, and the corpse of a creature with these times, as it is made (`holds`: it has a loot
/// profile and its items, else it has neither).
fn corpse_of(rot: CorpseRot, holds: bool) -> (Harness, (usize, u64), u64) {
    let mut h = Harness::new();
    h.add_creatures(1);
    let me = h.player_in_world("one", "Killer");
    h.run(10);
    let beetle = h
        .server
        .things
        .by_id
        .values()
        .find(|t| t.template == yalain_testkit::TEST_CREATURE && t.alive())
        .map(|t| t.id)
        .expect("a creature");
    {
        let t = h.server.things.get_mut(beetle).expect("the creature");
        t.health = 10;
        t.max_health = 10;
        let c = t.creature.as_mut().expect("a creature");
        c.stats.attacks_first = false;
        c.stats.rot = rot;
        if !holds {
            c.stats.loot_profile = None;
            c.stats.trophies.clear();
        }
    }
    let clock = h.clock();
    yalain_world::combat::hurt_creature(&mut h.server, &mut h.net, beetle, me.1, 50, clock);
    let there = |h: &Harness| {
        h.server
            .things
            .by_id
            .values()
            .find(|t| t.corpse.as_ref().is_some_and(|c| c.originator == beetle))
            .map(|t| t.id)
    };
    assert!(h.run_until(400, |h| there(h).is_some()), "a corpse");
    let corpse = there(&h).expect("a corpse");
    assert_eq!(
        h.server
            .things
            .get(corpse)
            .and_then(|t| t.corpse.as_ref())
            .is_some_and(|c| c.contents.is_empty()),
        !holds,
        "what it holds"
    );
    (h, me, corpse)
}

fn wait(h: &mut Harness, seconds: f64) {
    h.advance(seconds);
    h.run(2);
}

fn there(h: &Harness, corpse: u64) -> bool {
    h.server.things.get(corpse).is_some()
}

fn open(h: &mut Harness, corpse: u64) {
    let world = *h.server.players.keys().next().expect("the player");
    yalain_world::inventory::open(&mut h.server, &mut h.net, world, corpse);
}

fn close(h: &mut Harness, corpse: u64) {
    h.run(1);
    let world = *h.server.players.keys().next().expect("the player");
    yalain_world::inventory::close(&mut h.server, &mut h.net, world, corpse);
}

fn empty(h: &mut Harness, corpse: u64) {
    h.server
        .things
        .get_mut(corpse)
        .and_then(|t| t.corpse.as_mut())
        .expect("a corpse")
        .contents
        .clear();
}

/// Nobody touches it: it lasts its template's time from its making.
/// Behaviour: server.corpse.lasts-its-templates-time
#[test]
fn a_corpse_nobody_touches_lasts_its_templates_time() {
    let (mut h, _, corpse) = corpse_of(times(40.0, 12.0, 3.0), true);
    wait(&mut h, 38.0);
    assert!(there(&h, corpse), "before its time");
    wait(&mut h, 3.0);
    assert!(!there(&h, corpse), "after its time");
}

/// Closed with items left it goes after its template's time for that, closed empty after its time
/// for that.
/// Behaviour: server.corpse.closed-by-its-templates-times
#[test]
fn a_corpse_closed_goes_by_its_templates_times_for_what_it_holds() {
    let (mut h, _, corpse) = corpse_of(times(400.0, 20.0, 5.0), true);
    open(&mut h, corpse);
    close(&mut h, corpse);
    wait(&mut h, 18.0);
    assert!(there(&h, corpse), "items left, before its time");
    wait(&mut h, 3.0);
    assert!(!there(&h, corpse), "items left, after its time");

    let (mut h, _, corpse) = corpse_of(times(400.0, 20.0, 5.0), true);
    open(&mut h, corpse);
    empty(&mut h, corpse);
    close(&mut h, corpse);
    wait(&mut h, 3.0);
    assert!(there(&h, corpse), "empty, before its time");
    wait(&mut h, 3.0);
    assert!(!there(&h, corpse), "empty, after its time");
}

/// A creature with no loot profile that left nothing: its corpse goes as one closed empty does,
/// from its making.
/// Behaviour: server.corpse.one-that-can-hold-nothing-goes-soon
#[test]
fn a_corpse_that_can_hold_nothing_goes_after_the_time_for_an_empty_one() {
    let (mut h, _, corpse) = corpse_of(CorpseRot::CREATURE, false);
    wait(&mut h, 13.0);
    assert!(there(&h, corpse), "before its time");
    wait(&mut h, 3.0);
    assert!(!there(&h, corpse), "after its time");
}

/// A creature with no loot profile whose own drop came to something: as long as any corpse.
/// Behaviour: server.corpse.one-holding-a-drop-lasts
#[test]
fn a_corpse_holding_a_drop_lasts_as_any_corpse_does_with_no_loot_profile() {
    let mut h = Harness::new();
    h.add_creatures(1);
    let me = h.player_in_world("one", "Killer");
    h.run(10);
    let beetle = h
        .server
        .things
        .by_id
        .values()
        .find(|t| t.template == yalain_testkit::TEST_CREATURE && t.alive())
        .map(|t| t.id)
        .expect("a creature");
    {
        let t = h.server.things.get_mut(beetle).expect("the creature");
        t.health = 10;
        t.max_health = 10;
        let c = t.creature.as_mut().expect("a creature");
        c.stats.attacks_first = false;
        c.stats.loot_profile = None;
        c.stats.trophies = vec![(yalain_testkit::TEST_ITEM, 1.0)];
    }
    let clock = h.clock();
    yalain_world::combat::hurt_creature(&mut h.server, &mut h.net, beetle, me.1, 50, clock);
    let corpse_of = |h: &Harness| {
        h.server
            .things
            .by_id
            .values()
            .find(|t| t.corpse.as_ref().is_some_and(|c| c.originator == beetle))
            .map(|t| t.id)
    };
    assert!(h.run_until(400, |h| corpse_of(h).is_some()), "a corpse");
    let corpse = corpse_of(&h).expect("a corpse");
    wait(&mut h, 100.0);
    assert!(there(&h, corpse), "well past the time for an empty one");
}

/// A creature with a loot profile that happened to leave nothing: as long as any corpse.
/// Behaviour: server.corpse.a-profile-that-drew-nothing-lasts
#[test]
fn a_corpse_whose_loot_profile_drew_nothing_lasts_as_any_corpse_does() {
    let mut h = Harness::new();
    h.add_creatures(1);
    h.server.loot = yalain_content::LootTable::default();
    let me = h.player_in_world("one", "Killer");
    h.run(10);
    let beetle = h
        .server
        .things
        .by_id
        .values()
        .find(|t| t.template == yalain_testkit::TEST_CREATURE && t.alive())
        .map(|t| t.id)
        .expect("a creature");
    {
        let t = h.server.things.get_mut(beetle).expect("the creature");
        t.health = 10;
        t.max_health = 10;
        t.creature.as_mut().expect("a creature").stats.attacks_first = false;
    }
    let clock = h.clock();
    yalain_world::combat::hurt_creature(&mut h.server, &mut h.net, beetle, me.1, 50, clock);
    let corpse_of = |h: &Harness| {
        h.server
            .things
            .by_id
            .values()
            .find(|t| t.corpse.as_ref().is_some_and(|c| c.originator == beetle))
            .map(|t| t.id)
    };
    assert!(h.run_until(400, |h| corpse_of(h).is_some()), "a corpse");
    let corpse = corpse_of(&h).expect("a corpse");
    wait(&mut h, 100.0);
    assert!(there(&h, corpse), "well past the time for an empty one");
}

/// Opened and closed before its short time is up, one that can hold nothing goes when it would have.
/// Behaviour: server.corpse.opening-keeps-the-short-time
#[test]
fn opening_a_corpse_that_holds_nothing_keeps_its_short_time() {
    let (mut h, _, corpse) = corpse_of(CorpseRot::CREATURE, false);
    open(&mut h, corpse);
    wait(&mut h, 5.0);
    close(&mut h, corpse);
    wait(&mut h, 8.0);
    assert!(there(&h, corpse), "before its time");
    wait(&mut h, 3.0);
    assert!(!there(&h, corpse), "at its time, not the closing's");
}

/// A corpse does not leave while it is open, however long; it goes at the first look after it is
/// closed, a look each time an empty one's time goes by.
/// Behaviour: server.corpse.stays-while-open
#[test]
fn a_corpse_stays_while_it_is_open_and_goes_at_the_next_look_after_it_is_closed() {
    let (mut h, _, corpse) = corpse_of(times(30.0, 60.0, 8.0), true);
    open(&mut h, corpse);
    for _ in 0..12 {
        wait(&mut h, 5.0);
    }
    assert!(there(&h, corpse), "open long past its time");
    close(&mut h, corpse);
    wait(&mut h, 9.0);
    assert!(!there(&h, corpse), "by the next look after it closed");
}

/// A corpse left open past its time and closed goes at the next look, not after a closing's time.
/// Behaviour: server.corpse.closed-past-its-time
#[test]
fn a_corpse_closed_past_its_time_goes_at_the_next_look_and_not_a_closings_time_later() {
    let (mut h, _, corpse) = corpse_of(times(30.0, 60.0, 20.0), true);
    open(&mut h, corpse);
    wait(&mut h, 25.0);
    assert!(there(&h, corpse), "before its time");
    wait(&mut h, 8.0);
    assert!(there(&h, corpse), "open, past its time");
    close(&mut h, corpse);
    wait(&mut h, 5.0);
    assert!(there(&h, corpse), "closed, before the look");
    wait(&mut h, 15.0);
    assert!(
        !there(&h, corpse),
        "after the look, well before a closing's 60 s"
    );
}

/// Closing never makes a corpse last longer than it would have.
/// Behaviour: server.corpse.closing-never-lengthens
#[test]
fn closing_a_corpse_never_makes_it_last_longer() {
    let (mut h, _, corpse) = corpse_of(times(30.0, 60.0, 5.0), true);
    wait(&mut h, 20.0);
    open(&mut h, corpse);
    close(&mut h, corpse);
    wait(&mut h, 8.0);
    assert!(there(&h, corpse), "before its time");
    wait(&mut h, 3.0);
    assert!(!there(&h, corpse), "at its first time, not the closing's");
}

/// A player's corpse lasts its minute, closed or not.
/// Behaviour: server.corpse.a-players-lasts-a-minute
#[test]
fn a_players_corpse_closed_empty_still_lasts_its_minute() {
    let mut h = Harness::new();
    h.add_creatures(1);
    let (b, me) = h.player_in_world("one", "Killer");
    h.run(10);
    let world = *h.server.players.keys().next().expect("the player");
    let clock = h.clock();
    yalain_world::death::player_dies(&mut h.server, &mut h.net, world, None, clock);
    let there_now = |h: &Harness| {
        h.server
            .things
            .by_id
            .values()
            .find(|t| t.corpse.as_ref().is_some_and(|c| c.originator == me))
            .map(|t| t.id)
    };
    assert!(
        h.run_until(4000, |h| there_now(h).is_some()),
        "its corpse for {b}"
    );
    let corpse = there_now(&h).expect("its corpse");
    open(&mut h, corpse);
    close(&mut h, corpse);
    wait(&mut h, 57.0);
    assert!(there(&h, corpse), "before its minute");
    wait(&mut h, 4.0);
    assert!(!there(&h, corpse), "after it");
}
