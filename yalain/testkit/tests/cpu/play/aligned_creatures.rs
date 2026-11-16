//! Creatures a world's rows align: one holds the kingdom its template names from its first moment
//! for the row's time, and players of that kingdom may not harm it meanwhile; then it holds none.
//! Fixture: the test world's beetles, their template made the Order's, one player.

use yalain_testkit::{Harness, TEST_CREATURE};
use yalain_world::combat;
use yalain_world::kingdoms::{kingdom, refusal, set_kingdom};
use yalain_world::things::Mode;

fn world(faction: Option<u32>) -> Harness {
    let mut h = Harness::new();
    h.add_creatures(1);
    if let Some(t) = h.server.catalog.templates.get_mut(&TEST_CREATURE) {
        t.facts.faction = faction;
    }
    h.server.creature_rows.aligned.insert(TEST_CREATURE, 60.0);
    h.player_in_world("one", "Kestrel");
    h.run(20);
    h
}

fn beetle(h: &Harness) -> u64 {
    h.server
        .things
        .by_id
        .values()
        .find(|t| t.creature.as_ref().is_some_and(|c| c.mode != Mode::Dead))
        .map(|t| t.id)
        .expect("a beetle")
}

/// May the one player, of `kingdom`, harm the beetle?
fn may_harm(h: &mut Harness, of: Option<u32>) -> Result<(), u32> {
    let world = *h.server.players.keys().next().expect("in the world");
    if let Some(p) = h.server.players.get_mut(&world) {
        set_kingdom(&mut p.qualities, of.unwrap_or(0));
    }
    let b = beetle(h);
    combat::may_attack_thing(&h.server, world, b)
}

/// An aligned beetle holds the Order from its first moment: an Order player may not harm it, any
/// other may; once the row's time has gone it holds no kingdom, and the Order player may harm it.
/// Behaviour: server.creatures.an-aligned-creature-spares-its-kingdom-for-a-while
#[test]
fn an_aligned_creature_may_not_be_harmed_by_its_own_kingdom_until_its_time_has_gone() {
    let mut h = world(Some(kingdom::ORDER));
    let b = beetle(&h);
    assert_eq!(
        h.server.things.get(b).and_then(|t| t.kingdom),
        Some(kingdom::ORDER)
    );
    assert_eq!(
        may_harm(&mut h, Some(kingdom::ORDER)),
        Err(refusal::SAME_KINGDOM)
    );
    assert_eq!(may_harm(&mut h, Some(kingdom::SHADOW)), Ok(()));
    let t = h.now.0;
    while h.now.0 < t + 61.0 {
        h.step();
    }
    assert_eq!(
        h.server.things.get(b).and_then(|t| t.kingdom),
        None,
        "neutral"
    );
    assert_eq!(may_harm(&mut h, Some(kingdom::ORDER)), Ok(()));
}

/// A template that names no kingdom: its creatures hold none, whatever the row.
/// Behaviour: server.creatures.an-aligned-creature-spares-its-kingdom-for-a-while
#[test]
fn a_creature_whose_template_names_no_kingdom_holds_none() {
    let mut h = world(None);
    let b = beetle(&h);
    assert_eq!(h.server.things.get(b).and_then(|t| t.kingdom), None);
    assert_eq!(may_harm(&mut h, Some(kingdom::ORDER)), Ok(()));
}
