//! Death treasures: a world's fixed item for a creature's template or its camp, rolled once at its
//! chance as the corpse is made, on top of the rest of what the creature leaves; only a creature a
//! camp keeps leaves one, and a corpse holds an item once however many rows name it. Fixture: the
//! test world's camp of beetles, its regeneration shortened, and a treasure item.

use std::collections::BTreeMap;

use yalain_content::{DeathTreasure, Dropper, Props};
use yalain_store::StoredPosition;
use yalain_testkit::{Harness, TEST_CREATURE, TEST_ITEM};
use yalain_world::rng::Rng;
use yalain_world::things::Mode;

/// The treasure, an item no profile of the test world draws.
const TREASURE: u32 = 0x4700_2109;
/// A second treasure.
const TREASURE_2: u32 = 0x4700_210A;
/// A template no camp of the test world makes.
const ELSEWHERE: u32 = 0x4700_2225;

/// The test world with a camp of `population` beetles that come back `regen` seconds after each
/// death, the treasure's template, the rows given, the server's draws seeded.
fn camp_world(population: u32, regen: f64, rows: Vec<DeathTreasure>, seed: u64) -> Harness {
    let mut h = Harness::new();
    h.add_creatures(population);
    h.server.spawns.camps[0].regen = regen;
    for t in [TREASURE, TREASURE_2] {
        h.server
            .catalog
            .insert(yalain_testkit::test_template(t, 0x3A1, Props::default()));
    }
    h.server.loot.treasures = rows;
    h.server.rng = Rng::new(seed);
    h.run(20);
    h
}

fn row(from: Dropper, chance: f32) -> DeathTreasure {
    DeathTreasure {
        from,
        item: TREASURE,
        chance,
        placed: "authored for a test".into(),
        confidence: "authored".into(),
    }
}

fn living(h: &Harness) -> Vec<u64> {
    let mut v: Vec<u64> = h
        .server
        .things
        .by_id
        .values()
        .filter(|t| t.creature.as_ref().is_some_and(|c| c.mode != Mode::Dead))
        .map(|t| t.id)
        .collect();
    v.sort_unstable();
    v
}

/// Every creature alive now dies; how many.
fn kill_all(h: &mut Harness) -> usize {
    let all = living(h);
    for id in &all {
        let clock = h.clock();
        yalain_world::death::creature_dies(&mut h.server, &mut h.net, *id, 0, clock);
    }
    all.len()
}

fn run_for(h: &mut Harness, seconds: f64) {
    let t = h.now.0 + seconds;
    while h.now.0 < t {
        h.step_by(0.25);
    }
}

/// Every corpse in the world now, added to those seen: what each holds, as (template, value).
fn note_corpses(h: &Harness, seen: &mut BTreeMap<u64, Vec<(u32, u32)>>) {
    for t in h.server.things.by_id.values() {
        let Some(c) = t.corpse.as_ref() else { continue };
        seen.entry(t.id).or_insert_with(|| {
            c.contents
                .iter()
                .filter_map(|i| h.server.things.get(*i))
                .map(|i| (i.template, i.value))
                .collect()
        });
    }
}

/// Kill the camp's beetles `rounds` times over, each round's corpses made before the next; what
/// every corpse held, and how many died.
fn rounds(h: &mut Harness, rounds: usize) -> (BTreeMap<u64, Vec<(u32, u32)>>, usize) {
    let (mut seen, mut died) = (BTreeMap::new(), 0);
    for _ in 0..rounds {
        died += kill_all(h);
        run_for(h, 2.5);
        note_corpses(h, &mut seen);
        run_for(h, 3.5);
    }
    (seen, died)
}

fn holding(corpses: &BTreeMap<u64, Vec<(u32, u32)>>) -> usize {
    corpses
        .values()
        .filter(|c| c.iter().any(|(t, _)| *t == TREASURE))
        .count()
}

/// A creature whose template has a death treasure leaves it at the row's chance: a quarter of the
/// time over 400 kills, never twice in one corpse, and always beside what its profile leaves.
/// Behaviour: server.loot.death-treasure-at-its-chance
#[test]
fn a_creatures_death_treasure_is_left_at_its_chance_over_many_kills() {
    let mut h = camp_world(
        10,
        1.0,
        vec![row(Dropper::Template(TEST_CREATURE), 0.25)],
        7,
    );
    let (corpses, died) = rounds(&mut h, 40);
    assert_eq!(
        (died, corpses.len()),
        (400, 400),
        "a corpse for every death"
    );
    let hits = holding(&corpses);
    assert!(
        (70..=130).contains(&hits),
        "about a quarter of 400 corpses: {hits}"
    );
    for c in corpses.values() {
        assert!(
            c.iter().filter(|(t, _)| *t == TREASURE).count() <= 1,
            "once a corpse: {c:x?}"
        );
        assert!(
            c.iter().any(|(t, _)| *t == TEST_ITEM),
            "beside its profile's item: {c:x?}"
        );
    }
}

/// A row for another template or another camp changes nothing for the creatures it doesn't name:
/// with the same draws, every corpse holds what it held in a world with no rows. A creature that
/// has a row leaves its own loot as it would without it, and its treasure after.
/// Behaviour: server.loot.death-treasure-only-its-own
#[test]
fn creatures_without_a_death_treasure_leave_what_they_did_before() {
    let plain = rounds(&mut camp_world(5, 1.0, Vec::new(), 11), 20).0;
    let others = vec![
        row(Dropper::Template(ELSEWHERE), 1.0),
        row(Dropper::Camp("elsewhere".into()), 1.0),
    ];
    let with_others = rounds(&mut camp_world(5, 1.0, others, 11), 20).0;
    assert_eq!(plain.len(), 100);
    assert_eq!(with_others, plain, "the same corpses, the same draws");
    let mut h = camp_world(5, 1.0, vec![row(Dropper::Template(TEST_CREATURE), 1.0)], 11);
    kill_all(&mut h);
    run_for(&mut h, 2.5);
    let mut first = BTreeMap::new();
    note_corpses(&h, &mut first);
    let (id, held) = first.iter().next().expect("a corpse");
    let mut own = held.clone();
    assert_eq!(own.pop(), Some((TREASURE, 0)), "its treasure, last");
    assert_eq!(&own, &plain[id], "its own loot as without the row");
}

