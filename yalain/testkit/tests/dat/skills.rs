//! The skills' definitions as the server reads them from the September 2013 data: how each used
//! skill is used.

use yalain_testkit::authored::Portal;
use yalain_world::missiles::attack_skill;
use yalain_world::rules::Kind;

/// The starting skills of a human read with their reach, waits, vigor, implements and hooks as the
/// data holds them: Deadeye shoots a missile from 2 to 80 meters with a bow, every 2 seconds, for
/// 32 vigor; Mind Lash and Riposte strike within a meter; Lifestone Recall powers up for 10
/// seconds and waits 30 to be used again.
/// Behaviour: server.skills.used-skills-read-from-their-definitions
#[test]
fn the_starting_skills_read_with_how_they_are_used() {
    let t = Portal::open().skills();
    let used = t.skills.values().filter(|d| d.usage.is_some()).count();
    assert!(used > 1_000, "most skills are used ones: {used}");

    let deadeye = t.skills[&207].usage.clone().expect("Deadeye is used");
    assert_eq!((deadeye.min_range, deadeye.max_range), (2.0, 80.0));
    assert_eq!(deadeye.recovery, 2.0);
    assert_eq!(deadeye.vigor_add, -32.0);
    assert_eq!(deadeye.implements_right, 4, "a bow");
    let shot = deadeye
        .hooks
        .iter()
        .find(|h| h.missile != 0)
        .expect("its missile");
    assert_eq!((shot.missile, shot.missile_speed), (0x4700_00BE, 35.0));
    assert_eq!((shot.add, shot.mult), (5.0, 1.0));
    assert_eq!(deadeye.target_effects, vec![0x7000_0725]);

    let lash = t.skills[&96].usage.clone().expect("Mind Lash is used");
    assert_eq!((lash.max_range, lash.recovery), (1.0, 5.0));
    let riposte = t.skills[&176].usage.clone().expect("Riposte is used");
    assert_eq!((riposte.max_range, riposte.recovery), (1.0, 1.0));

    let recall = t.skills[&835].usage.clone().expect("the recall is used");
    assert_eq!((recall.powerup, recall.reset), (10.0, 30.0));
    assert_eq!(recall.valid_targets, 1, "only its user");
}

/// A creature's attacks are its skills' definitions: `Strong` strikes with its own behavior and
/// adds 3 damage at rating 1 to 75 at 1,000 (`Stronger` 7), and its use bars it for a while; the
/// harming missile throws a missile, the plain bolt of magic is a magic attack that throws one; a
/// heal a creature uses on itself is no attack, nor is a rating.
/// Behaviour: server.creatures.attacks-from-skill-definitions
#[test]
fn creatures_attacks_read_from_their_skills_definitions() {
    let t = Portal::open().skills();
    let strong = attack_skill(&t, 1846, 1).expect("Strong is an attack");
    assert_eq!(strong.behavior, 0x4100_0077);
    assert_eq!(strong.kind, Kind::Melee);
    assert!(!strong.missile);
    assert!((strong.added - 3.0).abs() < 1e-3, "{}", strong.added);
    assert!(strong.barred_after);
    let top = attack_skill(&t, 1846, 1000).expect("Strong").added;
    assert!((top - 75.0).abs() < 1e-3, "{top}");
    let stronger = attack_skill(&t, 1847, 1).expect("Stronger").added;
    assert!((stronger - 7.0).abs() < 1e-3, "{stronger}");
    let harm = attack_skill(&t, 1860, 5).expect("the harming missile");
    assert!(harm.missile);
    assert_eq!(harm.kind, Kind::Missile);
    let bolt = attack_skill(&t, 1840, 1).expect("the plain bolt");
    assert!(bolt.missile);
    assert_eq!(bolt.kind, Kind::Magic);
    assert!(attack_skill(&t, 1818, 50).is_none(), "a heal on itself");
    assert!(attack_skill(&t, 2, 0).is_none(), "a rating");
}

