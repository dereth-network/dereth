//! Skills: training spends credits, raising spends experience, each by the client's rules, and a
//! refusal is a text alone.

use std::collections::HashMap;

use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_testkit::play::server_call;
use yalain_testkit::Harness;
use yalain_world::calls::{self, Value};
use yalain_world::skills::{raise_cost, table_level, SkillDef, SkillTable};
use yalain_world::state::SkillState;

const TRAIN: u32 = 0x8214_0002;
const RAISE: u32 = 0x8214_000C;
const UPDATE_INFO: u32 = 0x8214_000D;
const UPDATE_REPOSITORY: u32 = 0x8214_0011;
const SHOW_TEXT: u32 = 0x8205_0011;

/// A made-up skill to train at level 2 for one credit, raised along a table like the masters'.
const SKILL: u32 = 15;
const TABLE: u32 = 0x6F00_000E;

fn table() -> SkillTable {
    let mut skills = HashMap::new();
    skills.insert(
        SKILL,
        SkillDef {
            id: SKILL,
            name: (0xA860_7CEC, 0x2500_085B),
            cost: 1,
            min_level: 2,
            cap: 50,
            table: TABLE,
            flags: 0x1,
            ..SkillDef::default()
        },
    );
    let mut tables = HashMap::new();
    tables.insert(
        TABLE,
        (0..=50u64).map(|n| 2 * n * n.saturating_sub(1)).collect(),
    );
    SkillTable { skills, tables }
}

/// At no vigor a player's rating in a skill that says so falls to the least there is, and what it
/// adds to a skill it is a parent of goes too; a skill that does not say so keeps its own. With any
/// vigor at all each has its whole rating.
/// Behaviour: server.skills.rating-at-no-vigor
#[test]
fn at_no_vigor_a_skill_that_says_so_rates_nothing_and_others_keep_their_rating() {
    use yalain_world::player::quality;
    use yalain_world::quals;
    use yalain_world::skill_use::rating_of_for_test as rating;
    use yalain_world::skills::flag::NOTHING_AT_NO_VIGOR;
    const PENALIZED: u32 = 16;
    const FREE: u32 = 17;
    const CHILD: u32 = 18;
    let mut t = table();
    let def = |id, flags, parents| SkillDef {
        id,
        cap: 200,
        table: TABLE,
        flags,
        parents,
        ..SkillDef::default()
    };
    t.skills
        .insert(PENALIZED, def(PENALIZED, NOTHING_AT_NO_VIGOR, Vec::new()));
    t.skills.insert(FREE, def(FREE, 0, Vec::new()));
    t.skills.insert(CHILD, def(CHILD, 0, vec![(PENALIZED, 0)]));
    let mut h = Harness::new();
    h.server.skill_table = t;
    let (_, me) = h.player_in_world("one", "Elara");
    let world = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .expect("in the world")
        .0;
    let trained = |skill, granted| SkillState {
        skill,
        mask: yalain_world::skills::mask::TRAINED,
        granted,
        cost: 0,
        children: 0,
        dependents: 0,
        spent: 0,
    };
    if let Some(p) = h.server.players.get_mut(&world) {
        p.play.skills = vec![
            trained(PENALIZED, 30),
            trained(FREE, 30),
            trained(CHILD, 20),
        ];
        quals::set_int(&mut p.qualities, quality::VIGOR, 1);
    }
    let of = |h: &Harness, skill| rating(&h.server, world, skill);
    assert_eq!(of(&h, PENALIZED), 31.0, "granted, and the table's first");
    assert_eq!(of(&h, CHILD), 24.0, "with a tenth of its parent");
    if let Some(p) = h.server.players.get_mut(&world) {
        quals::set_int(&mut p.qualities, quality::VIGOR, 0);
    }
    assert_eq!(of(&h, PENALIZED), 1.0);
    assert_eq!(of(&h, FREE), 31.0);
    assert_eq!(of(&h, CHILD), 21.0, "with none");
}

fn calls_of(h: &Harness, b: usize, from: usize) -> Vec<(u32, Vec<Value>)> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c) => Some((
                c.function.0,
                c.args
                    .as_ref()
                    .and_then(|a| calls::values(&a.0).ok())
                    .unwrap_or_default(),
            )),
            _ => None,
        })
        .collect()
}

/// A level-2 player with a credit and 300 experience to spend.
fn trainee(h: &mut Harness) -> usize {
    h.server.skill_table = table();
    let (b, _) = h.player_in_world("one", "Elara");
    for p in h.server.players.values_mut() {
        p.play.credits = 1;
        yalain_world::quals::set_int(&mut p.qualities, 302, 2);
        yalain_world::quals::set_long(&mut p.qualities, 301, 300);
    }
    b
}

