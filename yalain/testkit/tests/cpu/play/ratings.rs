//! Accuracy and evasion: what an attack's two ratings are made of, and that they decide whether it
//! lands.

use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_testkit::play::server_call;
use yalain_testkit::{Harness, TEST_CREATURE};
use yalain_world::effect_defs::{numeric, EffectDef, Kind, Scale};
use yalain_world::player::quality;
use yalain_world::ratings::{self, Rater};
use yalain_world::rules::Outcome;
use yalain_world::skills::{mask, Rated, RatingPart, SkillDef};
use yalain_world::state::{EffectState, SkillState};

const START_ATTACK: u32 = 0x8204_0008;
/// The human starting sword (a player attacks with a weapon in hand).
const STARTER_SWORD: u32 = 0x4700_051C;
/// An attack made up for the tests, and the skills its parts rate.
const STRIKE: u32 = 5_000;
const ATTACK: u32 = 7;
const DEFENSE: u32 = 8;
const MASTERY: u32 = 10;
const HEROIC: u32 = 5_001;
const RAISES_ATTACK: u32 = 0x6F00_0002;
const RAISES_DEFENSE: u32 = 0x6F00_0003;

fn part(of: Rated, weight: f32) -> RatingPart {
    RatingPart { of, weight }
}

fn raises(state: u32, skill: u32, by: f32) -> EffectDef {
    EffectDef {
        state,
        kind: Kind::Numeric {
            flags: numeric::ADD,
            stats: vec![yalain_world::effects::SKILL_STAT | skill],
            magnitude: Scale(vec![(0.0, by)]),
        },
        duration: Scale::default(),
        priority: Scale::default(),
        fx: 0,
        external_flags: 0,
        internal_flags: 0,
        enum_value: 0,
        eq_class: 0,
        name: None,
        ends_at_logout: false,
    }
}

fn record(eid: u32, state: u32) -> EffectState {
    EffectState {
        eid,
        state,
        cast: 0.0,
        timeout: 1.0e9,
        caster: 0,
        spellcraft: 10.0,
        pk: false,
        acting: 0,
        skill: 0,
        item: 0,
        flags: 0,
        categories: 1,
    }
}

fn trained(skill: u32, granted: u32) -> SkillState {
    SkillState {
        skill,
        granted,
        mask: mask::TRAINED,
        cost: 0,
        children: 0,
        dependents: 0,
        spent: 0,
    }
}

fn creature_of(h: &Harness, b: usize) -> u64 {
    h.world_messages(b, 0)
        .iter()
        .find_map(|m| match m {
            ServerMessage::CreateObject(o) if o.weenie.entity_did == TEST_CREATURE => Some(o.id),
            _ => None,
        })
        .expect("the camp's creature is made")
}

fn scene_drawn(h: &mut Harness, b: usize) {
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
}

/// An attack's accuracy is the parts its skill names, each a weight times its user's rating: a
/// player's level part is 8 a level up to level 50 with its Heroic Mastery attribute, and its
/// ratings count its effects. Its target's evasion is the parts the same skill names of it: a
/// creature's ratings are its template's with its effects, and its level part is nothing.
/// Behaviour: server.combat.ratings-from-skills
#[test]
fn an_attacks_accuracy_and_evasion_are_the_parts_its_skill_names() {
    let mut h = Harness::new();
    h.add_creatures(1);
    let (b, _) = h.player_in_world("one", "Elara");
    scene_drawn(&mut h, b);
    let beetle = creature_of(&h, b);
    let world = *h.server.players.keys().next().expect("in the world");
    h.server.skill_table.skills.insert(
        STRIKE,
        SkillDef {
            id: STRIKE,
            accuracy: vec![
                part(Rated::Level, 1.0),
                part(Rated::Skill(ATTACK), 1.0),
                part(Rated::Skill(MASTERY), 2.4),
            ],
            evasion: vec![
                part(Rated::Level, 1.0),
                part(Rated::Skill(DEFENSE), 1.0),
                part(Rated::Skill(MASTERY), 2.0),
            ],
            ..SkillDef::default()
        },
    );
    h.server.skill_table.skills.insert(
        HEROIC,
        SkillDef {
            id: HEROIC,
            cap: 200,
            category: ratings::HEROIC_MASTERY_CATEGORY,
            attribute_mod: 1.5,
            ..SkillDef::default()
        },
    );
    for d in [
        raises(RAISES_ATTACK, ATTACK, 30.0),
        raises(RAISES_DEFENSE, DEFENSE, 7.0),
    ] {
        h.server.effect_table.effects.insert(d.state, d);
    }
    let set_level = |h: &mut Harness, level: i32| {
        let p = h.server.players.get_mut(&world).expect("the player");
        yalain_world::quals::set_int(&mut p.qualities, yalain_world::examine::LEVEL, level);
    };
    {
        let p = h.server.players.get_mut(&world).expect("the player");
        p.play.skills.push(trained(ATTACK, 20));
        p.play.skills.push(trained(MASTERY, 5));
        p.play.skills.push(trained(DEFENSE, 9));
        let c = h
            .server
            .things
            .get_mut(beetle)
            .and_then(|t| t.creature.as_mut())
            .expect("a creature");
        c.stats.skills = vec![(DEFENSE, 30), (MASTERY, 10), (ATTACK, 4)];
    }
    let of = |h: &Harness, a: Rater, d: Rater| ratings::of(&h.server, STRIKE, a, d);
    let (me, it) = (Rater::Player(world), Rater::Thing(beetle));

    set_level(&mut h, 30);
    let r = of(&h, me, it);
    assert!((r.accuracy - (240.0 + 20.0 + 12.0)).abs() < 1e-3, "{r:?}");
    assert!(
        (r.evasion - (30.0 + 20.0)).abs() < 1e-3,
        "no level part: {r:?}"
    );
    let back = of(&h, it, me);
    assert!((back.accuracy - (4.0 + 24.0)).abs() < 1e-3, "{back:?}");
    assert!(
        (back.evasion - (240.0 + 9.0 + 10.0)).abs() < 1e-3,
        "{back:?}"
    );

    set_level(&mut h, 70);
    let r = of(&h, me, it);
    assert!(
        (r.accuracy - (400.0 + 32.0)).abs() < 1e-3,
        "8 a level to 50: {r:?}"
    );

    // Heroic Mastery adds its attribute to the level part.
    h.server
        .players
        .get_mut(&world)
        .expect("the player")
        .play
        .skills
        .push(trained(HEROIC, 20));
    let r = of(&h, me, it);
    assert!((r.accuracy - (400.0 + 30.0 + 32.0)).abs() < 1e-3, "{r:?}");

    // Effects on the ratings count, on the player and on the creature.
    h.server
        .players
        .get_mut(&world)
        .expect("the player")
        .play
        .effects
        .push(record(1, RAISES_ATTACK));
    h.server
        .creature_effects
        .on
        .insert(beetle, vec![record(2, RAISES_DEFENSE)]);
    let r = of(&h, me, it);
    assert!((r.accuracy - (430.0 + 62.0)).abs() < 1e-3, "{r:?}");
    assert!((r.evasion - (37.0 + 20.0)).abs() < 1e-3, "{r:?}");

    // A skill the server does not know rates both sides at nothing.
    let none = ratings::of(&h.server, STRIKE + 7, me, it);
    assert!(none.accuracy == 0.0 && none.evasion == 0.0, "{none:?}");
}