/// The two summoning skills are used as the recall is, at half the user's vigor: Summon Lifestone
/// and Summon Portal power up for 10 seconds, only on their user, and name the effects that open
/// their portals; the portal's template is one that is not walked into.
/// Behaviour: server.skills.summoning-skills-read
#[test]
fn the_summoning_skills_read_as_recalls_at_half_vigor() {
    let p = Portal::open();
    let t = p.skills();
    for (skill, effect) in [(871, 0x7000_0728), (872, 0x7000_0729)] {
        let u = t.skills[&skill].usage.clone().expect("a summoning is used");
        assert_eq!(u.powerup, 10.0, "{skill}");
        assert_eq!(u.powerup_behavior, 0x4100_00BF, "{skill}");
        assert_eq!(u.vigor_mult, 0.5, "{skill}");
        assert_eq!(u.valid_targets, 1, "{skill}: only its user");
        assert_eq!(u.target_effects, vec![effect], "{skill}");
    }
    let portal = p.template(yalain_world::usable::portals::TEMPLATE);
    assert_eq!(portal.description.package, 0xF12);
}

/// Portal Recall, the cities' recalls and Allegiance Recall are used as the lifestone's recall is
/// (a 10 s power-up, only on their user, the whole of its vigor, the wait every skill shares of
/// 30 s): Portal Recall names its recall as the effect on its target, the others put theirs on
/// their user, the cities' with their protection.
/// Behaviour: server.skills.recall-skills-read
#[test]
fn the_recall_skills_read_as_the_lifestones() {
    let p = Portal::open();
    let t = p.skills();
    for (skill, target, user) in [
        (870, vec![0x7000_0727], vec![]),
        (925, vec![], vec![0x7000_1FC0, 0x7000_2014]),
        (926, vec![], vec![0x7000_1FC1, 0x7000_2014]),
        (927, vec![], vec![0x7000_1FC2, 0x7000_2014]),
        (1045, vec![], vec![0x6F00_0222, 0x7000_2014]),
        (999, vec![], vec![0x7000_1EC5]),
    ] {
        let u = t.skills[&skill].usage.clone().expect("a recall is used");
        assert_eq!(
            (
                u.powerup,
                u.powerup_behavior,
                u.vigor_mult,
                u.valid_targets,
                u.reset
            ),
            (10.0, 0x4100_00BF, 1.0, 1, 30.0),
            "{skill}"
        );
        assert_eq!(
            (u.target_effects, u.user_effects),
            (target, user),
            "{skill}"
        );
    }
}