fn ask(h: &mut Harness, b: usize, f: u32) -> usize {
    let from = h.bots[b].inbox.len();
    h.bots[b].send(true, queue::WEENIE, &server_call(f, &[Arg::Int(SKILL)]));
    h.run(10);
    from
}

/// Training a skill a player may train: the repository with the credit spent, the skill's
/// information, then its text; training it again is refused with a text alone.
/// Behaviour: server.skills.train
#[test]
fn training_a_skill_spends_its_credits_and_again_is_refused() {
    let mut h = Harness::new();
    let b = trainee(&mut h);
    let from = ask(&mut h, b, TRAIN);
    let fs: Vec<u32> = calls_of(&h, b, from).iter().map(|(f, _)| *f).collect();
    assert_eq!(fs, vec![UPDATE_REPOSITORY, UPDATE_INFO, SHOW_TEXT]);
    let p = h.server.players.values().next().expect("the player");
    assert_eq!(p.play.credits, 0);
    assert!(p
        .play
        .skills
        .iter()
        .any(|s| s.skill == SKILL && s.mask == 0x3 && s.cost == 1));
    let from = ask(&mut h, b, TRAIN);
    let fs: Vec<u32> = calls_of(&h, b, from).iter().map(|(f, _)| *f).collect();
    assert_eq!(fs, vec![SHOW_TEXT], "already trained: a text alone");
}

/// Raising a trained skill costs the experience to its table's next level (4 from none on the
/// masters' table, then 8): the skill's information, the experience left, its text; with too
/// little experience left, a text alone.
/// Behaviour: server.skills.raise
#[test]
fn raising_a_skill_spends_the_experience_to_its_tables_next_level() {
    let mut h = Harness::new();
    let b = trainee(&mut h);
    ask(&mut h, b, TRAIN);
    let from = ask(&mut h, b, RAISE);
    let fs: Vec<u32> = calls_of(&h, b, from).iter().map(|(f, _)| *f).collect();
    assert_eq!(fs, vec![UPDATE_INFO, SHOW_TEXT]);
    let p = h.server.players.values().next().expect("the player");
    assert_eq!(yalain_world::quals::long(&p.qualities, 301), Some(296));
    ask(&mut h, b, RAISE);
    let p = h.server.players.values().next().expect("the player");
    assert_eq!(yalain_world::quals::long(&p.qualities, 301), Some(288));
    for p in h.server.players.values_mut() {
        yalain_world::quals::set_long(&mut p.qualities, 301, 5);
    }
    let from = ask(&mut h, b, RAISE);
    let fs: Vec<u32> = calls_of(&h, b, from).iter().map(|(f, _)| *f).collect();
    assert_eq!(fs, vec![SHOW_TEXT], "short of experience: a text alone");
}

/// An active skill stops at 50 for a player who is no hero, whatever its experience and its parents
/// come to; a hero's goes on to the skill's own cap, in its rating and in a raise. A passive skill
/// has its own cap for anyone.
/// Behaviour: server.skills.active-cap-for-who-is-no-hero
#[test]
fn an_active_skill_stops_at_fifty_for_who_is_no_hero_and_goes_on_for_a_hero() {
    use yalain_world::skills::{refusal, Held};
    const PARENT: u32 = 16;
    let mut t = table();
    t.tables.insert(
        TABLE,
        (0..=100u64).map(|n| 2 * n * n.saturating_sub(1)).collect(),
    );
    let at = |n: u64| 2 * n * n.saturating_sub(1);
    let def = |id: u32, active: bool, parents: Vec<(u32, u32)>| SkillDef {
        id,
        cap: 150,
        table: TABLE,
        active,
        parents,
        ..SkillDef::default()
    };
    t.skills.insert(SKILL, def(SKILL, true, vec![(PARENT, 0)]));
    t.skills.insert(PARENT, def(PARENT, false, vec![]));
    let held = |skill: u32, xp: u64| Held {
        skill,
        granted: 0,
        xp,
        mask: 0x3,
        cost: 0,
        children: 0,
        dependents: 0,
    };
    // A parent of 50 gives 5: 62 of its own comes to 67.
    let skills = [held(SKILL, at(62)), held(PARENT, at(50))];
    assert_eq!(t.rating(&skills, SKILL, true), 67, "a hero's");
    assert_eq!(t.rating(&skills, SKILL, false), 50, "no hero's");
    assert_eq!(t.rating(&skills, PARENT, false), 50);
    let passive = [held(PARENT, at(62))];
    assert_eq!(t.rating(&passive, PARENT, false), 62, "passive");
    let at_fifty = [held(SKILL, at(50))];
    assert_eq!(
        t.raise(&at_fifty, SKILL, 1_000_000, false),
        Err(refusal::CAP)
    );
    assert_eq!(
        t.raise(&at_fifty, SKILL, 1_000_000, true),
        Ok(at(51) - at(50)),
        "a hero raises on"
    );

    // Asked of the server, by a hero and by a player who is none.
    let raises = |hero: bool| {
        let mut h = Harness::new();
        let b = trainee(&mut h);
        h.server.skill_table = t.clone();
        h.server
            .skill_table
            .skills
            .get_mut(&SKILL)
            .unwrap()
            .parents
            .clear();
        ask(&mut h, b, TRAIN);
        for p in h.server.players.values_mut() {
            yalain_world::quals::set_bool(&mut p.qualities, yalain_world::focus::HERO, hero);
            yalain_world::quals::set_long(&mut p.qualities, 301, 1_000_000);
            let s = p.play.skills.iter_mut().find(|s| s.skill == SKILL).unwrap();
            s.spent = at(50);
        }
        let from = ask(&mut h, b, RAISE);
        calls_of(&h, b, from)
            .iter()
            .map(|(f, _)| *f)
            .collect::<Vec<u32>>()
    };
    assert_eq!(raises(true), vec![UPDATE_INFO, SHOW_TEXT], "a hero's raise");
    assert_eq!(raises(false), vec![SHOW_TEXT], "refused at 50");
}

