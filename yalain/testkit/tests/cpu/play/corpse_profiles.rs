//! A corpse's own profile: a creature whose template names its own corpse in place of the common
//! one, where that corpse's template names a generator profile, has the profile's row drawn into its
//! corpse beside what it leaves itself, and kept there however full the corpse. Fixture: the test
//! world's camp of beetles, its regeneration shortened, a corpse template of its own and two items.

use std::collections::BTreeMap;

use yalain_content::{LootProfile, Prop, Props};
use yalain_testkit::{Harness, TEST_CREATURE, TEST_ITEM};
use yalain_world::death::{CORPSE_OVERRIDE, GENERATOR_PROFILE, MOST_ITEMS};
use yalain_world::rng::Rng;
use yalain_world::things::Mode;

/// The beetle's own corpse, which names the corpse profile.
const OWN_CORPSE: u32 = 0x4700_2226;
/// A corpse template that names no profile.
const PLAIN_CORPSE: u32 = 0x4700_2227;
/// The corpse profile's value and name.
const CORPSE_PROFILE: u32 = 0x2A0;
const CORPSE_PROFILE_NAME: &str = "GP_TestBeetleCorpse";
/// What the corpse profile draws, and its extra.
const KEEPSAKE: u32 = 0x4700_2109;
const EXTRA: u32 = 0x4700_210A;

/// How the beetle's corpse is named.
#[derive(Clone, Copy)]
enum Corpse {
    /// The common corpse.
    Common,
    /// Its own corpse, which names the corpse profile.
    Own,
    /// Its own corpse, which names no profile.
    Plain,
}