/// What a landed special's effect takes from a creature's armor counts in the next hit, on the whole
/// server with the install's own skills: Rend Armor at rating 50 takes 150, and Expose at rating 40
/// then does the weapon's roll plus its 60, less the 173 of armor and with the 150 back, within 269
/// to 347 for a weapon of 232 to 310; 119 to 197 with no Rend Armor before it (2017, that weapon and
/// armor with one such effect held: a median 288).
/// Behaviour: server.creatures.armor-debuff-in-a-special
#[test]
fn a_special_after_rend_armor_does_what_it_took_from_the_creature() {
    use auberean_protocol::{queue, ServerMessage};
    use auberean_script::args::Arg;
    use yalain_content::{Prop, Props};
    use yalain_testkit::play::server_call;
    use yalain_testkit::{Harness, TEST_CREATURE};
    use yalain_world::skill_use::function;
    use yalain_world::state::SkillState;
    const REND_ARMOR: u32 = 766;
    const EXPOSE: u32 = 757;
    const WEAPON: u32 = 0x4700_07A3;
    let portal = Portal::open();
    let skills = portal.skills();
    let mut states: Vec<u32> = [REND_ARMOR, EXPOSE]
        .iter()
        .flat_map(|k| {
            skills.skills[k]
                .usage
                .as_ref()
                .expect("used")
                .target_effects
                .clone()
        })
        .collect();
    states.sort_unstable();
    let effects = portal.effects(&states);
    let fights = |rend_first: bool| -> Vec<i32> {
        let mut out = Vec::new();
        for seed in 1..40u64 {
            let mut h = Harness::new();
            h.add_creatures(1);
            h.server.skill_table = skills.clone();
            h.server.effect_table = effects.clone();
            h.server.rng = yalain_world::rng::Rng::new(seed);
            let (b, me) = h.player_in_world("one", "Elara");
            h.bots[b].send(
                true,
                queue::WEENIE,
                &auberean_protocol::ClientMessage::SceneRenderingComplete(
                    auberean_protocol::login::Empty,
                ),
            );
            h.run(5);
            let mut p = Props::default();
            p.set("Weapon_Damage", Prop::Int(310))
                .set("Weapon_Variance", Prop::Float(0.25));
            h.server
                .catalog
                .insert(yalain_testkit::test_template(WEAPON, 0x1067, p));
            h.server
                .catalog
                .implements
                .insert(WEAPON, yalain_world::skill_use::implement::BOW);
            h.arm(b, me, WEAPON);
            for (skill, rating) in [(REND_ARMOR, 50), (EXPOSE, 40)] {
                let d = h
                    .server
                    .skill_table
                    .skills
                    .get_mut(&skill)
                    .expect("a skill");
                d.parents.clear();
                d.prereqs.clear();
                let u = d.usage.as_mut().expect("used");
                u.implements_right = -1;
                u.implements_left = -1;
                u.vigor_add = 0.0;
                u.vigor_mult = 0.0;
                u.min_vigor = 0.0;
                u.min_range = -1.0;
                u.max_range = 1000.0;
                u.ui_flags &= !yalain_world::skill_use::ui_flag::MOVE_TO;
                let p = h.server.players.values_mut().next().expect("in the world");
                p.play.skills.push(SkillState {
                    skill,
                    // Its experience's level, none reaching 1, comes on top.
                    granted: rating - 1,
                    mask: 1,
                    cost: 0,
                    children: 0,
                    dependents: 0,
                    spent: 0,
                });
            }
            let foe = h
                .world_messages(b, 0)
                .iter()
                .find_map(|m| match m {
                    ServerMessage::CreateObject(o) if o.weenie.entity_did == TEST_CREATURE => {
                        Some(o.id)
                    }
                    _ => None,
                })
                .expect("the camp's creature");
            {
                let t = h.server.things.get_mut(foe).expect("the creature");
                t.max_health = 10_000_000;
                t.health = 10_000_000;
                let c = t.creature.as_mut().expect("a creature");
                c.stats.armor = 173;
                c.stats.fights = false;
            }
            let hit = |h: &mut Harness, id: u32, skill: u32| -> Option<i32> {
                let from = h.bots[b].inbox.len();
                h.bots[b].send(
                    true,
                    queue::WEENIE,
                    &server_call(
                        function::USE,
                        &[Arg::Int(id), Arg::Int(skill), Arg::LongInt(foe)],
                    ),
                );
                h.run(240);
                h.world_messages(b, from).iter().find_map(|m| {
                    let ServerMessage::DoStory(e) = m else {
                        return None;
                    };
                    e.action.hooks.iter().find_map(|k| {
                        (e.action.skill == skill && k.numbers[0] == 0x4000_0008 && k.numbers[3] < 0)
                            .then_some(-k.numbers[3])
                    })
                })
            };
            if rend_first {
                let mut landed = false;
                for id in 10..30 {
                    if hit(&mut h, id, REND_ARMOR).is_some() {
                        landed = true;
                        break;
                    }
                }
                assert!(landed, "Rend Armor landed");
                assert_eq!(yalain_world::combat::armor_taken(&h.server, foe), 150);
            } else {
                assert_eq!(yalain_world::combat::armor_taken(&h.server, foe), 0);
            }
            if let Some(d) = hit(&mut h, 40, EXPOSE) {
                out.push(d);
            }
        }
        out
    };
    let after = fights(true);
    let alone = fights(false);
    assert!(
        after.len() >= 10 && alone.len() >= 10,
        "{} {}",
        after.len(),
        alone.len()
    );
    assert!(after.iter().all(|d| (269..=347).contains(d)), "{after:?}");
    assert!(alone.iter().all(|d| (119..=197).contains(d)), "{alone:?}");
}

/// The items whose classes name a quest for taking them read with it: a piece of the Broth, for its
/// own quest at the step it asks; a thing with no such quest reads none.
/// Behaviour: server.content.take-gates-of-classes
#[test]
fn an_item_whose_class_names_a_quest_for_taking_reads_with_it() {
    let p = Portal::open();
    let t = p.template(0x4700_0609);
    assert_eq!(
        t.props.word("Usage_RequiredQuest_ForTaking"),
        Some(0x4000_0024)
    );
    assert_eq!(
        t.props.word("Usage_RequiredQuestStatus_ForTaking"),
        Some(0x1000_0002)
    );
    let medallion = p.template(0x4700_08CF);
    assert_eq!(medallion.props.word("Usage_RequiredQuest_ForTaking"), None);
}