/// A skill's level in its table: the highest level whose experience it holds (none still reaches
/// level 1); a raise costs the experience to the next, none at the top.
/// A rating with effects: a parent's counts, as changed, toward its child; the cap still holds.
/// Behaviour: server.skills.rating-with-effects
#[test]
fn a_skills_rating_counts_its_own_and_its_parents_effects_up_to_its_cap() {
    use yalain_world::skills::{mask, Held};
    const PARENT: u32 = 16;
    let mut t = table();
    t.skills.insert(
        PARENT,
        SkillDef {
            id: PARENT,
            cap: 200,
            table: TABLE,
            ..SkillDef::default()
        },
    );
    t.skills.get_mut(&SKILL).expect("the skill").parents = vec![(PARENT, 1)];
    let held = |skill, granted| Held {
        skill,
        granted,
        xp: 0,
        mask: mask::TRAINED,
        cost: 0,
        children: 0,
        dependents: 0,
    };
    let skills = [held(SKILL, 20), held(PARENT, 10)];
    assert_eq!(t.rating(&skills, SKILL, false), 20 + 1 + 1);
    // The parent's raised rating counts as a tenth, at most its 50.
    let parent_raised = |sk: u32, r: u32| if sk == PARENT { r + 19 } else { r };
    assert_eq!(
        t.rating_with(&skills, SKILL, false, &parent_raised),
        20 + 3 + 1
    );
    let own_raised = |sk: u32, r: u32| if sk == SKILL { r + 6 } else { r };
    assert_eq!(t.rating_with(&skills, SKILL, false, &own_raised), 22 + 6);
    let far = |sk: u32, r: u32| if sk == SKILL { r + 600 } else { r };
    assert_eq!(t.rating_with(&skills, SKILL, false, &far), 50, "its cap");
    // No effect, as without.
    assert_eq!(
        t.rating_with(&skills, SKILL, false, &|_, r| r),
        t.rating(&skills, SKILL, false)
    );
}

/// Behaviour: server.skills.advancement-table
#[test]
fn a_tables_level_and_raise_cost_follow_the_library() {
    let t: Vec<u64> = vec![0, 0, 4, 12, 24];
    assert_eq!(table_level(&t, 0), 1);
    assert_eq!(table_level(&t, 4), 2);
    assert_eq!(table_level(&t, 11), 2);
    assert_eq!(raise_cost(&t, 0), 4);
    assert_eq!(raise_cost(&t, 4), 8);
    assert_eq!(raise_cost(&t, 24), 0);
}

