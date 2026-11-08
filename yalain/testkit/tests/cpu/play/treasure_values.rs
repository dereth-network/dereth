//! What treasure a creature leaves is worth: its own value drawn from its world's values for the
//! creature's level, and none for a rare or a trump of the tarot, which keep their template's.

use yalain_content::{Prop, Props};
use yalain_testkit::{Harness, TEST_ITEM};
use yalain_world::things::Thing;

/// Two item templates made for these tests: a rare, and a trump (of a package taken as a trump's).
const RARE_ITEM: u32 = 0x4700_2109;
const TRUMP_ITEM: u32 = 0x4700_210A;
const TRUMP_PACKAGE: u32 = 0x777;

/// The test world with its creature: the level 2 beetle, whose profile leaves one test item.
fn world() -> (Harness, Thing) {
    let mut h = Harness::new();
    h.add_creatures(1);
    h.run(20);
    let creature = h
        .server
        .things
        .by_id
        .values()
        .find(|t| t.creature.is_some())
        .cloned()
        .expect("the camp's creature is made");
    (h, creature)
}

/// The own values of the treasure the creature leaves at a level over some draws.
fn drawn(h: &mut Harness, creature: &mut Thing, level: i32, draws: usize) -> Vec<u32> {
    if let Some(c) = creature.creature.as_mut() {
        c.stats.level = level;
    }
    (0..draws)
        .flat_map(|_| yalain_world::death::roll_loot(&mut h.server, creature, 0))
        .map(|(_, value)| value.expect("the profile's item is treasure"))
        .collect()
}

/// A treasure item's own value is one of its world's values for its dropper's level, each drawn
/// evenly as often as it is listed; a level with none of its own draws the nearest level's, the
/// lower of two as near (2017: each dropped item's own value was one recorded at its dropper's
/// level; this world's values are made up).
/// Behaviour: server.loot.treasure-value-by-level-row
#[test]
fn a_treasure_items_own_value_is_drawn_from_its_worlds_values_for_its_droppers_level() {
    let (mut h, mut creature) = world();
    h.server.loot.values = [(2, vec![7, 9, 9, 9]), (5, vec![100])].into();
    let at_two = drawn(&mut h, &mut creature, 2, 400);
    assert!(at_two.iter().all(|v| [7, 9].contains(v)), "{at_two:?}");
    let sevens = at_two.iter().filter(|v| **v == 7).count();
    assert!((60..=140).contains(&sevens), "a quarter of 400: {sevens}");
    assert!(drawn(&mut h, &mut creature, 5, 20)
        .iter()
        .all(|v| *v == 100));
    for (level, nearest) in [(1, 2), (3, 2), (4, 5), (9, 5)] {
        let values = drawn(&mut h, &mut creature, level, 40);
        let expected: &[u32] = if nearest == 2 { &[7, 9] } else { &[100] };
        assert!(
            values.iter().all(|v| expected.contains(v)),
            "level {level} draws level {nearest}'s: {values:?}"
        );
    }
}

/// A rare and a trump of the tarot left as treasure carry no own value, so selling one pays its
/// template's, in a world that gives values and in one that gives none alike; other treasure draws
/// its own (2017: none of the 44 trumps and 7 rares recorded dropped carried one, against 5,135 of
/// the other 5,154 treasure items).
/// Behaviour: server.loot.treasure-value-none-for-trumps-and-rares
#[test]
fn a_rare_and_a_trump_left_as_treasure_carry_no_value_of_their_own() {
    let (mut h, mut creature) = world();
    let mut rare = Props::default();
    rare.set(yalain_world::treasure::RARE, Prop::Bool(true));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(RARE_ITEM, 0x200, rare));
    h.server.catalog.insert(yalain_testkit::test_template(
        TRUMP_ITEM,
        TRUMP_PACKAGE,
        Props::default(),
    ));
    h.server.treasure.trumps.insert(TRUMP_PACKAGE);
    for values in [[(2, vec![7])].into(), std::collections::BTreeMap::new()] {
        let rows = !values.is_empty();
        h.server.loot.values = values;
        for item in [TEST_ITEM, RARE_ITEM, TRUMP_ITEM] {
            h.server.loot.profiles[0].items = vec![(item, 1)];
            let got = drawn(&mut h, &mut creature, 2, 10);
            let expected = |v: &u32| match (item, rows) {
                (TEST_ITEM, true) => *v == 7,
                (TEST_ITEM, false) => (1..=4).contains(v),
                _ => *v == 0,
            };
            assert!(
                got.len() == 10 && got.iter().all(expected),
                "{item:#010x}, rows {rows}: {got:?}"
            );
        }
    }
    assert!(yalain_world::death::keeps_template_value(
        &h.server, TRUMP_ITEM
    ));
    assert!(!yalain_world::death::keeps_template_value(
        &h.server, TEST_ITEM
    ));
}

/// In a world that gives treasure no values, a creature's treasure is worth one of the server's own
/// values for its level's band, each whole number from the band's least to its most drawn evenly,
/// the top band's from level 50 on (2017's own values by dropper level, the tenth to the ninetieth
/// percentile of each band).
/// Behaviour: server.loot.treasure-value-bands-without-rows
#[test]
fn treasure_in_a_world_with_no_values_is_worth_the_servers_own_by_level_band() {
    let (mut h, mut creature) = world();
    assert!(h.server.loot.values.is_empty(), "the test world gives none");
    for (level, least, most) in [
        (2, 1, 4),
        (9, 1, 4),
        (10, 4, 11),
        (25, 10, 24),
        (39, 21, 34),
        (40, 41, 84),
        (50, 75, 156),
        (90, 75, 156),
    ] {
        assert_eq!(
            yalain_world::death::fallback_value_band(level),
            (least, most),
            "level {level}"
        );
        let values = drawn(&mut h, &mut creature, level, 2_000);
        let seen: std::collections::BTreeSet<u32> = values.iter().copied().collect();
        assert_eq!(
            seen,
            (least..=most).collect(),
            "level {level} draws each of its band's values and no other"
        );
    }
    let values = drawn(&mut h, &mut creature, 2, 4_000);
    for v in 1..=4 {
        let n = values.iter().filter(|x| **x == v).count();
        assert!(
            (800..=1_200).contains(&n),
            "a quarter of 4,000 are {v}: {n}"
        );
    }
}