/// A camp's row gives its own creatures its treasure and no other camp's, though they are of one
/// template, as a template's row gives every camp's; a creature no camp keeps (as one made by a
/// death or an item's use) leaves neither.
/// Behaviour: server.loot.death-treasure-a-camps-creatures
#[test]
fn a_camps_death_treasure_is_its_creatures_and_no_others() {
    let rows = vec![
        row(Dropper::Camp("test".into()), 1.0),
        DeathTreasure {
            item: TREASURE_2,
            ..row(Dropper::Template(TEST_CREATURE), 1.0)
        },
    ];
    let mut h = camp_world(2, 1.0, rows, 3);
    let mut other = yalain_testkit::test_camp(2);
    other.id = "test-2".into();
    h.server.spawns.camps.push(other);
    h.server
        .camps
        .push(yalain_world::spawn::CampState::default());
    run_for(&mut h, 1.0);
    let template = h
        .server
        .catalog
        .get(TEST_CREATURE)
        .cloned()
        .expect("the beetle");
    let stats = yalain_content::CreatureStats::from_props(&template.props).expect("a creature");
    let at = StoredPosition {
        cell: 0x0299_0109,
        origin: [62.0, 240.0, -44.6],
        rotation: [1.0, 0.0, 0.0, 0.0],
    };
    let loose = yalain_world::spawn::make(
        &mut h.server,
        &template,
        stats,
        (at, 0),
        None,
        None,
        h.now.0,
    );
    let camp_of = |h: &Harness, id: u64| {
        h.server
            .things
            .get(id)
            .and_then(|t| t.creature.as_ref())
            .and_then(|c| c.camp)
    };
    let by_camp: Vec<(u64, Option<usize>)> = living(&h)
        .into_iter()
        .map(|id| (id, camp_of(&h, id)))
        .collect();
    assert_eq!(by_camp.len(), 5, "two of each camp and the loose one");
    kill_all(&mut h);
    run_for(&mut h, 2.5);
    for (id, camp) in by_camp {
        let held = h
            .server
            .things
            .by_id
            .values()
            .find_map(|t| t.corpse.as_ref().filter(|c| c.originator == id))
            .map(|c| {
                let has = |x: u32| {
                    c.contents
                        .iter()
                        .filter_map(|i| h.server.things.get(*i))
                        .any(|i| i.template == x)
                };
                (has(TREASURE), has(TREASURE_2))
            })
            .expect("a corpse");
        assert_eq!(
            held,
            (camp == Some(0), camp.is_some()),
            "{id:x} of camp {camp:?}"
        );
        if id == loose {
            assert_eq!(camp, None);
        }
    }
}

/// A corpse holds a death treasure once, though its template's row and its camp's both name it;
/// a corpse full from its profile still holds it; a creature killed again once dead leaves one
/// corpse.
/// Behaviour: server.loot.death-treasure-once
#[test]
fn a_corpse_holds_a_death_treasure_once_however_it_is_named() {
    let rows = vec![
        row(Dropper::Template(TEST_CREATURE), 1.0),
        row(Dropper::Camp("test".into()), 1.0),
    ];
    let mut h = camp_world(1, 60.0, rows, 5);
    h.server.loot.profiles[0].counts = vec![0.0; yalain_world::death::MOST_ITEMS + 1];
    h.server.loot.profiles[0].counts.push(1.0);
    let beetle = living(&h)[0];
    for _ in 0..2 {
        let clock = h.clock();
        yalain_world::death::creature_dies(&mut h.server, &mut h.net, beetle, 0, clock);
        h.step();
    }
    run_for(&mut h, 2.5);
    let mut corpses = BTreeMap::new();
    note_corpses(&h, &mut corpses);
    assert_eq!(corpses.len(), 1, "one corpse");
    let held = corpses.values().next().expect("the corpse");
    assert_eq!(held.len(), yalain_world::death::MOST_ITEMS, "full");
    assert_eq!(
        held.iter().filter(|(t, _)| *t == TREASURE).count(),
        1,
        "the treasure once: {held:x?}"
    );
}

/// However fast its creatures are killed, a camp's treasure comes no faster than the camp makes
/// them again: killed the moment each appears for two minutes, a camp of one that comes back 30 s
/// after a death leaves at most five, one a death.
/// Behaviour: server.loot.death-treasure-at-the-camps-pace
#[test]
fn a_camps_death_treasure_comes_no_faster_than_the_camp_makes_its_creatures() {
    let mut h = camp_world(1, 30.0, vec![row(Dropper::Camp("test".into()), 1.0)], 9);
    let start = h.now.0;
    let mut corpses = BTreeMap::new();
    let mut died = 0;
    while h.now.0 < start + 120.0 {
        died += kill_all(&mut h);
        run_for(&mut h, 0.5);
        note_corpses(&h, &mut corpses);
    }
    run_for(&mut h, 2.5);
    note_corpses(&h, &mut corpses);
    assert!(
        (4..=5).contains(&died),
        "a death a period, and the first: {died}"
    );
    assert_eq!(holding(&corpses), died, "one a death, none more");
}
