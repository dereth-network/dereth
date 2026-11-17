//! Corpse finds: a world's find any creature's corpse may hold beside its profile's draw, rolled
//! once at each player's kill for a creature and killer its gates let through, its item drawn
//! evenly from the band of the killer's level. Fixture: the test world's level 2 beetle, two made
//! up stone templates and a token, a player as its killer.

use yalain_content::{CorpseFind, Prop, Props};
use yalain_testkit::{Harness, TEST_CREATURE};
use yalain_world::levels::{Level, LevelTable};
use yalain_world::rng::Rng;
use yalain_world::things::Thing;

/// Two stones of the first band, one of the second, and a token: items no profile of the test
/// world draws.
const LOW_A: u32 = 0x4700_2109;
const LOW_B: u32 = 0x4700_210A;
const HIGH: u32 = 0x4700_210B;
const TOKEN: u32 = 0x4700_210C;
/// The flag a creature's template sets to leave no stones.
const NEVER: &str = "NPC_NeverDropStones";

/// A find that every corpse its gates let through holds.
fn find(key: &str, bands: Vec<(i32, Vec<u32>)>) -> CorpseFind {
    CorpseFind {
        key: key.into(),
        chance: 1.0,
        never: None,
        least_level: None,
        below_killer: None,
        hero_cap: false,
        bands,
        placed: "authored for a test".into(),
        confidence: "authored".into(),
    }
}

/// The test world with its beetle, the finds given, a player in the world as its killer, the
/// server's draws seeded.
fn world(finds: Vec<CorpseFind>) -> (Harness, Thing, u64) {
    let mut h = Harness::new();
    h.add_creatures(1);
    for t in [LOW_A, LOW_B, HIGH, TOKEN] {
        h.server
            .catalog
            .insert(yalain_testkit::test_template(t, 0x3A1, Props::default()));
    }
    h.server.loot.finds = finds;
    h.server.rng = Rng::new(7);
    h.run(20);
    let creature = h
        .server
        .things
        .by_id
        .values()
        .find(|t| t.creature.is_some())
        .cloned()
        .expect("the camp's creature is made");
    let (_, me) = h.player_in_world("one", "Elara");
    (h, creature, me)
}

/// The killer's level, its hero skill credits held, and the beetle's level.
fn set(h: &mut Harness, creature: &mut Thing, me: u64, killer: i32, held: u32, dropper: i32) {
    let p = h
        .server
        .players
        .values_mut()
        .find(|p| p.character == me)
        .expect("in the world");
    yalain_world::quals::set_int(&mut p.qualities, yalain_world::examine::LEVEL, killer);
    p.play.hero_credits = held;
    if let Some(c) = creature.creature.as_mut() {
        c.stats.level = dropper;
    }
}

/// What the beetle leaves of the finds' items over some kills by a killer.
fn found(h: &mut Harness, creature: &Thing, killer: u64, kills: usize) -> Vec<u32> {
    (0..kills)
        .flat_map(|_| {
            let clock = h.clock();
            yalain_world::death::roll_loot(&mut h.server, creature, killer, clock)
        })
        .map(|(t, _)| t)
        .filter(|t| [LOW_A, LOW_B, HIGH, TOKEN].contains(t))
        .collect()
}

