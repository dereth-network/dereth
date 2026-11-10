//! The operator's game rates: the experience a player earns, the treasure a creature draws and a
//! camp's period, each a multiple of the game's own; at 1, as a world that sets none has them, the
//! game is as it is without them.
//! Fixture: the test world's beetles, whose profile always draws one test item, and whose camp
//! makes a replacement 60 s after a death.

use yalain_store::MemoryStore;
use yalain_testkit::{test_rules, Harness};
use yalain_world::progress::{self, q, Award};
use yalain_world::rates::Rates;
use yalain_world::{quals, ServerConfig};

/// The test world started with these rates.
fn with(rates: Rates) -> Harness {
    let cfg = ServerConfig {
        rates,
        ..ServerConfig::default()
    };
    Harness::with(cfg, MemoryStore::default(), test_rules())
}

fn world_of(h: &Harness, id: u64) -> yalain_net::ConnId {
    h.server
        .players
        .iter()
        .find(|(_, p)| p.character == id)
        .map(|(w, _)| *w)
        .expect("in the world")
}

fn total_xp(h: &Harness, w: yalain_net::ConnId) -> u64 {
    let p = h.server.players.get(&w).expect("in the world");
    quals::long(&p.qualities, q::TOTAL_XP).unwrap_or(0)
}

/// The experience rate multiplies what a player earns, rounded to the nearest: a quest step's, a
/// quest's, a part of a kill and a fellow's share. What vassals pass up, and an admin's award, are
/// the numbers given. A world that sets no rate pays the game's own.
/// Behaviour: server.operator.experience-rate
#[test]
fn the_experience_rate_multiplies_what_a_player_earns_and_not_what_is_given() {
    for (rate, earned) in [(1.0, 100), (2.5, 250), (0.5, 50), (0.0, 0), (1.004, 100)] {
        let mut h = with(Rates {
            experience: rate,
            ..Rates::default()
        });
        let (_, id) = h.player_in_world("one", "Elara");
        let w = world_of(&h, id);
        for why in [
            Award::Step,
            Award::Quest((0x1111_1111, 0x2500_0001)),
            Award::Part,
            Award::Share,
        ] {
            let before = total_xp(&h, w);
            progress::award(&mut h.server, &mut h.net, w, 100, why);
            assert_eq!(total_xp(&h, w) - before, earned, "{why:?} at {rate}");
        }
        for why in [Award::Allegiance, Award::Given] {
            let before = total_xp(&h, w);
            progress::award(&mut h.server, &mut h.net, w, 100, why);
            assert_eq!(total_xp(&h, w) - before, 100, "{why:?} at {rate}");
        }
    }
    let mut h = Harness::new();
    let (_, id) = h.player_in_world("one", "Elara");
    let w = world_of(&h, id);
    progress::award(&mut h.server, &mut h.net, w, 123, Award::Step);
    assert_eq!(total_xp(&h, w), 123, "a world that sets none");
}

/// The test world with these rates and its one beetle, and the beetle.
fn beetle(rates: Rates) -> (Harness, yalain_world::things::Thing) {
    let mut h = with(rates);
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

/// How many items the beetle leaves at each of so many deaths.
fn left(h: &mut Harness, creature: &yalain_world::things::Thing, deaths: usize) -> Vec<usize> {
    (0..deaths)
        .map(|_| yalain_world::death::roll_loot(&mut h.server, creature, 0).len())
        .collect()
}

/// The loot rate multiplies how many items a creature's profile draws, a part of one a chance of
/// one more, and a corpse holds no more than it can; at 1 the draw is the profile's and no die is
/// cast for the rate.
/// Behaviour: server.operator.loot-rate
#[test]
fn the_loot_rate_multiplies_the_treasure_a_creature_draws() {
    for (rate, each) in [(1.0, 1), (3.0, 3), (0.0, 0), (100.0, 21)] {
        let (mut h, creature) = beetle(Rates {
            loot: rate,
            ..Rates::default()
        });
        let counts = left(&mut h, &creature, 20);
        assert!(counts.iter().all(|n| *n == each), "{rate}: {counts:?}");
    }
    let (mut h, creature) = beetle(Rates {
        loot: 2.5,
        ..Rates::default()
    });
    let counts = left(&mut h, &creature, 400);
    assert!(counts.iter().all(|n| [2, 3].contains(n)), "{counts:?}");
    let threes = counts.iter().filter(|n| **n == 3).count();
    assert!((140..=260).contains(&threes), "about half of 400: {threes}");
    let mut dice = h.server.rng.clone();
    let before = dice.clone();
    assert_eq!(yalain_world::rates::loot_count(&mut dice, 1.0, 4), 4);
    assert_eq!(dice, before, "no die cast at 1");
}

/// The respawn rate divides a camp's period: at 2 a dead creature's replacement is due in half the
/// camp's 60 s, at 0.5 in twice it; a world that sets none keeps the camp's own.
/// Behaviour: server.operator.respawn-rate
#[test]
fn the_respawn_rate_divides_a_camps_period() {
    for (rate, after) in [(1.0, 60.0), (2.0, 30.0), (0.5, 120.0)] {
        let (mut h, creature) = beetle(Rates {
            respawn: rate,
            ..Rates::default()
        });
        yalain_world::spawn::died(&mut h.server, 0, creature.id, 1000.0);
        assert_eq!(h.server.camps[0].due, [1000.0 + after], "{rate}");
    }
    let (mut h, creature) = beetle(Rates::default());
    yalain_world::spawn::died(&mut h.server, 0, creature.id, 1000.0);
    assert_eq!(h.server.camps[0].due, [1060.0], "a world that sets none");
}

/// A world that sets no rate has each at 1, and names none as changed; one that sets some names
/// those that aren't 1, by name.
/// Behaviour: server.operator.game-rates-on-the-status-endpoint
#[test]
fn the_rates_not_at_one_are_named_and_a_world_that_sets_none_names_none() {
    let none = Harness::new().server.cfg.rates;
    assert_eq!(none, Rates::default());
    assert_eq!(
        none.all(),
        [("experience", 1.0), ("loot", 1.0), ("respawn", 1.0)]
    );
    assert!(none.changed().is_empty());
    let some = Rates {
        experience: 2.0,
        respawn: 0.5,
        ..Rates::default()
    };
    assert_eq!(some.changed(), [("experience", 2.0), ("respawn", 0.5)]);
}