/// Three skills' definitions say how much faster they go: Sinew Snap 1.6, the two drudges' 1.4; no
/// other's does.
/// Behaviour: server.specials.combat-speed-read
#[test]
fn only_three_skills_say_they_go_faster() {
    let portal = Portal::open();
    let skills = portal.skills();
    let mut said: Vec<(u32, f32)> = skills
        .skills
        .values()
        .filter_map(|d| Some((d.id, d.usage.as_ref()?.combat_speed?)))
        .collect();
    said.sort_by_key(|x| x.0);
    assert_eq!(said, [(994, 1.6), (2088, 1.4), (2113, 1.4)]);
}

/// An alchemist's basic attacks are told apart by what they name: its plain ones bar the dark side
/// and Immolation, its melee ones require the first and bar the second, and each has a fiery kind
/// that requires Immolation; all name Fire Breath to be trained.
/// Behaviour: server.weapons.basic-attacks-read-what-they-want
#[test]
fn an_alchemists_basic_attacks_name_what_they_want_and_what_bars_them() {
    const DARK_SIDE: u32 = 0x7000_1F43;
    const IMMOLATION: u32 = 0x7000_1F51;
    const FIRE_BREATH: u32 = 189;
    let skills = Portal::open().skills();
    let usage = |k: u32| skills.skills[&k].usage.as_ref().expect("used");
    for plain in [804, 805, 806] {
        let u = usage(plain);
        assert!(u.required_effects.is_empty());
        assert!(u.barring_effects.contains(&DARK_SIDE) && u.barring_effects.contains(&IMMOLATION));
        assert_eq!(u.use_prereqs, [(FIRE_BREATH, 0)]);
    }
    for melee in [953, 954, 955] {
        let u = usage(melee);
        assert_eq!(u.required_effects, [DARK_SIDE]);
        assert!(u.barring_effects.contains(&IMMOLATION));
    }
    for fiery in [1371, 1372, 1373] {
        let u = usage(fiery);
        assert_eq!(u.required_effects, [IMMOLATION]);
        assert!(u.barring_effects.contains(&DARK_SIDE));
    }
    for fiery_melee in [1374, 1375, 1376] {
        assert_eq!(usage(fiery_melee).required_effects, [DARK_SIDE, IMMOLATION]);
    }
}

/// A fresh character of any race holds basic attacks it may swing, wanting no effect and no skill it
/// lacks; a fresh human, some for each of its starting weapons.
/// Behaviour: server.weapons.basic-attacks-read-what-they-want
#[test]
fn a_new_characters_basic_attacks_want_nothing_it_lacks() {
    use auberean_assets::script::ScriptState;
    use auberean_script::graph::Graph;
    let p = Portal::open();
    let lib = p.library();
    let skills = p.skills();
    let bytes = p.dat.read(0x7000_0390).expect("character generation data");
    let st: ScriptState = auberean_assets::decode(0x7000_0390, &bytes, &p.ctx).expect("reads");
    let g = Graph::decode(&st.graph, &lib).expect("its graph reads");
    let rules = yalain_world::start::StartRules::from_matrix(&lib, &g).expect("the rules read");
    assert!(rules.skills.len() >= 3);
    for (race, list) in &rules.skills {
        let held: Vec<u32> = list.iter().map(|x| x.0).collect();
        let usable: Vec<u32> = held
            .iter()
            .copied()
            .filter(|k| {
                let Some(d) = skills.skills.get(k) else {
                    return false;
                };
                let Some(u) = d.usage.as_ref() else {
                    return false;
                };
                d.auto_attack
                    && u.required_effects.is_empty()
                    && u.use_prereqs.iter().all(|(n, _)| held.contains(n))
            })
            .collect();
        assert!(
            !usable.is_empty(),
            "race {race} starts with nothing to swing"
        );
        if *race == 1 {
            use yalain_world::skill_use::implement as im;
            for weapon in [im::SWORD, im::BOW, im::STAVE] {
                assert!(
                    usable.iter().any(|k| {
                        let u = skills.skills[k].usage.as_ref().expect("used");
                        u.implements_right.cast_unsigned() & weapon != 0
                    }),
                    "no human basic attack for {weapon:#x}"
                );
            }
        }
    }
}
