//! A kill's experience paid in parts: one for each damager by the health it took (none past the
//! creature's death), a pet's to its owner, another creature's to no one; a part of a damager the
//! creature could not reach held back alone; a part worth nothing sending nothing. Fixture: the
//! test world's beetles, one of them made a player's pet, and the prey worth 1000 at the player's
//! level with 300 health.

use yalain_testkit::Harness;
use yalain_world::pets::{Order, Pet};
use yalain_world::progress::text;
use yalain_world::{combat, pets};

use super::{experience, Told};

const HEALTH: i32 = 300;

/// A player and three beetles: the prey (its experience and health set), the player's pet, and
/// another; returns the harness, the client, the player and the three.
fn world(death_xp: i64) -> (Harness, usize, u64, [u64; 3]) {
    let mut h = Harness::new();
    h.add_creatures(3);
    let (b, me) = h.player_in_world("one", "Kestrel");
    h.run(20);
    let level = h
        .server
        .players
        .values()
        .find(|p| p.character == me)
        .map(|p| yalain_world::examine::level_of(&p.qualities))
        .expect("in the world");
    let mut beetles: Vec<u64> = h
        .server
        .things
        .by_id
        .values()
        .filter(|t| t.creature.is_some())
        .map(|t| t.id)
        .collect();
    beetles.sort_unstable();
    let [prey, pet, other] = beetles[..] else {
        panic!("three beetles: {beetles:?}")
    };
    for id in beetles {
        if let Some(c) = h
            .server
            .things
            .get_mut(id)
            .and_then(|t| t.creature.as_mut())
        {
            c.stats.attacks_first = false;
        }
    }
    if let Some(t) = h.server.things.get_mut(prey) {
        t.health = HEALTH;
        t.max_health = HEALTH;
        if let Some(c) = t.creature.as_mut() {
            c.stats.death_xp = death_xp;
            c.stats.level = level;
        }
    }
    h.server.pets.by_id.insert(
        pet,
        Pet {
            id: pet,
            owner: me,
            template: 0,
            class: 0,
            flags: 0,
            mode: 0,
            name: None,
            order: Order::Follow,
            next_plan: 0.0,
            goal: None,
            effect: 0,
            assist: None,
            next_tend: 0.0,
        },
    );
    (h, b, me, [prey, pet, other])
}

/// The texts of a kill's experience and the totals a client was told since `from`.
fn told(h: &Harness, b: usize, from: usize) -> Vec<Told> {
    experience(h, b, from)
        .into_iter()
        .filter(|t| match t {
            Told::Text(id, _) => [text::GAINED.0, text::KILLED.0, text::UNFAIR.0].contains(id),
            Told::Total(_) => true,
        })
        .collect()
}

fn total_of(h: &Harness, me: u64) -> u64 {
    h.server
        .players
        .values()
        .find(|p| p.character == me)
        .and_then(|p| yalain_world::quals::long(&p.qualities, yalain_world::progress::q::TOTAL_XP))
        .unwrap_or(0)
}

/// A kill's experience is paid in parts in the order its damagers first hurt it: each the
/// creature's experience times the health that damager took over all the health taken (the
/// killing blow's past the creature's death not counted), rounded to the nearest. The player's
/// part and its pet's are each paid to the player with "You have gained %1 experience points!"
/// and the totals; another creature's part is paid to no one; "You have killed the %1." follows.
/// Behaviour: server.death.experience-in-parts
#[test]
fn a_kills_experience_is_paid_in_parts_by_the_health_each_damager_took_a_pets_to_its_owner() {
    let (mut h, b, me, [prey, pet, other]) = world(1000);
    let was = total_of(&h, me);
    let from = h.bots[b].inbox.len();
    let clock = h.clock();
    // The player takes 100, its pet 151, another creature 30; the player's blow of 50 finds 19.
    combat::hurt_creature(&mut h.server, &mut h.net, prey, me, 100, clock);
    pets::struck(&mut h.server, &mut h.net, prey, (pet, 0), 151, clock);
    combat::hurt_creature(&mut h.server, &mut h.net, prey, other, 30, clock);
    combat::hurt_creature(&mut h.server, &mut h.net, prey, me, 50, clock);
    h.run(10);
    // 1000 x 119 / 300 = 396.7 and 1000 x 151 / 300 = 503.3; the other's 100 goes to no one.
    assert_eq!(
        told(&h, b, from),
        vec![
            Told::Text(text::GAINED.0, Some(397)),
            Told::Total(was + 397),
            Told::Text(text::GAINED.0, Some(503)),
            Told::Total(was + 900),
            Told::Text(text::KILLED.0, None),
        ]
    );
}

/// The part of a damager the creature could not reach is held back alone, and the player it was
/// for told once "You receive no experience, since that wasn't a fair fight." in its place among
/// the parts: the pet's held back, the player's own is paid; the player's held back, its pet's is
/// paid; both held back, the player told once and paid nothing.
/// Behaviour: server.death.unfair-part-held-back
#[test]
fn the_part_of_a_damager_the_creature_could_not_reach_is_held_back_and_the_others_paid() {
    for (unreached, want) in [
        (
            &[1usize][..],
            vec![
                Told::Text(text::GAINED.0, Some(333)),
                Told::Total(333),
                Told::Text(text::UNFAIR.0, None),
                Told::Text(text::KILLED.0, None),
            ],
        ),
        (
            &[0],
            vec![
                Told::Text(text::UNFAIR.0, None),
                Told::Text(text::GAINED.0, Some(667)),
                Told::Total(667),
                Told::Text(text::KILLED.0, None),
            ],
        ),
        (
            &[0, 1],
            vec![
                Told::Text(text::UNFAIR.0, None),
                Told::Text(text::KILLED.0, None),
            ],
        ),
    ] {
        let (mut h, b, me, [prey, pet, _]) = world(1000);
        assert_eq!(total_of(&h, me), 0, "a new player");
        let damagers = [me, pet];
        if let Some(c) = h
            .server
            .things
            .get_mut(prey)
            .and_then(|t| t.creature.as_mut())
        {
            c.unreached = unreached.iter().map(|i| damagers[*i]).collect();
        }
        let from = h.bots[b].inbox.len();
        let clock = h.clock();
        combat::hurt_creature(&mut h.server, &mut h.net, prey, me, 100, clock);
        pets::struck(&mut h.server, &mut h.net, prey, (pet, 0), 200, clock);
        h.run(10);
        assert_eq!(told(&h, b, from), want, "could not reach {unreached:?}");
    }
}