/// The test world with a camp of `population` beetles back a second after each body leaves, its
/// corpse named as `corpse` says, the corpse profile's row if `row`, the server's draws seeded.
fn camp_world(population: u32, corpse: Corpse, row: bool, seed: u64) -> Harness {
    let mut h = Harness::new();
    h.add_creatures(population);
    h.server.spawns.camps[0].regen = 1.0;
    for t in [KEEPSAKE, EXTRA] {
        h.server
            .catalog
            .insert(yalain_testkit::test_template(t, 0x3A1, Props::default()));
    }
    let mut own = Props::default();
    own.set(GENERATOR_PROFILE, Prop::Word(CORPSE_PROFILE));
    h.server.catalog.insert(yalain_testkit::test_template(
        OWN_CORPSE,
        yalain_world::death::CORPSE_PACKAGE,
        own,
    ));
    h.server.catalog.insert(yalain_testkit::test_template(
        PLAIN_CORPSE,
        yalain_world::death::CORPSE_PACKAGE,
        Props::default(),
    ));
    h.server
        .catalog
        .profiles
        .insert(CORPSE_PROFILE, CORPSE_PROFILE_NAME.into());
    let named = match corpse {
        Corpse::Common => None,
        Corpse::Own => Some(OWN_CORPSE),
        Corpse::Plain => Some(PLAIN_CORPSE),
    };
    if let Some(c) = named {
        h.server
            .catalog
            .templates
            .get_mut(&TEST_CREATURE)
            .expect("the beetle")
            .props
            .set(CORPSE_OVERRIDE, Prop::Word(c));
    }
    if row {
        h.server.loot.profiles.push(LootProfile {
            name: CORPSE_PROFILE_NAME.into(),
            counts: vec![0.0, 1.0],
            items: vec![(KEEPSAKE, 1)],
            extras: vec![(EXTRA, 0.25)],
            placed: "authored for a test".into(),
            confidence: "authored".into(),
            chest: None,
        });
    }
    h.server.rng = Rng::new(seed);
    h.run(20);
    h
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

fn run_for(h: &mut Harness, seconds: f64) {
    let t = h.now.0 + seconds;
    while h.now.0 < t {
        h.step_by(0.25);
    }
}

/// Kill the camp's beetles `rounds` times over, each round's corpses made before the next: what
/// every corpse held, by template, in its order.
fn rounds(h: &mut Harness, rounds: usize) -> BTreeMap<u64, Vec<u32>> {
    let mut seen = BTreeMap::new();
    for _ in 0..rounds {
        for id in living(h) {
            let clock = h.clock();
            yalain_world::death::creature_dies(&mut h.server, &mut h.net, id, 0, clock);
        }
        run_for(h, 2.5);
        for t in h.server.things.by_id.values() {
            let Some(c) = t.corpse.as_ref() else { continue };
            seen.entry(t.id).or_insert_with(|| {
                c.contents
                    .iter()
                    .filter_map(|i| h.server.things.get(*i))
                    .map(|i| i.template)
                    .collect()
            });
        }
        // The bodies leave, and the camp's period runs from then.
        run_for(h, 5.5);
    }
    seen
}

fn count(held: &[u32], t: u32) -> usize {
    held.iter().filter(|x| **x == t).count()
}

/// A creature whose own corpse names a profile leaves the profile's draw on its corpse beside what
/// it leaves itself: over 200 kills every corpse holds its own loot and the drawn item once, after
/// it, and the profile's extra about a quarter of the time, as its row says.
/// Behaviour: server.loot.corpse-profile-beside-its-loot
#[test]
fn a_creatures_own_corpse_holds_its_profiles_draw_beside_its_loot() {
    let corpses = rounds(&mut camp_world(10, Corpse::Own, true, 7), 20);
    assert_eq!(corpses.len(), 200, "a corpse for every death");
    for held in corpses.values() {
        assert_eq!(
            held.first(),
            Some(&TEST_ITEM),
            "its own loot first: {held:x?}"
        );
        assert_eq!(count(held, KEEPSAKE), 1, "the drawn item once: {held:x?}");
        assert!(
            held.iter().position(|t| *t == KEEPSAKE) > Some(0),
            "after its own loot: {held:x?}"
        );
    }
    let extras = corpses.values().filter(|c| count(c, EXTRA) == 1).count();
    assert!(
        (30..=70).contains(&extras),
        "the extra about a quarter of 200 corpses: {extras}"
    );
}

/// Only a corpse that names a profile with a row is filled from it: a creature with the common
/// corpse, its own corpse naming no profile, or its own corpse whose profile has no row leaves the
/// same corpses, draw for draw, as it does without any of them.
/// Behaviour: server.loot.corpse-profile-only-its-own
#[test]
fn a_creature_whose_corpse_names_no_profile_with_a_row_leaves_what_it_did_before() {
    let plain = rounds(&mut camp_world(5, Corpse::Common, true, 11), 10);
    assert_eq!(plain.len(), 50);
    assert!(plain.values().all(|c| c == &[TEST_ITEM]), "{plain:x?}");
    for (corpse, row) in [
        (Corpse::Plain, true),
        (Corpse::Own, false),
        (Corpse::Common, false),
    ] {
        let other = rounds(&mut camp_world(5, corpse, row, 11), 10);
        assert_eq!(other, plain, "the same corpses, the same draws");
    }
}

/// A corpse full from what the creature leaves itself still holds its profile's draw: the
/// creature's own items give way to it, and the corpse holds no more than its most.
/// Behaviour: server.loot.corpse-profile-kept-in-a-full-corpse
#[test]
fn a_corpse_full_from_its_creatures_loot_still_holds_its_profiles_draw() {
    let mut h = camp_world(1, Corpse::Own, true, 5);
    h.server.loot.profiles[0].counts = vec![0.0; MOST_ITEMS + 1];
    h.server.loot.profiles[0].counts.push(1.0);
    let corpses = rounds(&mut h, 8);
    assert_eq!(corpses.len(), 8);
    for held in corpses.values() {
        assert_eq!(held.len(), MOST_ITEMS, "full: {held:x?}");
        assert_eq!(count(held, KEEPSAKE), 1, "the drawn item kept: {held:x?}");
        assert!(count(held, TEST_ITEM) >= MOST_ITEMS - 2, "{held:x?}");
    }
}
