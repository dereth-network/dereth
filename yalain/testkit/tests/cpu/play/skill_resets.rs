//! A complete reset of a player's skills or hero skills: granted by an effect, used by `/skills
//! reset` or `/heroskills reset`, which untrains every skill of the kind the player bought and
//! gives back its credits and every point of experience put into the kind's skills; the hour after
//! a reset in which the player may reset again without another. Fixture: a new player holding
//! made-up skills: a bought skill with experience in it and a bought child of it, a hero skill, and
//! a starting skill with experience in it.

use std::collections::HashMap;

use auberean_script::args::Arg;
use yalain_testkit::Harness;
use yalain_world::effect_defs::{EffectDef, Kind, Reset, Scale};
use yalain_world::progress::q::AVAILABLE_XP;
use yalain_world::skills::{function, no_reset, reset, untraining, SkillDef, SkillTable};
use yalain_world::state::SkillState;
use yalain_world::{effects, quals};

/// A skill bought for two credits, a child of it bought for one, a hero skill bought for three
/// hero skill credits, and a starting skill.
const BOUGHT: u32 = 15;
const CHILD: u32 = 16;
const HERO: u32 = 17;
const STARTING: u32 = 18;
const XP_BOUGHT: u64 = 50;
const XP_CHILD: u64 = 10;
const XP_HERO: u64 = 20;
const XP_STARTING: u64 = 30;
const AVAILABLE: u64 = 301;

fn skill(id: u32, flags: u32) -> SkillDef {
    SkillDef {
        id,
        name: (0xA860_7CEC, 0x2500_085B),
        flags,
        ..SkillDef::default()
    }
}

/// The player holding the four skills, with one skill credit and none of hero skills, and `held`
/// resets of skills and `hero` of hero skills: its client and connection.
fn holding(held: i32, hero: i32) -> (Harness, usize, yalain_net::ConnId) {
    let mut h = Harness::new();
    let mut skills = HashMap::new();
    skills.insert(
        BOUGHT,
        SkillDef {
            cost: 2,
            ..skill(BOUGHT, 0x1)
        },
    );
    skills.insert(
        CHILD,
        SkillDef {
            cost: 1,
            parents: vec![(BOUGHT, 0)],
            ..skill(CHILD, 0x1)
        },
    );
    skills.insert(
        HERO,
        SkillDef {
            hero_cost: 3,
            ..skill(HERO, 0x11)
        },
    );
    skills.insert(STARTING, skill(STARTING, 0x8));
    h.server.skill_table = SkillTable {
        skills,
        tables: HashMap::new(),
        ..SkillTable::default()
    };
    h.all_hold(STARTING);
    let (b, _) = h.player_in_world("one", "Elara");
    let world = *h.server.players.keys().next().expect("in");
    let p = h.server.players.get_mut(&world).expect("in");
    p.play.credits = 1;
    p.play.hero_credits = 0;
    let state = |skill, cost, children, spent| SkillState {
        skill,
        granted: 0,
        mask: if skill == STARTING { 0x1 } else { 0x3 },
        cost,
        children,
        dependents: 0,
        spent,
    };
    p.play.skills = vec![
        state(BOUGHT, 2, 1, XP_BOUGHT),
        state(CHILD, 1, 0, XP_CHILD),
        state(HERO, 3, 0, XP_HERO),
        state(STARTING, 0, 0, XP_STARTING),
    ];
    quals::set_long(&mut p.qualities, AVAILABLE_XP, AVAILABLE);
    quals::set_int(&mut p.qualities, reset::SKILLS_HELD, held);
    quals::set_int(&mut p.qualities, reset::HERO_SKILLS_HELD, hero);
    h.run(2);
    (h, b, world)
}

fn ask(h: &mut Harness, b: usize, f: u32, args: &[Arg]) -> usize {
    let from = h.bots[b].inbox.len();
    h.ask(b, f, args, 5);
    from
}