/// A player's swings and a creature's strikes land by their ratings: far more accuracy than
/// evasion lands them nineteen times in twenty, far less one time in twenty.
/// Behaviour: server.combat.landing-chance
#[test]
fn swings_and_strikes_land_by_accuracy_against_evasion() {
    let shares = |player_ahead: bool| -> (f64, f64) {
        let mut h = Harness::new();
        h.add_creatures(1);
        let (b, me) = h.player_in_world("one", "Elara");
        scene_drawn(&mut h, b);
        h.arm(b, me, STARTER_SWORD);
        let beetle = creature_of(&h, b);
        let world = *h.server.players.keys().next().expect("in the world");
        // Every attack rates its user's attack attribute against its target's defense.
        let mut ids: Vec<u32> = h.server.skill_table.skills.keys().copied().collect();
        ids.extend(yalain_world::combat::MELEE_SKILLS.iter().map(|(k, _)| *k));
        ids.extend(
            yalain_world::missiles::CREATURE_ATTACKS
                .iter()
                .map(|a| a.skill),
        );
        for k in ids {
            let d = h.server.skill_table.skills.entry(k).or_insert(SkillDef {
                id: k,
                ..SkillDef::default()
            });
            d.accuracy = vec![part(Rated::Skill(ATTACK), 1.0)];
            d.evasion = vec![part(Rated::Skill(DEFENSE), 1.0)];
        }
        let (mine, its) = if player_ahead { (1_000, 0) } else { (0, 1_000) };
        let p = h.server.players.get_mut(&world).expect("the player");
        // Neither side dies: the run goes on.
        for q in [quality::HEALTH, quality::HEALTH_MAX] {
            yalain_world::quals::set_int(&mut p.qualities, q, 1_000_000);
        }
        p.play.skills.push(trained(ATTACK, mine));
        p.play.skills.push(trained(DEFENSE, mine));
        let t = h.server.things.get_mut(beetle).expect("the beetle");
        t.max_health = 1_000_000;
        t.health = 1_000_000;
        let c = t.creature.as_mut().expect("a creature");
        c.stats.natural_damage = 0.0;
        c.stats.skills.push((ATTACK, i64::from(its)));
        c.stats.skills.push((DEFENSE, i64::from(its)));
        let from = h.bots[b].inbox.len();
        h.bots[b].send(
            true,
            queue::WEENIE,
            &server_call(START_ATTACK, &[Arg::LongInt(beetle)]),
        );
        h.run(20_000);
        let share = |attacker: u64, target: u64| {
            let results: Vec<i32> = h
                .world_messages(b, from)
                .iter()
                .filter_map(|m| match m {
                    ServerMessage::DoStory(e) if e.action.attacker == attacker => {
                        Some(e.action.hooks.clone())
                    }
                    _ => None,
                })
                .flatten()
                .filter(|k| k.target == target && k.numbers[0] != 0)
                .map(|k| k.numbers[0])
                .collect();
            assert!(results.len() > 400, "{}", results.len());
            let landed = results
                .iter()
                .filter(|r| {
                    **r == Outcome::Hit.code().cast_signed()
                        || **r == Outcome::Critical.code().cast_signed()
                })
                .count();
            #[allow(clippy::cast_precision_loss)]
            let s = landed as f64 / results.len() as f64;
            s
        };
        (share(me, beetle), share(beetle, me))
    };
    let (mine, its) = shares(true);
    assert!(mine > 0.9, "the player's swings: {mine}");
    assert!(its < 0.1, "the creature's strikes: {its}");
    let (mine, its) = shares(false);
    assert!(mine < 0.1, "the player's swings: {mine}");
    assert!(its > 0.9, "the creature's strikes: {its}");
}