/// A part worth nothing sends nothing: a creature worth no experience sends no "You have gained"
/// and no totals, only "You have killed the %1."; nor does a part that rounds to nothing.
/// Behaviour: server.death.nothing-worth-sends-nothing
#[test]
fn a_part_worth_nothing_sends_no_text_and_no_totals() {
    let (mut h, b, me, [prey, _, _]) = world(0);
    let from = h.bots[b].inbox.len();
    let clock = h.clock();
    combat::hurt_creature(&mut h.server, &mut h.net, prey, me, HEALTH, clock);
    h.run(10);
    assert_eq!(told(&h, b, from), vec![Told::Text(text::KILLED.0, None)]);
    // Worth 1: the player's 1 of 300 rounds to nothing.
    let (mut h, b, me, [prey, _, other]) = world(1);
    let from = h.bots[b].inbox.len();
    let clock = h.clock();
    combat::hurt_creature(&mut h.server, &mut h.net, prey, other, HEALTH - 1, clock);
    combat::hurt_creature(&mut h.server, &mut h.net, prey, me, 1, clock);
    h.run(10);
    assert_eq!(told(&h, b, from), vec![Told::Text(text::KILLED.0, None)]);
}

/// A pet's skill and a player's basic attack count at a creature's draw by their points too; a blow
/// of a skill with none counts as it fell.
/// Behaviour: server.creatures.a-skills-harm-counts-by-its-points
#[test]
fn a_pets_skill_and_a_basic_attack_count_at_a_draw_by_their_points() {
    let (mut h, _, me, [prey, pet, _]) = world(1000);
    let harm = |h: &Harness| -> Vec<i64> {
        h.server
            .things
            .get(prey)
            .and_then(|t| t.creature.as_ref())
            .expect("a creature")
            .harm
            .iter()
            .map(|e| e.2)
            .collect()
    };
    if let Some(t) = h.server.things.get_mut(prey) {
        t.max_health = 1_000_000;
        t.health = 1_000_000;
    }
    let clock = h.clock();
    pets::struck(&mut h.server, &mut h.net, prey, (pet, 1076), 1000, clock);
    pets::struck(&mut h.server, &mut h.net, prey, (pet, 0), 1000, clock);
    assert_eq!(harm(&h), [1035, 1000]);
    let world = h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .map(|(c, _)| *c)
        .expect("in the world");
    combat::lands(
        &mut h.server,
        &mut h.net,
        combat::Landing {
            world,
            target: prey,
            skill: 2189,
            outcome: yalain_world::rules::Outcome::Hit,
            damage: 1000,
            back: 0,
            index: 0,
            thrown: false,
            anim_hook: 1,
        },
        clock,
    );
    assert_eq!(harm(&h)[2], 1050);
}

/// A level table of `top` levels, each taking 1,000 more than the one before.
fn thousand_a_level(top: i32) -> yalain_world::levels::LevelTable {
    use yalain_world::levels::{Level, LevelTable};
    LevelTable {
        levels: (0..u64::try_from(top).expect("a count"))
            .map(|n| Level {
                xp: n * 1_000,
                ..Level::default()
            })
            .collect(),
    }
}

/// Experience enough for level 52 takes a character that is no hero to level 50 and no further,
/// its total still counted in full; made a hero, its next award brings the levels it had earned;
/// a hero all along goes straight there.
/// Behaviour: server.experience.a-non-hero-stops-at-level-fifty
#[test]
fn a_character_that_is_no_hero_stops_at_level_fifty_until_it_is_one() {
    use yalain_world::progress::{award, q, Award};
    use yalain_world::{focus, quals};
    let mut h = Harness::new();
    let (_, me) = h.player_in_world("one", "Kestrel");
    h.run(20);
    h.server.levels = thousand_a_level(60);
    let world = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .map(|(w, _)| w)
        .expect("in the world");
    let read = |h: &Harness| {
        let p = h.server.players.get(&world).expect("in the world");
        (
            quals::int(&p.qualities, q::LEVEL).unwrap_or(0),
            quals::long(&p.qualities, q::TOTAL_XP).unwrap_or(0),
        )
    };
    let had = read(&h).1;
    award(&mut h.server, &mut h.net, world, 51_000, Award::Step);
    assert_eq!(
        read(&h),
        (50, had + 51_000),
        "stopped at 50, all of it kept"
    );
    award(&mut h.server, &mut h.net, world, 1_000, Award::Step);
    assert_eq!(read(&h), (50, had + 52_000), "still 50");
    let p = h.server.players.get_mut(&world).expect("in the world");
    quals::set_bool(&mut p.qualities, focus::HERO, true);
    award(&mut h.server, &mut h.net, world, 1, Award::Step);
    assert_eq!(read(&h).0, 53, "a hero has what it earned");
}