/// The skills the player holds, with the experience in each, in order.
fn skills(h: &Harness, world: yalain_net::ConnId) -> Vec<(u32, u64)> {
    let p = &h.server.players[&world];
    let mut v: Vec<(u32, u64)> = [BOUGHT, CHILD, HERO, STARTING]
        .into_iter()
        .filter_map(|s| match p.play.skills.iter().find(|x| x.skill == s) {
            Some(x) => Some((s, x.spent)),
            // A starting skill as its race started it is kept by its absence.
            None if s == STARTING => Some((s, 0)),
            None => None,
        })
        .collect();
    v.sort_unstable();
    v
}

fn int(h: &Harness, world: yalain_net::ConnId, q: u32) -> Option<i32> {
    quals::int(&h.server.players[&world].qualities, q)
}

fn available(h: &Harness, world: yalain_net::ConnId) -> u64 {
    quals::long(&h.server.players[&world].qualities, AVAILABLE_XP).unwrap_or(0)
}

/// A player holding a reset of its skills uses it: every skill it bought is untrained (the child
/// with its parent) and its credits come back; the experience in those and in its starting skill
/// comes back at once to spend, the starting skill kept at the level it was given at; its hero
/// skill is untouched. The reset is used up, the hour of resets begins, and the player is told so.
/// Behaviour: server.skills.a-held-reset-untrains-every-bought-skill
#[test]
fn a_held_reset_untrains_every_bought_skill_and_gives_back_its_credits_and_all_the_experience() {
    let (mut h, b, world) = holding(1, 0);
    let now = h.server.game_now;
    let from = ask(&mut h, b, function::SELL_BACK_ALL, &[]);
    assert_eq!(h.texts(b, from), [(reset::SHOWN_AS, reset::SKILLS_DONE.0)]);
    assert_eq!(skills(&h, world), [(HERO, XP_HERO), (STARTING, 0)]);
    let p = &h.server.players[&world];
    assert_eq!(p.play.credits, 1 + 2 + 1, "both bought skills' credits");
    assert_eq!(p.play.hero_credits, 0);
    assert_eq!(
        available(&h, world),
        AVAILABLE + XP_BOUGHT + XP_CHILD + XP_STARTING
    );
    assert_eq!(int(&h, world, reset::SKILLS_HELD), Some(0), "used up");
    let began = quals::timestamp(&p.qualities, reset::SKILLS_LAST);
    assert!(began.is_some_and(|t| t >= now), "{began:?}");
    let gone: Vec<u32> = h
        .calls(b, from)
        .into_iter()
        .filter(|(f, _)| *f == function::REMOVE_INFO)
        .filter_map(|(_, v)| match v.first() {
            Some(yalain_world::calls::Value::Int(s)) => Some(*s),
            _ => None,
        })
        .collect();
    assert_eq!(gone.len(), 2, "{gone:?}");
    assert!(gone.contains(&BOUGHT) && gone.contains(&CHILD));
}

/// An untraining under way of a bought skill ends with the reset: its experience comes back with
/// the reset's, once.
/// Behaviour: server.skills.a-held-reset-untrains-every-bought-skill
#[test]
fn an_untraining_under_way_ends_with_the_reset_its_experience_given_back_once() {
    let (mut h, b, world) = holding(1, 0);
    let from = ask(&mut h, b, function::UNTRAIN, &[Arg::Int(CHILD)]);
    assert_eq!(
        h.texts(b, from),
        [(untraining::SHOWN_AS, untraining::UNDER_WAY)]
    );
    assert!(h.server.players[&world].untraining.is_some());
    ask(&mut h, b, function::SELL_BACK_ALL, &[]);
    assert!(h.server.players[&world].untraining.is_none());
    assert_eq!(
        available(&h, world),
        AVAILABLE + XP_BOUGHT + XP_CHILD + XP_STARTING
    );
}