/// A hero skill (Arcane Lore's kind) is trained only by a hero and paid in hero skill credits: a
/// player who is no hero is refused (30047); a hero short of its hero cost is refused (30006); a
/// hero with it pays the hero cost from its hero skill credits, its skill credits untouched, the
/// skill kept with that cost, and the repository sent with the hero skill credits left.
/// Behaviour: server.skills.hero-skills-paid-in-hero-credits
#[test]
fn a_hero_skill_is_trained_only_by_a_hero_and_paid_in_hero_skill_credits() {
    use yalain_world::skills::{flag, refusal, repository_update, Credits};
    const HERO_SKILL: u32 = 1126;
    let mut h = Harness::new();
    let b = trainee(&mut h);
    h.server.skill_table.skills.insert(
        HERO_SKILL,
        SkillDef {
            id: HERO_SKILL,
            name: (0xA860_7CEC, 0x2500_0DD4),
            hero_cost: 75,
            cap: 1000,
            flags: flag::HERO | flag::MAY_UNTRAIN,
            ..SkillDef::default()
        },
    );
    let refusal_of = |h: &Harness, hero: Option<u32>| {
        h.server
            .skill_table
            .train_refusal(&[], HERO_SKILL, 2, 1, Credits { skill: 1000, hero })
    };
    assert_eq!(refusal_of(&h, None), Some(refusal::HERO), "no hero");
    assert_eq!(refusal_of(&h, Some(74)), Some(refusal::CREDITS), "short");
    assert_eq!(refusal_of(&h, Some(75)), None, "a hero with its cost");
    let train = |h: &mut Harness| {
        let from = h.bots[b].inbox.len();
        h.bots[b].send(
            true,
            queue::WEENIE,
            &server_call(TRAIN, &[Arg::Int(HERO_SKILL)]),
        );
        h.run(10);
        from
    };
    for p in h.server.players.values_mut() {
        p.play.hero_credits = 80;
    }
    let from = train(&mut h);
    let fs: Vec<u32> = calls_of(&h, b, from).iter().map(|(f, _)| *f).collect();
    assert_eq!(fs, vec![SHOW_TEXT], "no hero: a text alone");
    for p in h.server.players.values_mut() {
        yalain_world::focus::set_hero_kept(&mut p.qualities, true);
        p.play.hero_credits = 74;
    }
    let from = train(&mut h);
    let fs: Vec<u32> = calls_of(&h, b, from).iter().map(|(f, _)| *f).collect();
    assert_eq!(fs, vec![SHOW_TEXT], "short of hero credits: a text alone");
    for p in h.server.players.values_mut() {
        p.play.hero_credits = 80;
    }
    let from = train(&mut h);
    let fs: Vec<u32> = calls_of(&h, b, from).iter().map(|(f, _)| *f).collect();
    assert_eq!(fs, vec![UPDATE_REPOSITORY, UPDATE_INFO, SHOW_TEXT]);
    assert!(
        h.world_messages(b, from)
            .iter()
            .any(|m| *m == repository_update(1, 5)),
        "the repository with the hero credits left"
    );
    let repo = calls_of(&h, b, from)
        .into_iter()
        .find(|(f, _)| *f == UPDATE_REPOSITORY)
        .expect("the repository");
    let Some(Value::Graph(g)) = repo.1.first() else {
        panic!("a repository graph");
    };
    let slots = g
        .iter()
        .find_map(|(_, n)| match n {
            calls::Node::Script {
                package: 0x233,
                slots,
            } => Some(slots.clone()),
            _ => None,
        })
        .expect("the repository object");
    let words: Vec<u32> = slots.iter().map(|(_, v)| *v).collect();
    assert_eq!(
        words[..4],
        [1, 0, 0, 5],
        "credits, the untraining's experience, hero credits"
    );
    let p = h.server.players.values().next().expect("the player");
    assert_eq!((p.play.credits, p.play.hero_credits), (1, 5));
    assert!(p
        .play
        .skills
        .iter()
        .any(|s| s.skill == HERO_SKILL && s.mask == 0x3 && s.cost == 75));
}

/// Reaching a level that brings hero skill credits (from level 45, 100 a level in the client's
/// table) adds them with the level's skill credit, the repository sent with both.
/// Behaviour: server.skills.hero-credits-by-level
#[test]
fn a_level_from_45_brings_its_hero_skill_credits() {
    use yalain_world::levels::{Level, LevelTable};
    use yalain_world::skills::repository_update;
    let mut h = Harness::new();
    let b = trainee(&mut h);
    h.server.levels = LevelTable {
        levels: (1..=46)
            .map(|l: u64| Level {
                xp: l * 10,
                health: 100,
                vigor: 100,
                skill_credits: 1,
                hero_credits: if l >= 45 { 100 } else { 0 },
            })
            .collect(),
    };
    let world = *h.server.players.keys().next().expect("in the world");
    h.server.players.get_mut(&world).unwrap().play.hero_credits = 3;
    let from = h.bots[b].inbox.len();
    yalain_world::progress::level_up(&mut h.server, &mut h.net, world, 43, 44);
    h.run(3);
    assert_eq!(h.server.players[&world].play.hero_credits, 3, "none at 44");
    yalain_world::progress::level_up(&mut h.server, &mut h.net, world, 44, 46);
    h.run(3);
    let p = &h.server.players[&world];
    assert_eq!((p.play.credits, p.play.hero_credits), (4, 203));
    assert!(
        h.world_messages(b, from)
            .iter()
            .any(|m| *m == repository_update(4, 203)),
        "the repository with both"
    );
}