/// A corpse find's item is drawn evenly from the band of the killer's level, whatever the
/// creature's own level: a killer below the second band's least level draws from the first, one at
/// it or above from the second; a find with a chance under 1 is held by about that share of
/// corpses, and a kill no player made holds none (2017: lodestones on corpses by the killer's
/// level, Crumbling to 24, Rough from 40, ten of ten; these templates and chance are made up).
/// Behaviour: server.loot.corpse-find-by-killers-level
#[test]
fn a_corpse_finds_item_is_drawn_from_the_band_of_its_killers_level() {
    let stones = find("stones", vec![(0, vec![LOW_A, LOW_B]), (25, vec![HIGH])]);
    let (mut h, mut creature, me) = world(vec![stones]);
    set(&mut h, &mut creature, me, 24, 0, 30);
    let low = found(&mut h, &creature, me, 200);
    assert_eq!(low.len(), 200, "one a corpse");
    let a = low.iter().filter(|t| **t == LOW_A).count();
    assert!(
        low.iter().all(|t| [LOW_A, LOW_B].contains(t)) && (70..=130).contains(&a),
        "the first band, evenly: {a} of 200 the first"
    );
    set(&mut h, &mut creature, me, 25, 0, 2);
    assert!(
        found(&mut h, &creature, me, 50).iter().all(|t| *t == HIGH),
        "the second band, from its least level, for a creature far below it"
    );
    assert!(
        found(&mut h, &creature, 0, 20).is_empty(),
        "no player's kill"
    );
    h.server.loot.finds[0].chance = 0.25;
    let some = found(&mut h, &creature, me, 400).len();
    assert!((60..=140).contains(&some), "a quarter of 400: {some}");
}

/// A corpse find is held only past its gates: not by a creature whose template sets its never
/// flag, nor one below its least level, nor one further below its killer than it allows (2017:
/// none of 2,468 corpses more than a level below their killer held a lodestone, 9 of 1,464 at or
/// above; the install marks the creatures that never leave lodestones or Tesserae).
/// Behaviour: server.loot.corpse-find-gates
#[test]
fn a_corpse_find_is_held_only_by_a_creature_its_gates_let_through() {
    let stones = CorpseFind {
        never: Some(NEVER.into()),
        below_killer: Some(1),
        ..find("stones", vec![(0, vec![LOW_A])])
    };
    let token = CorpseFind {
        least_level: Some(42),
        ..find("tokens", vec![(0, vec![TOKEN])])
    };
    let (mut h, mut creature, me) = world(vec![stones, token]);
    let count = |got: &[u32], t: u32| got.iter().filter(|x| **x == t).count();
    // A level 40 killer: stones from a creature of 39 up, tokens from 42 up.
    for (dropper, stones, tokens) in [(38, 0, 0), (39, 10, 0), (41, 10, 0), (42, 10, 10)] {
        set(&mut h, &mut creature, me, 40, 0, dropper);
        let got = found(&mut h, &creature, me, 10);
        assert_eq!(
            (count(&got, LOW_A), count(&got, TOKEN)),
            (stones, tokens),
            "a creature of level {dropper}"
        );
    }
    if let Some(t) = h.server.catalog.templates.get_mut(&TEST_CREATURE) {
        t.props.set(NEVER, Prop::Bool(true));
    }
    let got = found(&mut h, &creature, me, 10);
    assert_eq!(
        (count(&got, LOW_A), count(&got, TOKEN)),
        (0, 10),
        "never stones from a creature marked so; its tokens still"
    );
}

/// A corpse find capped by hero skill credits is held only for a killer whose credits, held and
/// spent, are under its level's cap: none for a killer of a level with no cap, none for one at its
/// cap, one for one below it (2017: the seven Tesserae on corpses were all for killers of level 47
/// to 50, none of 2,232 corpses of killers below 45; the level table caps a level's hero skill
/// credits from 45; the table here is made up).
/// Behaviour: server.loot.corpse-find-under-the-hero-credit-cap
#[test]
fn a_corpse_find_capped_by_hero_credits_is_held_only_under_the_killers_cap() {
    let token = CorpseFind {
        hero_cap: true,
        ..find("tokens", vec![(0, vec![TOKEN])])
    };
    let (mut h, mut creature, me) = world(vec![token]);
    h.server.levels = LevelTable {
        levels: (1..=3_i32)
            .map(|l| Level {
                xp: u64::from(l.unsigned_abs()) * 100,
                hero_credit_cap: if l >= 2 { 125 * (l - 1) } else { 0 },
                ..Level::default()
            })
            .collect(),
    };
    for (level, held, tokens) in [(1, 0, 0), (2, 125, 0), (2, 124, 10), (3, 200, 10)] {
        set(&mut h, &mut creature, me, level, held, 2);
        assert_eq!(
            found(&mut h, &creature, me, 10).len(),
            tokens,
            "a level {level} killer holding {held}"
        );
    }
}