/// For an hour after a reset is used the player may reset again as often as it likes without
/// another; after the hour, holding none, it is refused as a player granted none is.
/// Behaviour: server.skills.a-reset-may-be-used-again-for-an-hour
#[test]
fn for_an_hour_after_a_reset_another_needs_none_and_after_it_one_is_needed_again() {
    let (mut h, b, world) = holding(1, 0);
    ask(&mut h, b, function::SELL_BACK_ALL, &[]);
    let began =
        quals::timestamp(&h.server.players[&world].qualities, reset::SKILLS_LAST).expect("begun");
    let from = ask(&mut h, b, function::SELL_BACK_ALL, &[]);
    assert_eq!(
        h.texts(b, from),
        [(reset::SHOWN_AS, reset::SKILLS_DONE.0)],
        "again within the hour"
    );
    assert_eq!(int(&h, world, reset::SKILLS_HELD), Some(0));
    assert_eq!(
        quals::timestamp(&h.server.players[&world].qualities, reset::SKILLS_LAST),
        Some(began),
        "the hour runs from the reset used, not from each one after it"
    );
    let p = h.server.players.get_mut(&world).expect("in");
    let past = h.server.game_now - reset::AGAIN_FOR - 1.0;
    quals::set_timestamp(&mut p.qualities, reset::SKILLS_LAST, past);
    let from = ask(&mut h, b, function::SELL_BACK_ALL, &[]);
    assert_eq!(
        h.texts(b, from),
        [(untraining::SHOWN_AS, no_reset::SKILLS.0)],
        "the hour over"
    );
}

/// A reset of hero skills untrains the hero skills alone, their hero skill credits and experience
/// back; a reset of skills held is no reset of hero skills, nor the other way about.
/// Behaviour: server.skills.a-hero-reset-is-its-own
#[test]
fn a_hero_skills_reset_untrains_the_hero_skills_alone_and_is_held_apart() {
    let (mut h, b, world) = holding(1, 0);
    let from = ask(&mut h, b, function::SELL_BACK_ALL_HERO, &[]);
    assert_eq!(
        h.texts(b, from),
        [(untraining::SHOWN_AS, no_reset::HERO_SKILLS.0)],
        "a reset of skills is not one of hero skills"
    );
    assert_eq!(int(&h, world, reset::SKILLS_HELD), Some(1));

    let (mut h, b, world) = holding(0, 1);
    let from = ask(&mut h, b, function::SELL_BACK_ALL, &[]);
    assert_eq!(
        h.texts(b, from),
        [(untraining::SHOWN_AS, no_reset::SKILLS.0)]
    );
    let from = ask(&mut h, b, function::SELL_BACK_ALL_HERO, &[]);
    assert_eq!(
        h.texts(b, from),
        [(reset::SHOWN_AS, reset::HERO_SKILLS_DONE.0)]
    );
    assert_eq!(
        skills(&h, world),
        [
            (BOUGHT, XP_BOUGHT),
            (CHILD, XP_CHILD),
            (STARTING, XP_STARTING)
        ]
    );
    let p = &h.server.players[&world];
    assert_eq!((p.play.credits, p.play.hero_credits), (1, 3));
    assert_eq!(available(&h, world), AVAILABLE + XP_HERO);
    assert_eq!(int(&h, world, reset::HERO_SKILLS_HELD), Some(0));
    assert!(quals::timestamp(&p.qualities, reset::HERO_SKILLS_LAST).is_some());
    assert!(quals::timestamp(&p.qualities, reset::SKILLS_LAST).is_none());
}

