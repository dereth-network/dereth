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
            missed: false,
            closing: None,
            vigor: None,
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

/// A kill's part is paid times the Fast Learner its player holds, floored, and the text says so.
/// Behaviour: server.death.experience-with-fast-learner
#[test]
fn a_kills_part_is_paid_times_the_fast_learner_its_player_holds() {
    use yalain_world::skills::{Perk, SkillDef};
    const FAST_LEARNER: u32 = 1983;
    let (mut h, b, me, [prey, _, _]) = world(1000);
    h.server.skill_table.skills.insert(
        FAST_LEARNER,
        SkillDef {
            perk: Some(Perk {
                kind: 20,
                value: 1.03,
                priority: 1,
            }),
            ..SkillDef::default()
        },
    );
    for p in h.server.players.values_mut() {
        p.play.skills.push(yalain_world::state::SkillState {
            skill: FAST_LEARNER,
            granted: 0,
            mask: yalain_world::skills::mask::TRAINED,
            cost: 0,
            children: 0,
            dependents: 0,
            spent: 0,
        });
    }
    let was = total_of(&h, me);
    let from = h.bots[b].inbox.len();
    let clock = h.clock();
    combat::hurt_creature(&mut h.server, &mut h.net, prey, me, 10_000, clock);
    h.run(10);
    assert_eq!(
        told(&h, b, from),
        vec![
            Told::Text(text::GAINED.0, Some(1030)),
            Told::Total(was + 1030),
            Told::Text(text::KILLED.0, None),
        ]
    );
    // What a quest or an allegiance pays is no kill's.
    let was = total_of(&h, me);
    let world = *h.server.players.keys().next().expect("in the world");
    yalain_world::progress::award(
        &mut h.server,
        &mut h.net,
        world,
        1000,
        yalain_world::progress::Award::Step,
    );
    assert_eq!(total_of(&h, me), was + 1000);
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

/// An hour-long effect that doubles what kills pay at full spellcraft: the install's own (a
/// factor on the hunter's kill experience, 1 at spellcraft 0 to 2 at 100, for 3,601 seconds, of
/// its class).
fn doubling_hour() -> yalain_world::effect_defs::EffectDef {
    use yalain_world::effect_defs::{EffectDef, Kind, Scale};
    EffectDef {
        state: DOUBLING,
        kind: Kind::Numeric {
            flags: 0xA,
            stats: vec![yalain_world::effects::KILL_EXPERIENCE],
            magnitude: Scale(vec![(0.0, 1.0), (100.0, 2.0)]),
        },
        duration: Scale(vec![(0.0, 3601.0)]),
        priority: Scale(vec![(0.0, 2.0)]),
        fx: 0,
        external_flags: 0xA,
        internal_flags: 0x0100_0713,
        enum_value: 6191,
        eq_class: 4049,
        name: None,
    }
}

const DOUBLING: u32 = 0x6F00_1C71;

/// A change of one quality, by these flags, of a class.
fn change(
    state: u32,
    stat: u32,
    flags: u32,
    by: f32,
    class: u32,
) -> yalain_world::effect_defs::EffectDef {
    use yalain_world::effect_defs::{EffectDef, Kind, Scale};
    EffectDef {
        state,
        kind: Kind::Numeric {
            flags,
            stats: vec![stat],
            magnitude: Scale(vec![(0.0, by)]),
        },
        duration: Scale(vec![(0.0, 300.0)]),
        priority: Scale(vec![(0.0, 1.0)]),
        fx: 0,
        external_flags: flags,
        internal_flags: 0x0100_0713,
        enum_value: 0,
        eq_class: class,
        name: None,
    }
}

fn world_conn(h: &Harness, me: u64) -> yalain_net::ConnId {
    *h.server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .expect("in the world")
        .0
}

/// Make a beetle worth `death_xp` at the player's level, with the prey's health.
fn worth(h: &mut Harness, id: u64, like: u64, death_xp: i64) {
    let level = h
        .server
        .things
        .get(like)
        .and_then(|t| t.creature.as_ref())
        .map(|c| c.stats.level)
        .expect("the prey");
    if let Some(t) = h.server.things.get_mut(id) {
        t.health = HEALTH;
        t.max_health = HEALTH;
        if let Some(c) = t.creature.as_mut() {
            c.stats.death_xp = death_xp;
            c.stats.level = level;
        }
    }
}

/// A player holding the hour-long effect that doubles kills, cast at full spellcraft, is paid
/// exactly twice a kill's experience while it runs, and the kill's own experience once it has run
/// out.
/// Behaviour: server.death.experience-doubled-for-the-hour
#[test]
fn a_kill_pays_twice_during_the_doubling_hour_and_once_after_it() {
    use yalain_world::effects::{self, Cast};
    let (mut h, b, me, [prey, _, other]) = world(1000);
    worth(&mut h, other, prey, 1000);
    h.server
        .effect_table
        .effects
        .insert(DOUBLING, doubling_hour());
    let world = world_conn(&h, me);
    let clock = h.clock();
    let start = clock.local;
    assert!(effects::apply(
        &mut h.server,
        &mut h.net,
        world,
        DOUBLING,
        Cast {
            caster: me,
            spellcraft: 100.0,
            ..Cast::default()
        },
        clock,
    ));
    h.run(5);
    let was = total_of(&h, me);
    let from = h.bots[b].inbox.len();
    let clock = h.clock();
    combat::hurt_creature(&mut h.server, &mut h.net, prey, me, 10_000, clock);
    h.run(10);
    assert_eq!(
        told(&h, b, from),
        vec![
            Told::Text(text::GAINED.0, Some(2000)),
            Told::Total(was + 2000),
            Told::Text(text::KILLED.0, None),
        ]
    );
    // The hour runs out, and with it the doubling.
    h.run_to(yalain_net::LocalTime(start.0 + 3605.0));
    let holds = |h: &Harness| {
        h.server.players[&world]
            .play
            .effects
            .iter()
            .any(|e| e.state == DOUBLING)
    };
    assert!(!holds(&h), "the record has run out");
    let was = total_of(&h, me);
    let from = h.bots[b].inbox.len();
    let clock = h.clock();
    combat::hurt_creature(&mut h.server, &mut h.net, other, me, 10_000, clock);
    h.run(10);
    assert_eq!(
        told(&h, b, from),
        vec![
            Told::Text(text::GAINED.0, Some(1000)),
            Told::Total(was + 1000),
            Told::Text(text::KILLED.0, None),
        ]
    );
}

/// What a player's effects make of its kills goes by each effect's own flags, the top effect of
/// each class in turn, the highest class first: a doubling of a higher class and an addition of a
/// tenth of a lower one make a factor of 2.1 (not 2.2), held as a single-precision number a hair
/// under it, so a kill of 1000 pays 2099; a second doubling of the first one's class changes
/// nothing; and what a creature's own effects make of its worth comes first, its product cut to a
/// whole number (1000 times 1.03 is worth 1029).
/// Behaviour: server.death.experience-boosts-by-their-flags
#[test]
fn boosts_change_a_kill_by_their_flags_the_highest_class_first() {
    use yalain_world::effects::{self, Cast, On, DEATH_EXPERIENCE, KILL_EXPERIENCE};
    const TENTH: u32 = 0x6F00_20F5;
    const SECOND: u32 = 0x6F00_24B6;
    const INFLATE: u32 = 0x6F00_263A;
    let (mut h, b, me, [prey, _, other]) = world(1000);
    worth(&mut h, other, prey, 1000);
    for d in [
        doubling_hour(),
        change(TENTH, KILL_EXPERIENCE, 0x6, 0.1, 1127),
        change(SECOND, KILL_EXPERIENCE, 0xA, 2.0, 4049),
        change(INFLATE, DEATH_EXPERIENCE, 0x9, 1.03, 5064),
    ] {
        h.server.effect_table.effects.insert(d.state, d);
    }
    let world = world_conn(&h, me);
    let at = |spellcraft| Cast {
        caster: me,
        spellcraft,
        ..Cast::default()
    };
    let clock = h.clock();
    for state in [DOUBLING, TENTH, SECOND] {
        assert!(effects::apply(
            &mut h.server,
            &mut h.net,
            world,
            state,
            at(100.0),
            clock
        ));
    }
    h.run(5);
    let was = total_of(&h, me);
    let from = h.bots[b].inbox.len();
    let clock = h.clock();
    combat::hurt_creature(&mut h.server, &mut h.net, prey, me, 10_000, clock);
    h.run(10);
    assert_eq!(
        told(&h, b, from),
        vec![
            Told::Text(text::GAINED.0, Some(2099)),
            Told::Total(was + 2099),
            Told::Text(text::KILLED.0, None),
        ]
    );
    // The creature's worth: 1000 x 1.03 = 1029.99..., cut to 1029, then 2.1 times it, floored.
    let clock = h.clock();
    assert!(effects::apply_on(
        &mut h.server,
        &mut h.net,
        On::Creature(other),
        INFLATE,
        at(1.0),
        clock
    ));
    h.run(2);
    let was = total_of(&h, me);
    let from = h.bots[b].inbox.len();
    let clock = h.clock();
    combat::hurt_creature(&mut h.server, &mut h.net, other, me, 10_000, clock);
    h.run(10);
    assert_eq!(
        told(&h, b, from),
        vec![
            Told::Text(text::GAINED.0, Some(2160)),
            Told::Total(was + 2160),
            Told::Text(text::KILLED.0, None),
        ]
    );
}