/// An effect that grants a reset gives its player one more of its kind and tells it so, by the
/// command that uses it; a player granted one may then reset.
/// Behaviour: server.skills.an-effect-grants-a-reset
#[test]
fn an_effect_granting_a_reset_gives_one_more_and_tells_the_player() {
    const GRANT: u32 = 0x6F00_7E51;
    const GRANT_HERO: u32 = 0x6F00_7E52;
    let (mut h, b, world) = holding(0, 0);
    for (state, kind) in [(GRANT, Reset::Skills), (GRANT_HERO, Reset::HeroSkills)] {
        h.server.effect_table.effects.insert(
            state,
            EffectDef {
                state,
                kind: Kind::Reset(kind),
                duration: Scale::default(),
                priority: Scale::default(),
                fx: 0,
                external_flags: 0,
                internal_flags: 0,
                ends_at_logout: false,
                enum_value: 0,
                eq_class: 0,
                name: None,
            },
        );
    }
    let records = h.server.players[&world].play.effects.len();
    let from = h.bots[b].inbox.len();
    let clock = h.clock();
    for state in [GRANT, GRANT, GRANT_HERO] {
        effects::apply(
            &mut h.server,
            &mut h.net,
            world,
            state,
            effects::Cast::default(),
            clock,
        );
    }
    h.run(2);
    assert_eq!(
        h.texts(b, from),
        [
            (reset::SHOWN_AS, reset::SKILLS_GRANTED.0),
            (reset::SHOWN_AS, reset::SKILLS_GRANTED.0),
            (reset::SHOWN_AS, reset::HERO_SKILLS_GRANTED.0),
        ]
    );
    assert_eq!(int(&h, world, reset::SKILLS_HELD), Some(2));
    assert_eq!(int(&h, world, reset::HERO_SKILLS_HELD), Some(1));
    assert_eq!(
        h.server.players[&world].play.effects.len(),
        records,
        "no record kept"
    );
    let from = ask(&mut h, b, function::SELL_BACK_ALL, &[]);
    assert_eq!(h.texts(b, from), [(reset::SHOWN_AS, reset::SKILLS_DONE.0)]);
    assert_eq!(int(&h, world, reset::SKILLS_HELD), Some(1));
    // "You have been granted the ability to perform a complete skills reset. ..."
    assert_eq!(reset::SKILLS_GRANTED, (0x10CB_9CA5, 0x2500_04AA));
    // "You have now untrained all of your skills and regained all of your experience points ..."
    assert_eq!(reset::SKILLS_DONE, (0x32EE_F696, 0x2500_04AA));
}

/// A bought perk that gives the last bag stays through a reset while that bag is full, as
/// untraining it alone would, and the skill it stands on stays with it; with a free place there,
/// both go with the rest.
/// Behaviour: server.skills.a-held-reset-untrains-every-bought-skill
#[test]
fn the_bags_perk_stays_through_a_reset_while_the_last_bag_is_full() {
    use yalain_world::inventory::{segment_start, SEGMENTS};
    use yalain_world::state::Held;
    for full in [true, false] {
        let (mut h, b, world) = holding(1, 0);
        h.server
            .skill_table
            .skills
            .get_mut(&CHILD)
            .expect("defined")
            .perk = Some(yalain_world::skills::Perk {
            kind: untraining::BAGS_PERK,
            ..yalain_world::skills::Perk::default()
        });
        let last = SEGMENTS.len() - 1;
        let start = segment_start(last);
        let places = SEGMENTS[last] - u32::from(!full);
        h.server.players.get_mut(&world).expect("in").play.items = (0..places)
            .map(|i| Held {
                id: u64::from(i) + 1,
                template: 0x4700_0001,
                slot: start + i,
                location: 0,
                quantity: 1,
            })
            .collect();
        ask(&mut h, b, function::SELL_BACK_ALL, &[]);
        let held = skills(&h, world);
        let kept = |skill| held.iter().any(|(s, _)| *s == skill);
        assert_eq!((kept(CHILD), kept(BOUGHT)), (full, full), "full: {full}");
        if full {
            assert!(held.contains(&(BOUGHT, XP_BOUGHT)), "as it was: {held:?}");
        }
    }
}

/// A reset of skills leaves a bought skill a hero skill stands on as it was, as untraining it
/// alone would.
/// Behaviour: server.skills.a-hero-reset-is-its-own
#[test]
fn a_skills_reset_leaves_what_a_hero_skill_stands_on() {
    let (mut h, b, world) = holding(1, 0);
    h.server
        .skill_table
        .skills
        .get_mut(&HERO)
        .expect("defined")
        .prereqs = vec![(CHILD, 0)];
    ask(&mut h, b, function::SELL_BACK_ALL, &[]);
    assert_eq!(
        skills(&h, world),
        [
            (BOUGHT, XP_BOUGHT),
            (CHILD, XP_CHILD),
            (HERO, XP_HERO),
            (STARTING, 0)
        ]
    );
    assert_eq!(available(&h, world), AVAILABLE + XP_STARTING);
}
