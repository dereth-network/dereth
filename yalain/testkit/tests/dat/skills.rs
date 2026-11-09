//! The skills' definitions as the server reads them from the September 2013 data: how each used
//! skill is used.

use std::collections::BTreeSet;

use yalain_content::Props;
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

/// A creature's attack is critical at the chance its skill's class sets, read from the class's code:
/// the plain attacks of each kind never, `Strong` a fortieth of the time, `Stronger` and the harming
/// missile a twentieth; a player's Critical Shot sets a fifth.
/// Behaviour: server.combat.creature-critical-by-skill
#[test]
fn creatures_attacks_are_critical_at_the_chance_their_skills_class_sets() {
    let t = Portal::open().skills();
    for (skill, chance) in [
        (1845, 0.0),
        (1861, 0.0),
        (1840, 0.0),
        (1846, 0.025),
        (1865, 0.025),
        (1841, 0.025),
        (1847, 0.05),
        (1866, 0.05),
        (1842, 0.05),
        (1860, 0.05),
    ] {
        let a = attack_skill(&t, skill, 1).expect("an attack");
        assert!(
            (a.critical - chance).abs() < 1e-6,
            "{skill}: {}",
            a.critical
        );
    }
    assert_eq!(
        t.skills[&205].critical_threshold,
        Some(1.8),
        "Critical Shot"
    );
    let set = t
        .skills
        .values()
        .filter(|d| d.usage.is_some() && d.critical_threshold.is_some())
        .count();
    assert!(set > 1_500, "most used skills set one: {set}");
}

/// How much more a special's critical does is what its skill's class sets, read from the class's
/// code with its bases': Nettle, Critical Shot and Death Blow, which set a threshold of 1.8, set 2,
/// as every class at 1.8 does; Double Shot sets 1.2; the creatures' plain melee attack takes 1 from
/// its base.
/// Behaviour: server.specials.critical-modifier-read
#[test]
fn a_skills_critical_modifier_is_what_its_class_sets() {
    let t = Portal::open().skills();
    for (skill, modifier) in [(137, 2.0), (205, 2.0), (1900, 2.0), (209, 1.2), (493, 1.0)] {
        assert_eq!(
            t.skills[&skill].critical_modifier,
            Some(modifier),
            "{skill}"
        );
    }
    let critical_kind: Vec<_> = t
        .skills
        .values()
        .filter(|d| d.critical_threshold == Some(1.8))
        .map(|d| (d.id, d.critical_modifier))
        .collect();
    assert!(critical_kind.len() >= 13, "{critical_kind:?}");
    assert!(
        critical_kind.iter().all(|(_, m)| *m == Some(2.0)),
        "{critical_kind:?}"
    );
    let set = t
        .skills
        .values()
        .filter(|d| d.usage.is_some() && d.critical_modifier.is_some())
        .count();
    assert!(set > 1_500, "most used skills set one: {set}");
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
/// own quest at the step it asks; a relic through its class's base; a heart whose class sets only the
/// status for using it, with no status for taking; a thing with no such quest reads none.
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
    let relic = p.template(0x4700_11C7);
    assert_eq!(
        (
            relic.props.word("Usage_RequiredQuest_ForTaking"),
            relic.props.word("Usage_RequiredQuestStatus_ForTaking")
        ),
        (Some(0x4000_006D), Some(0x1000_0004))
    );
    let heart = p.template(0x4700_080B);
    assert_eq!(
        (
            heart.props.word("Usage_RequiredQuest_ForTaking"),
            heart.props.word("Usage_RequiredQuestStatus_ForTaking")
        ),
        (Some(0x4000_005A), None)
    );
    let medallion = p.template(0x4700_08CF);
    assert_eq!(medallion.props.word("Usage_RequiredQuest_ForTaking"), None);
}

/// The quests taking items ask, as the install's classes name them: every template whose class or
/// one of its bases sets a quest for taking it, read by the quest's name, at the status for taking
/// its class sets (none where it sets only the status for using it).
fn quests_for_taking() -> std::collections::BTreeMap<u32, (u32, Option<u32>)> {
    use yalain_content::props::{TAKING_QUEST, TAKING_STATUS};
    use yalain_content::Props;
    let p = Portal::open();
    let lib = p.library();
    let quest = |name: &str| p.quest_ids().get(name).copied();
    let entity = |id: u32| auberean_assets::decode(id, &p.dat.read(id).ok()?, &p.ctx).ok();
    let mut by_class = std::collections::HashMap::new();
    let ids: Vec<u32> = p
        .dat
        .entries_in(0x4700_0000, 0x47FF_FFFF)
        .map(|e| e.id)
        .collect();
    let mut read = std::collections::BTreeMap::new();
    for t in ids {
        let Some(class) = Props::class_of(t, &entity) else {
            continue;
        };
        let said = by_class
            .entry(class)
            .or_insert_with(|| yalain_world_db::install::class_properties(&lib, class));
        let props = Props::default().with_class(said).with_quest_named(&quest);
        if let Some(q) = props.word(TAKING_QUEST) {
            read.insert(t, (q, props.word(TAKING_STATUS)));
        }
    }
    read
}

/// The install's classes keep 93 templates from those who may not take them: 87 at the status for
/// taking their classes set, among them the Ancients' and Sun Blaze relics by their Golden Age
/// bases; and 6 under way at any phase, since their classes set only the status for using them.
/// Behaviour: server.content.take-gates-of-classes
#[test]
fn the_quests_taking_items_ask_read_from_their_classes_code() {
    let read = quests_for_taking();
    assert_eq!(read.len(), 93);
    let relics = read
        .values()
        .filter(|g| **g == (0x4000_006D, Some(0x1000_0004)))
        .count();
    assert_eq!(relics, 57, "the Golden Age relics and their kin");
    for t in [0x4700_11C7, 0x4700_14E3, 0x4700_2058, 0x4700_186E] {
        assert_eq!(
            read.get(&t),
            Some(&(0x4000_006D, Some(0x1000_0004))),
            "{t:#x}"
        );
    }
    for (t, quest) in [
        (0x4700_080B, 0x4000_005A),
        (0x4700_08D1, 0x4000_006D),
        (0x4700_1463, 0x4000_007E),
        (0x4700_1464, 0x4000_007E),
        (0x4700_1465, 0x4000_007E),
        (0x4700_1867, 0x4000_006D),
    ] {
        assert_eq!(read.get(&t), Some(&(quest, None)), "{t:#x}");
    }
    assert_eq!(read.values().filter(|g| g.1.is_none()).count(), 6);
}

/// The table a generator once made of the quests taking items ask, against the install's classes:
/// 49 of its 55 rows are what the classes say; its other 6 hold the status for using the item the
/// classes set, not one for taking it; and the classes gate 38 more, through their bases.
/// Behaviour: server.content.take-gates-of-classes
#[test]
fn the_generated_take_gates_differ_from_the_classes_in_six_statuses_and_38_relics() {
    const GENERATED: [(u32, u32, u32); 55] = [
        (0x47000605, 0x4000005C, 0x10000000),
        (0x47000608, 0x40000023, 0x10000002),
        (0x47000609, 0x40000024, 0x10000002),
        (0x4700060A, 0x40000025, 0x10000002),
        (0x4700060B, 0x40000026, 0x10000002),
        (0x4700060C, 0x40000027, 0x10000002),
        (0x4700060D, 0x4000002C, 0x30000000),
        (0x47000675, 0x40000010, 0x10000002),
        (0x4700080B, 0x4000005A, 0x30000000),
        (0x470008D0, 0x4000006D, 0x10000000),
        (0x470008D1, 0x4000006D, 0x10000003),
        (0x47000AEA, 0x4000005A, 0x10000000),
        (0x47000AEB, 0x4000005A, 0x10000000),
        (0x47000AEC, 0x4000005A, 0x10000000),
        (0x47000AED, 0x4000005A, 0x10000000),
        (0x47000AEE, 0x4000005A, 0x10000000),
        (0x47001330, 0x40000069, 0x10000003),
        (0x47001331, 0x4000006B, 0x10000002),
        (0x47001332, 0x4000006A, 0x10000002),
        (0x47001333, 0x4000006C, 0x10000002),
        (0x47001346, 0x40000084, 0x10000002),
        (0x47001463, 0x4000007E, 0x10000002),
        (0x47001464, 0x4000007E, 0x10000002),
        (0x47001465, 0x4000007E, 0x10000002),
        (0x47001469, 0x4000007E, 0x10000002),
        (0x4700146A, 0x4000007E, 0x10000002),
        (0x4700146B, 0x4000007E, 0x10000002),
        (0x4700146C, 0x4000007E, 0x10000002),
        (0x470014D8, 0x40000079, 0x10000002),
        (0x470016C0, 0x40000076, 0x10000001),
        (0x470016C1, 0x40000077, 0x10000001),
        (0x470016C2, 0x40000078, 0x10000001),
        (0x47001864, 0x4000006D, 0x10000004),
        (0x47001865, 0x4000006D, 0x10000004),
        (0x47001866, 0x4000006D, 0x10000004),
        (0x47001867, 0x4000006D, 0x10000001),
        (0x47001868, 0x4000006D, 0x10000004),
        (0x47001869, 0x4000006D, 0x10000004),
        (0x4700186A, 0x4000006D, 0x10000004),
        (0x4700186B, 0x4000006D, 0x10000004),
        (0x4700186C, 0x4000006D, 0x10000004),
        (0x4700186E, 0x4000006D, 0x10000004),
        (0x47001870, 0x4000006D, 0x10000004),
        (0x47001871, 0x4000006D, 0x10000004),
        (0x47001872, 0x4000006D, 0x10000004),
        (0x47001873, 0x4000006D, 0x10000004),
        (0x47001874, 0x4000006D, 0x10000004),
        (0x47001875, 0x4000006D, 0x10000004),
        (0x47001876, 0x4000006D, 0x10000004),
        (0x47001877, 0x4000006D, 0x10000004),
        (0x47001878, 0x4000006D, 0x10000004),
        (0x47001AEC, 0x40000088, 0x10000005),
        (0x47001AED, 0x40000088, 0x10000005),
        (0x47001AF1, 0x40000088, 0x10000003),
        (0x47002057, 0x4000006D, 0x10000004),
    ];
    let read = quests_for_taking();
    let mut same = 0;
    let mut use_status = Vec::new();
    for (t, quest, status) in GENERATED {
        match read.get(&t) {
            Some(&(q, Some(s))) if (q, s) == (quest, status) => same += 1,
            Some(&(q, None)) if q == quest => use_status.push(t),
            other => panic!("{t:#x}: {other:?}"),
        }
    }
    assert_eq!(same, 49);
    assert_eq!(
        use_status,
        [
            0x4700_080B,
            0x4700_08D1,
            0x4700_1463,
            0x4700_1464,
            0x4700_1465,
            0x4700_1867
        ]
    );
    let more: Vec<u32> = read
        .keys()
        .copied()
        .filter(|t| !GENERATED.iter().any(|g| g.0 == *t))
        .collect();
    assert_eq!(more.len(), 38, "{more:x?}");
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

/// The skills aimed at items read with what they take: a shield of the user's, a weapon in hand; their
/// effects raise a rating and a damage by the amounts 2017's profiles show (a shield's rating 40 and
/// damage 61 at spellcraft 30, a weapon's rating 67 at 48 and 70 at 51), for half an hour, and a second
/// effect on each attunes the item for as long.
/// Behaviour: server.skills.a-skill-aimed-at-an-item-lands-on-it
#[test]
fn the_skills_aimed_at_items_read_with_what_they_take_and_do() {
    use yalain_world::made::stat;
    use yalain_world::skill_use::{aims_at_item, object_kind, target_bit};
    let portal = Portal::open();
    let t = portal.skills();
    let e = portal.effects(&[
        0x7000_07D3,
        0x7000_1F3A,
        0x7000_1F7C,
        0x7000_1F7D,
        0x6F00_0039,
    ]);
    let at = |state: u32, spellcraft: f32, want: u32| {
        e.get(state)
            .expect("known")
            .changes(spellcraft)
            .iter()
            .filter(|(s, m, _)| *s == want && !*m)
            .map(|(_, _, by)| *by)
            .sum::<f32>()
            .floor()
    };
    let emblem = t.skills[&148].usage.clone().expect("used");
    assert!(aims_at_item(&emblem));
    assert_ne!(emblem.valid_targets & target_bit::OWN_ITEM, 0);
    assert_eq!(emblem.valid_object_kinds, vec![object_kind::SHIELD]);
    assert_eq!(emblem.target_effects, vec![0x7000_07D3, 0x7000_1F3A]);
    assert_eq!(at(0x7000_07D3, 30.0, stat::ARMOR), 40.0);
    assert_eq!(at(0x7000_07D3, 30.0, stat::DAMAGE), 61.0);
    let galvanize = t.skills[&1259].usage.clone().expect("used");
    assert_ne!(galvanize.valid_targets & target_bit::ITEM_IN_HAND, 0);
    assert_eq!(galvanize.valid_object_kinds, vec![object_kind::WEAPON]);
    assert_eq!(galvanize.target_effects, vec![0x7000_1F7C, 0x7000_1F7D]);
    assert_eq!(at(0x7000_1F7C, 48.0, stat::ARMOR), 67.0);
    assert_eq!(at(0x7000_1F7C, 51.0, stat::ARMOR), 70.0);
    let board = t.skills[&2160].usage.clone().expect("used");
    assert_eq!(board.valid_object_kinds, vec![object_kind::WEAPON]);
    assert_eq!(board.target_effects, vec![0x6F00_0039]);
    for state in [0x7000_07D3, 0x7000_1F7C, 0x7000_1F3A, 0x7000_1F7D] {
        assert_eq!(e.get(state).expect("known").duration.at(30.0), Some(1800.0));
    }
    for state in [0x7000_1F3A, 0x7000_1F7D] {
        assert_eq!(e.get(state).expect("known").eq_class, 114);
    }
    // What a skill names a weapon or a shield by is the class what a hand holds is known by.
    let lib = portal.library();
    let of = |class: &str, base: &str| {
        lib.derives_from(
            lib.package_id(class).expect("the class"),
            lib.package_id(base).expect("its base"),
        )
    };
    for (class, kind) in yalain_world::skill_use::IMPLEMENT_CLASSES {
        use yalain_world::skill_use::implement::{NONE, SHIELD};
        assert_eq!(of(class, "Shield"), kind == SHIELD, "{class}");
        // A lute is a weapon by its class that a hand holds as nothing; it alone is told apart.
        if class != "LuteTemplate" {
            assert_eq!(
                of(class, "Weapon"),
                kind != SHIELD && kind != NONE,
                "{class}"
            );
        }
    }
}

/// No skill aimed at a weapon can come to a lute: each asks its user's right hand to hold something,
/// and a lute, which is held as nothing, goes in the right hand and nowhere else.
/// Behaviour: server.skills.a-skill-aimed-at-an-item-refused
#[test]
fn no_skill_aimed_at_a_weapon_can_come_to_a_lute() {
    use yalain_world::skill_use::{implement, object_kind};
    const RIGHT_HAND: u32 = 64;
    let p = Portal::open();
    let aimed: Vec<_> = p
        .skills()
        .skills
        .values()
        .filter_map(|d| d.usage.clone())
        .filter(|u| u.valid_object_kinds.contains(&object_kind::WEAPON))
        .collect();
    assert_eq!(aimed.len(), 2);
    for u in &aimed {
        assert_eq!(u.implements_right.cast_unsigned() & implement::NONE, 0);
    }
    // Each of the kinds of lute and flute 2017 examined.
    for id in [
        0x4700_065B,
        0x4700_065D,
        0x4700_065E,
        0x4700_065F,
        0x4700_0660,
        0x4700_0661,
        0x4700_0662,
        0x4700_0663,
        0x4700_0664,
        0x4700_0673,
        0x4700_0674,
        0x4700_1148,
        0x4700_114B,
        0x4700_1A3A,
    ] {
        assert!(p.derives(id, &["LuteTemplate"]), "{id:#x}");
        assert_eq!(
            p.template(id).props.word("Item_ValidInventoryLocations"),
            Some(RIGHT_HAND),
            "{id:#x}"
        );
    }
}

/// Each contract is a template whose acquiring hires, and its mercenary a template that makes a pet;
/// no pet a skill summons is of a mercenary's class, and what a mercenary tends with heals.
/// Behaviour: server.pets.a-contract-hires-a-mercenary
#[test]
fn the_contracts_and_their_mercenaries_read_from_the_data() {
    use yalain_testkit::mercenary;
    use yalain_world::effect_defs::Kind;
    let portal = Portal::open();
    let kinds = yalain_testkit::test_pet_kinds();
    for contract in [mercenary::CRAFTER, mercenary::KEEPER, mercenary::MEDIC] {
        let t = portal.template(contract);
        assert!(yalain_world::quests::acquiring_does_more_than_quests(
            &t.props
        ));
        let pet = portal.template(
            kinds
                .iter()
                .find(|k| k.effect == contract)
                .expect("a kind")
                .template,
        );
        let stats = yalain_content::CreatureStats::from_props(&pet.props).expect("a pet");
        assert_eq!(stats.max_health, 150);
    }
    let table = portal.skills();
    let summons: Vec<u32> = table
        .skills
        .values()
        .filter_map(|d| d.usage.as_ref())
        .flat_map(|u| u.user_effects.iter().copied())
        .collect();
    let effects = portal.effects(&summons);
    for d in effects.effects.values() {
        if let Kind::Pet { class, .. } = d.kind {
            assert_eq!(class & mercenary::CLASS, 0, "{:08X}", d.state);
        }
    }
    let heal = portal.effects(&[mercenary::TEND_EFFECT]);
    assert!(matches!(
        heal.get(mercenary::TEND_EFFECT).expect("known").kind,
        Kind::InstantVital { .. }
    ));
}

/// The perks read from their definitions: 187 of them; Transmutation Master gives 1.03 to 1.18 times an
/// item's worth, each overriding the one before it by its priority.
/// Behaviour: server.skills.perks-read-from-their-definitions
#[test]
fn perks_read_with_their_kind_value_and_priority() {
    let t = Portal::open().skills();
    let perks = t.skills.values().filter(|d| d.perk.is_some()).count();
    assert_eq!(perks, 187);
    let perk = |id: u32| t.skills[&id].perk.expect("a perk");
    let values: Vec<f64> = (2053..=2058).map(|id| perk(id).value).collect();
    assert_eq!(values, [1.03, 1.06, 1.09, 1.12, 1.15, 1.18]);
    let priorities: Vec<i32> = (2053..=2058).map(|id| perk(id).priority).collect();
    assert_eq!(priorities, [1, 2, 3, 4, 5, 6]);
    assert!((2053..=2058).all(|id| perk(id).kind == 19));
}

/// The install's six Mining Master perks read as more units on each use of a summoned node: 2, 4,
/// 6, 9, 14 and 20, each overriding the one before it.
/// Behaviour: server.craft.mining-units-by-tool-and-perk
#[test]
fn the_mining_master_perks_read_as_units_on_each_use() {
    use yalain_world::skills::perk_kind::MINED_UNITS;
    let t = Portal::open().skills();
    let perk = |id: u32| t.skills[&id].perk.expect("a perk");
    let values: Vec<f64> = (2022..=2027).map(|id| perk(id).value).collect();
    assert_eq!(values, [2.0, 4.0, 6.0, 9.0, 14.0, 20.0]);
    let priorities: Vec<i32> = (2022..=2027).map(|id| perk(id).priority).collect();
    assert_eq!(priorities, [1, 2, 3, 4, 5, 6]);
    assert!((2022..=2027).all(|id| perk(id).kind == MINED_UNITS));
    let kinds = t
        .skills
        .values()
        .filter_map(|d| d.perk)
        .filter(|p| p.kind == MINED_UNITS)
        .count();
    assert_eq!(kinds, 6, "no other perk is of the kind");
}

/// A hero trains the install's first Transmutation Master for its 100 hero credits and the money bag
/// pays it 1.03 times the install's values, as it paid in 2017: 4,000 brought 4,120 and five of
/// 1,000 brought 5,150. Without the perk those pay their value.
/// Behaviour: server.inventory.money-bag-pays-a-perk
#[test]
fn a_hero_with_the_first_transmutation_master_is_paid_as_in_2017() {
    use auberean_script::args::Arg;
    use yalain_world::pack::function::TO_MONEY_BAG;
    const FOUR_THOUSAND: u32 = 0x4700_1C91;
    const ONE_THOUSAND: u32 = 0x4700_140F;
    let portal = Portal::open();
    let mut h = yalain_testkit::Harness::new();
    h.server.skill_table = portal.skills();
    h.server.catalog.insert(portal.template(FOUR_THOUSAND));
    h.server.catalog.insert(portal.template(ONE_THOUSAND));
    let (b, me) = h.player_in_world("one", "Elara");
    let world = *h.server.players.keys().next().expect("in the world");
    let sell = |h: &mut yalain_testkit::Harness, template: u32, n: u32| {
        yalain_world::inventory::give(&mut h.server, &mut h.net, world, template, n);
        h.run(3);
        yalain_world::quals::set_int(
            &mut h.server.players.get_mut(&world).unwrap().qualities,
            yalain_world::store::MONEY,
            0,
        );
        let item = h.server.players[&world]
            .play
            .items
            .iter()
            .find(|i| i.template == template)
            .expect("in the pack")
            .id;
        h.ask(
            b,
            TO_MONEY_BAG,
            &[
                Arg::LongInt(me),
                Arg::Int(0),
                Arg::Int(n),
                Arg::LongInt(item),
            ],
            5,
        );
        yalain_world::store::money(&h.server.players[&world].qualities)
    };
    assert_eq!(sell(&mut h, FOUR_THOUSAND, 1), 4_000, "no perk");
    assert_eq!(sell(&mut h, ONE_THOUSAND, 5), 5_000, "no perk");
    let p = h.server.players.get_mut(&world).unwrap();
    yalain_world::focus::set_hero_kept(&mut p.qualities, true);
    p.play.hero_credits = 100;
    h.ask(
        b,
        yalain_world::skills::function::TRAIN,
        &[Arg::Int(2053)],
        10,
    );
    assert_eq!(h.server.players[&world].play.hero_credits, 0, "paid for");
    assert_eq!(sell(&mut h, FOUR_THOUSAND, 1), 4_120);
    assert_eq!(sell(&mut h, ONE_THOUSAND, 5), 5_150);
}

/// A player holding a skill, as a saved character's list holds it.
fn holding(h: &mut yalain_testkit::Harness, world: yalain_net::ConnId, skills: &[u32]) {
    let p = h.server.players.get_mut(&world).expect("in the world");
    for skill in skills {
        p.play.skills.push(yalain_world::state::SkillState {
            skill: *skill,
            granted: 0,
            mask: yalain_world::skills::mask::TRAINED,
            cost: 0,
            children: 0,
            dependents: 0,
            spent: 0,
        });
    }
}

/// The perks of a vital's greatest add their value to it, and the repository tells the client
/// which perk gives each kind: Vitality at its second rank, Intelligence and Heroism at their first,
/// 100, 50 and 10; without them the greatest is what the qualities hold. What is kept isn't touched:
/// its own vital stays where it was, and giving the perk up brings a vital above it back within it.
/// Behaviour: server.skills.perks-raise-a-vitals-greatest
#[test]
fn the_perks_of_a_vitals_greatest_raise_it_and_the_repository_says_so() {
    use yalain_world::effects::{max_vital, vitals_within_their_greatest};
    use yalain_world::player::quality;
    let portal = Portal::open();
    let mut h = yalain_testkit::Harness::new();
    h.server.skill_table = portal.skills();
    let (_, _) = h.player_in_world("one", "Elara");
    let world = *h.server.players.keys().next().expect("in the world");
    let greatest = |h: &yalain_testkit::Harness, q: u32| {
        let p = &h.server.players[&world];
        max_vital(&h.server, p, q)
    };
    let (health, vigor, focus) = (
        greatest(&h, quality::HEALTH_MAX),
        greatest(&h, quality::VIGOR_MAX),
        greatest(&h, quality::FOCUS_MAX),
    );
    let before = yalain_world::quals::int(&h.server.players[&world].qualities, quality::HEALTH);
    holding(&mut h, world, &[2152, 2074, 2141]);
    assert_eq!(greatest(&h, quality::HEALTH_MAX), health + 100);
    assert_eq!(greatest(&h, quality::VIGOR_MAX), vigor + 50);
    assert_eq!(greatest(&h, quality::FOCUS_MAX), focus + 10);
    assert_eq!(
        yalain_world::quals::int(&h.server.players[&world].qualities, quality::HEALTH),
        before,
        "nothing kept moves"
    );
    let p = &h.server.players[&world];
    let starting = h
        .server
        .start
        .skills
        .get(&p.race)
        .cloned()
        .unwrap_or_default();
    let tables = yalain_world::skills::RepositoryTables::of(
        &h.server.skill_table,
        &yalain_world::skills::held(&starting, &p.play.skills),
    );
    assert!(tables.perks.contains(&(40, 2152)));
    assert!(tables.perks.contains(&(39, 2074)));
    assert!(tables.perks.contains(&(41, 2141)));
    let p = h.server.players.get_mut(&world).expect("in the world");
    yalain_world::quals::set_int(&mut p.qualities, quality::HEALTH, health + 100);
    p.play.skills.clear();
    vitals_within_their_greatest(&mut h.server, &mut h.net, world);
    assert_eq!(
        yalain_world::quals::int(&h.server.players[&world].qualities, quality::HEALTH),
        Some(health),
        "given up, within what is left"
    );
}

/// Of a kind's perks the table names the one of the highest priority, whichever came first; the
/// kinds and categories a character doesn't hold stay out; every category it holds is told, the
/// damage pool and Heroic Mastery a client reads into its attacks with the rest.
/// Behaviour: server.skills.repository-tables
#[test]
fn the_repository_tables_name_the_best_perk_of_a_kind_and_each_held_category() {
    use yalain_world::skills::{held, RepositoryTables};
    let t = Portal::open().skills();
    let of = |ids: &[u32]| {
        let kept: Vec<_> = ids
            .iter()
            .map(|skill| yalain_world::state::SkillState {
                skill: *skill,
                granted: 0,
                mask: yalain_world::skills::mask::TRAINED,
                cost: 0,
                children: 0,
                dependents: 0,
                spent: 0,
            })
            .collect();
        RepositoryTables::of(&t, &held(&[], &kept))
    };
    assert_eq!(of(&[2152, 2150]).perks, vec![(40, 2152)]);
    assert_eq!(of(&[2150, 2152]).perks, vec![(40, 2152)]);
    assert_eq!(of(&[]), RepositoryTables::default());
    let by_category = |c: u32| {
        t.skills
            .iter()
            .find(|(_, d)| d.category == c && d.perk.is_none())
            .map(|(id, _)| *id)
            .expect("a skill of the category")
    };
    assert_eq!(of(&[by_category(5)]).categories, vec![(5, by_category(5))]);
    for c in [3, 6] {
        assert_eq!(
            of(&[by_category(c)]).categories,
            vec![(c, by_category(c))],
            "{c}"
        );
    }
}

/// Nocturnal Blood adds to the vigor rates by night and Diurnal Blood by day, 0.3 each, and neither
/// to health's.
/// Behaviour: server.skills.blood-perks-raise-vigor-by-the-hour
#[test]
fn the_blood_perks_raise_the_vigor_rates_at_their_own_hour() {
    use yalain_world::regen::{rate, rate_of};
    let portal = Portal::open();
    let mut h = yalain_testkit::Harness::new();
    h.server.skill_table = portal.skills();
    let (_, _) = h.player_in_world("one", "Elara");
    let world = *h.server.players.keys().next().expect("in the world");
    holding(&mut h, world, &[2031]);
    let p = &h.server.players[&world];
    let at = |which: u32, night: bool| rate_of(&h.server, p, which, night);
    assert!((at(rate::PEACE_VIGOR, true) - at(rate::PEACE_VIGOR, false) - 0.3).abs() < 1e-5);
    assert!((at(rate::COMBAT_VIGOR, true) - at(rate::COMBAT_VIGOR, false) - 0.3).abs() < 1e-5);
    assert!((at(rate::PEACE_HEALTH, true) - at(rate::PEACE_HEALTH, false)).abs() < 1e-5);
    h.server
        .players
        .get_mut(&world)
        .unwrap()
        .play
        .skills
        .clear();
    holding(&mut h, world, &[1979]);
    let p = &h.server.players[&world];
    let at = |which: u32, night: bool| rate_of(&h.server, p, which, night);
    assert!((at(rate::PEACE_VIGOR, false) - at(rate::PEACE_VIGOR, true) - 0.3).abs() < 1e-5);
}

/// Fast Learner pays a kill's experience times its value, floored; without it, the same.
/// Behaviour: server.skills.fast-learner
#[test]
fn fast_learner_raises_what_a_kill_pays_floored() {
    let portal = Portal::open();
    let mut h = yalain_testkit::Harness::new();
    h.server.skill_table = portal.skills();
    let (_, _) = h.player_in_world("one", "Elara");
    let world = *h.server.players.keys().next().expect("in the world");
    let pay = |h: &yalain_testkit::Harness, xp| {
        yalain_world::skills::with_fast_learner(&h.server, world, xp)
    };
    assert_eq!(pay(&h, 1_000), 1_000);
    holding(&mut h, world, &[1983]);
    assert_eq!(pay(&h, 1_000), 1_010);
    assert_eq!(pay(&h, 150), 151);
}

/// A session start tells the perks its character holds, as the updates after it do.
/// Behaviour: server.skills.session-start-tells-its-perks
#[test]
fn a_session_start_and_an_update_tell_the_perks_a_character_holds() {
    const WORDS: [u8; 12] = [0, 1, 0, 0x44, 0, 0, 0, 0, 1, 0, 0xFF, 0xFF];
    let portal = Portal::open();
    let mut h = yalain_testkit::Harness::new();
    h.server.skill_table = portal.skills();
    h.server.start.skills.insert(1, vec![(2150, 0)]);
    h.server.start.skills.insert(2, vec![(2150, 0)]);
    h.server.start.skills.insert(4, vec![(2150, 0)]);
    let (b, _) = h.player_in_world("one", "Elara");
    let holds_perk = |raw: &[u8]| {
        let at = raw
            .windows(WORDS.len())
            .position(|w| w == WORDS)
            .expect("a table");
        let n = u16::from_le_bytes([raw[at + 12], raw[at + 13]]);
        n == 1
            && u32::from_le_bytes(raw[at + 16..at + 20].try_into().unwrap()) == 40
            && u32::from_le_bytes(raw[at + 20..at + 24].try_into().unwrap()) == 2150
    };
    let start = h
        .world_messages(b, 0)
        .into_iter()
        .find(|m| {
            matches!(m, auberean_protocol::ServerMessage::PlayerCall(c) if c.function.0 == yalain_world::session::function::SESSION_START)
        })
        .expect("a session start");
    assert!(holds_perk(&start.encode()), "the session start");
    let world = *h.server.players.keys().next().expect("in the world");
    let p = &h.server.players[&world];
    let update = yalain_world::skills::repository_of(&h.server, p);
    assert!(holds_perk(&update.encode()), "an update");
}

/// A new character of each of the five races, either sex, holds the items its data names: the
/// weapons (and a Drudge's robe) in its pack, the clothes it is to wear worn where they go. It is
/// made wearing them, each in the color its physique chose, and is drawn so at its entry and again
/// at the next.
/// Behaviour: server.characters.starting-pack-from-the-install
#[test]
fn a_new_character_of_each_race_holds_its_weapons_and_wears_its_clothes() {
    use auberean_protocol::login::{CharacterCreated, CharacterRef, CreateCharacter};
    use auberean_protocol::table::{Table, SMALL};
    use auberean_protocol::text::{NarrowString, WideString};
    use auberean_protocol::{queue, ClientMessage, ServerMessage};
    use yalain_world::chargen::{FEMALE, MALE};
    let p = Portal::open();
    let rules = p.start_rules();
    let lib = p.library();
    let mut catalog = yalain_content::Catalog::default();
    for t in rules.items.values().flatten() {
        catalog.insert(p.template(t.template));
    }
    catalog.implements =
        yalain_world::skill_use::implements_of_catalog(&catalog, |package, class| {
            lib.package_id(class)
                .is_some_and(|c| lib.derives_from(package, c))
        });
    for (race, pack, worn) in [
        (
            1,
            vec![0x4700_051B, 0x4700_051C, 0x4700_051D],
            vec![(0x4700_051E, 2), (0x4700_051F, 16), (0x4700_0520, 8)],
        ),
        (
            2,
            vec![0x4700_0515, 0x4700_0516, 0x4700_0517],
            vec![(0x4700_0518, 2), (0x4700_0519, 8), (0x4700_051A, 16)],
        ),
        (
            4,
            vec![0x4700_0521, 0x4700_0522, 0x4700_0523],
            vec![(0x4700_0524, 2), (0x4700_0525, 8)],
        ),
        (8, vec![0x4700_052C, 0x4700_052D, 0x4700_052E], vec![]),
        (
            16,
            vec![0x4700_0526, 0x4700_0527, 0x4700_0528],
            vec![(0x4700_0529, 2), (0x4700_052A, 16), (0x4700_052B, 8)],
        ),
    ] {
        for sex in [MALE, FEMALE] {
            let mut h = yalain_testkit::Harness::new();
            let areas = h.server.start.by_race.clone();
            h.server.start = rules.clone();
            h.server.start.by_race = areas;
            h.server.start.areas = yalain_testkit::test_start().areas;
            h.server.catalog = catalog.clone();
            let account = format!("race{race}sex{sex:x}");
            let b = h.at_character_list(&account);
            let before = h.bots[b].inbox.len();
            let m = ClientMessage::CreateCharacter(CreateCharacter {
                account: NarrowString::from(account.as_str()),
                name: WideString::from("Newcomer"),
                entity_did: 0x4700_110F,
                start_area: 0x4000_0001,
                race,
                sex,
                class: 2,
                implement: 1,
                physique: Table::from_inserted(
                    SMALL,
                    [
                        (1, 1.0),
                        (2, 1.0),
                        (0x100, 0.25),
                        (0x200, 0.5),
                        (0x400, 0.75),
                    ],
                ),
            });
            h.bots[b].send(false, queue::LOGON, &m);
            assert!(h.run_until(1000, |h| h.bots[b].inbox.len() > before));
            let Some(ServerMessage::CharacterCreated(CharacterCreated { identity, .. })) =
                h.bots[b].inbox[before].message()
            else {
                panic!("race {race} is not answered with a character");
            };
            let me = identity.id;
            assert_ne!(me, 0, "race {race} makes a character");
            let made = identity.visual.clone();
            // Each garment gives what its physique type names, but a human woman's boots: their
            // class names the men's for both.
            let garments = h.server.start.garments(&h.server.catalog, race, sex);
            assert_eq!(
                garments.len(),
                worn.len(),
                "race {race}: every garment drawn"
            );
            for (color, file) in &garments {
                let named = h.server.rules.physique[&(race, sex)]
                    .iter()
                    .find(|e| e.physique_type == *color && e.modifier == 2)
                    .map(|e| e.appearance);
                let want = if (race, sex, *color) == (1, FEMALE, 0x400) {
                    Some(0x2000_0019)
                } else {
                    named
                };
                assert_eq!(Some(*file), want, "race {race} sex {sex:#x}: {color:#x}");
            }
            for (color, file) in garments {
                let want = [0.25, 0.5, 0.75][(color.trailing_zeros() - 8) as usize];
                assert_eq!(
                    made.global.appearance.get(file).map(|t| t.entries.clone()),
                    Some(vec![(2, want), (0x10, 1.0)]),
                    "race {race} sex {sex:#x}: made wearing {file:#x}"
                );
            }
            let drawn = |h: &yalain_testkit::Harness, from: usize| {
                h.world_messages(b, from)
                    .iter()
                    .find_map(|m| match m {
                        ServerMessage::CreateObject(o) if o.id == me => Some(o.visual.clone()),
                        _ => None,
                    })
                    .expect("the player made")
            };
            let from = h.bots[b].inbox.len();
            h.enter_world(b, &account, me);
            assert_eq!(drawn(&h, from), made, "race {race}: drawn as made");
            let held = |h: &yalain_testkit::Harness, worn: bool| -> Vec<(u32, u32)> {
                h.server
                    .players
                    .values()
                    .find(|p| p.character == me)
                    .expect("in the world")
                    .play
                    .items
                    .iter()
                    .filter(|i| (i.location != 0) == worn)
                    .map(|i| (i.template, i.location))
                    .collect()
            };
            let in_pack: Vec<u32> = held(&h, false).iter().map(|x| x.0).collect();
            assert_eq!(in_pack, pack, "race {race}: the pack");
            assert_eq!(held(&h, true), worn, "race {race}: worn");
            // Each garment's color lies on its own pile appearance, else a starting garment's.
            for (template, _) in &worn {
                let id = h
                    .server
                    .players
                    .values()
                    .find(|p| p.character == me)
                    .expect("in")
                    .play
                    .items
                    .iter()
                    .find(|i| i.template == *template)
                    .expect("held")
                    .id;
                let table = h
                    .server
                    .catalog
                    .get(*template)
                    .expect("known")
                    .props
                    .word("Item_PileAppearance")
                    .unwrap_or(yalain_world::start::CLOTHES_COLORS);
                let o = h
                    .world_messages(b, from)
                    .iter()
                    .find_map(|m| match m {
                        ServerMessage::CreateObject(o) if o.id == id => Some(o.visual.clone()),
                        _ => None,
                    })
                    .expect("the garment made");
                assert_eq!(
                    o.global.appearance.get(table).map(|t| t.entries.len()),
                    Some(1),
                    "race {race}: {template:#x} colored on {table:#x}"
                );
            }

            let gone = h.bots[b].inbox.len();
            h.bots[b].send(
                false,
                queue::LOGON,
                &ClientMessage::LogOff(CharacterRef { character: me }),
            );
            assert!(h.run_until(400, |h| h.bots[b].inbox[gone..]
                .iter()
                .any(|r| matches!(r.message(), Some(ServerMessage::CharacterList(_))))));
            let from = h.bots[b].inbox.len();
            h.enter_world(b, &account, me);
            assert_eq!(drawn(&h, from), made, "race {race}: drawn as made again");
            assert_eq!(held(&h, true), worn, "race {race}: nothing more made");
            let in_pack: Vec<u32> = held(&h, false).iter().map(|x| x.0).collect();
            assert_eq!(in_pack, pack, "race {race}: the same pack");
        }
    }
}

/// Each kingdom's shrine grants fifteen skills, read from the install's changes of kingdom: from
/// 1,000 points to 6,000, rising, each a kingdom skill no player may train itself; the summoning
/// skills come at 2,800 and 4,500 points. The neutral kingdom's grants none.
/// Behaviour: server.kingdoms.kingdom-skills-read
#[test]
fn each_kingdoms_shrine_grants_fifteen_skills_read_from_the_install() {
    use yalain_world::kingdoms::kingdom;
    let p = Portal::open();
    let skills = p.skills();
    let t = p.kingdom_skills();
    assert_eq!(
        t.by_kingdom.keys().copied().collect::<Vec<_>>(),
        [kingdom::SHADOW, kingdom::ORDER, kingdom::DOMINION],
        "the three kingdoms, the neutral one granting none"
    );
    let mut all = std::collections::BTreeSet::new();
    for (k, table) in &t.by_kingdom {
        assert_eq!(table.len(), 15, "{k}: {table:?}");
        let least: Vec<u32> = table.iter().map(|(l, _)| *l).collect();
        assert_eq!(
            least,
            [
                1000, 1200, 1400, 1600, 1800, 2000, 2400, 2800, 3200, 3600, 4000, 4500, 5000, 5500,
                6000
            ],
            "{k}"
        );
        for (_, skill) in table {
            assert!((17..=61).contains(skill), "{k}: a kingdom skill, {skill}");
            let d = &skills.skills[skill];
            assert_ne!(
                d.flags & yalain_world::skills::flag::NOT_BY_PLAYER,
                0,
                "{skill}"
            );
            all.insert(*skill);
        }
    }
    assert_eq!(all.len(), 45, "no skill granted by two kingdoms");
    assert_eq!(t.earned(kingdom::ORDER, 2_799).len(), 7);
    assert_eq!(
        t.earned(kingdom::ORDER, 2_800)[7],
        39,
        "the Order's Envoy at 2,800"
    );
    assert_eq!(
        t.earned(kingdom::DOMINION, 4_500)[11],
        30,
        "the Dominion's Summit at 4,500"
    );
    assert_eq!(t.earned(kingdom::SHADOW, 6_000).len(), 15);
    assert!(t.earned(kingdom::SHADOW, 500).is_empty(), "nothing at 500");
}

/// The character generation data's physique table gives, per race and sex, the appearance each
/// physique type changes and its modifier: 67 types of ten races and sexes, build and height
/// naming none, in the order of the types; the test rules hold the same, row for row.
/// Behaviour: server.characters.physique-from-the-install
#[test]
fn the_install_names_the_appearance_each_physique_type_changes() {
    let read = Portal::open().physique();
    assert_eq!(read.len(), 10);
    assert_eq!(read.values().map(Vec::len).sum::<usize>(), 67);
    assert_eq!(read, yalain_testkit::test_physique());
}

/// The character generation data's race and sex table names each race and sex's body entity: ten,
/// the drudges' one for both sexes; the test rules hold the same, and so did the table the server
/// once kept, entry for entry.
/// Behaviour: server.characters.body-entities-from-the-install
#[test]
fn the_install_names_each_race_and_sexs_body_entity() {
    use yalain_world::chargen::{FEMALE, MALE};
    const KEPT: [(u32, u32, u32); 10] = [
        (1, MALE, 0x4700_00CD),
        (1, FEMALE, 0x4700_00CE),
        (2, MALE, 0x4700_00CF),
        (2, FEMALE, 0x4700_00D1),
        (4, MALE, 0x4700_00D0),
        (4, FEMALE, 0x4700_00D2),
        (8, MALE, 0x4700_0514),
        (8, FEMALE, 0x4700_0514),
        (16, MALE, 0x4700_0512),
        (16, FEMALE, 0x4700_0513),
    ];
    let read = Portal::open().body_entities();
    let kept: std::collections::BTreeMap<(u32, u32), u32> =
        KEPT.iter().map(|(r, s, e)| ((*r, *s), *e)).collect();
    assert_eq!(read, kept);
    let test: std::collections::BTreeMap<(u32, u32), u32> = yalain_testkit::BODY_ENTITIES
        .iter()
        .map(|(r, s, e)| ((*r, *s), *e))
        .collect();
    assert_eq!(read, test);
}

/// The knives are the install's templates of the butchery tools' class: the hunting knife and the
/// crude one, each butchering with the behavior the server plays.
/// Behaviour: server.craft.butchery-knives-from-the-install
#[test]
fn the_knives_are_the_butchery_tools_class_hunting_and_crude() {
    let p = Portal::open();
    let knives = p.templates_of_class(yalain_world::butchery::TOOL_CLASS);
    assert_eq!(knives, [0x4700_0CBB, 0x4700_0CBD]);
    for k in knives {
        assert_eq!(
            p.template(k).props.word("Usage_UserAnimation"),
            Some(yalain_world::butchery::BEHAVIOR)
        );
    }
}

/// Every attribute skill reads its measure of its attribute as the data holds it: health 5 a point
/// (3 for a few), vigor 3 (6 for one), focus 20, Arcane Lore and Heroic Mastery 1, the damage
/// attribute 1 or a two-hundredth; none reads nothing or a number out of all measure.
/// Behaviour: server.skills.attribute-measure-read
#[test]
fn every_attribute_skill_reads_its_measure() {
    let t = Portal::open().skills();
    let attributes: Vec<_> = t.skills.values().filter(|d| d.category != 0).collect();
    assert_eq!(attributes.len(), 131, "the attribute skills");
    for (skill, category, measure) in [
        (1099, 1, 5.0),
        (1117, 1, 3.0),
        (1106, 2, 3.0),
        (1158, 2, 6.0),
        (1114, 3, 0.005),
        (1154, 3, 0.005),
        (1184, 4, 20.0),
        (1109, 5, 1.0),
        (2128, 6, 1.0),
    ] {
        let d = &t.skills[&skill];
        assert_eq!(d.category, category, "{skill}");
        assert!(
            (d.attribute_mod - measure).abs() < 1e-6,
            "{skill}: {}",
            d.attribute_mod
        );
    }
    for d in attributes {
        assert!(
            d.attribute_mod > 0.0 && d.attribute_mod <= 20.0,
            "{}: {}",
            d.id,
            d.attribute_mod
        );
    }
}

/// A hero's attribute skills raise what they rate by their rating times their measure: ten points of
/// a health attribute raise its greatest health by 50, ten of vigor its greatest vigor by 30, four
/// of focus its greatest focus by 80, and seventeen of Arcane Lore its Lore to 17.
/// Behaviour: server.skills.attribute-skills-raise-their-attribute
#[test]
fn a_heros_attribute_skills_raise_their_vitals_and_lore() {
    use yalain_world::effects::own_greatest;
    use yalain_world::player::quality;
    use yalain_world::state::SkillState;
    let mut h = yalain_testkit::Harness::new();
    h.server.skill_table = Portal::open().skills();
    let (_, _) = h.player_in_world("one", "Elara");
    let world = *h.server.players.keys().next().expect("in the world");
    let read = |h: &yalain_testkit::Harness| {
        let p = &h.server.players[&world];
        (
            own_greatest(&h.server, p, quality::HEALTH_MAX),
            own_greatest(&h.server, p, quality::VIGOR_MAX),
            own_greatest(&h.server, p, quality::FOCUS_MAX),
            yalain_world::equip::arcane_lore_of(&h.server, p),
        )
    };
    let before = read(&h);
    let p = h.server.players.get_mut(&world).expect("the player");
    for (skill, granted) in [(1099, 10), (1106, 10), (1184, 4), (1109, 17)] {
        p.play.skills.push(SkillState {
            skill,
            granted,
            mask: yalain_world::skills::mask::TRAINED,
            cost: 0,
            children: 0,
            dependents: 0,
            spent: 0,
        });
    }
    let after = read(&h);
    assert_eq!(after.0 - before.0, 50, "health: {before:?} {after:?}");
    assert_eq!(after.1 - before.1, 30, "vigor");
    assert_eq!(after.2 - before.2, 80, "focus");
    assert_eq!((before.3, after.3), (0, 17), "Arcane Lore");
}

/// Every skill the 2017 servers toggled carries the data's toggle flag, Lumbering Might, Brimming
/// Heart and Bolster Metabolism among them with no aura to turn on; Iron Pincers, a skill only used,
/// carries none. Those three's effects file their records as ordinary ones (categories 1, as 2017
/// told them).
/// Behaviour: server.skills.toggle-flag-read
#[test]
fn the_toggle_flag_reads_on_every_skill_retail_toggled_and_not_on_one_only_used() {
    use yalain_world::effect_defs::Kind;
    use yalain_world::skills::flag;
    const TOGGLED: [u32; 26] = [
        68, 77, 78, 86, 99, 103, 105, 107, 114, 145, 157, 235, 289, 290, 291, 341, 357, 363, 640,
        646, 666, 670, 686, 696, 697, 1028,
    ];
    const NO_AURA: [u32; 3] = [341, 697, 1028];
    const IRON_PINCERS: u32 = 695;
    let p = Portal::open();
    let t = p.skills();
    for id in TOGGLED {
        assert_ne!(t.skills[&id].flags & flag::TOGGLE, 0, "{id}");
    }
    assert_eq!(t.skills[&IRON_PINCERS].flags & flag::TOGGLE, 0);
    let user: Vec<u32> = NO_AURA
        .iter()
        .flat_map(|id| t.skills[id].usage.clone().expect("used").user_effects)
        .collect();
    assert_eq!(user.len(), 4);
    let e = p.effects(&user);
    for st in user {
        let d = e.get(st).unwrap_or_else(|| panic!("{st:08X} reads"));
        assert!(!matches!(d.kind, Kind::Aura(_)), "{st:08X}");
        assert_eq!(d.categories(), 1, "{st:08X}");
    }
}

/// Every template of the install with a class, and its properties: its own, then what its class
/// or one of its bases says where its own say nothing.
fn every_item(p: &Portal) -> Vec<(u32, Props, Props)> {
    let lib = p.library();
    let entity = |id: u32| auberean_assets::decode(id, &p.dat.read(id).ok()?, &p.ctx).ok();
    let name = |q: u32| {
        p.ctx
            .properties()
            .and_then(|l| l.names.name(q))
            .map(str::to_string)
    };
    let mut by_class = std::collections::HashMap::new();
    let ids: Vec<u32> = p
        .dat
        .entries_in(0x4700_0000, 0x47FF_FFFF)
        .map(|e| e.id)
        .collect();
    ids.into_iter()
        .filter_map(|t| {
            let class = Props::class_of(t, &entity)?;
            let said = by_class
                .entry(class)
                .or_insert_with(|| yalain_world_db::install::class_properties(&lib, class));
            let own = Props::of(t, &entity, &name);
            let read = own.clone().with_class(said);
            Some((t, own, read))
        })
        .collect()
}

/// Items whose class binds them to whoever holds them read as attunable: a relic through its
/// class's base, a medallion and a glyph by their own classes; a starting sword is not.
/// Behaviour: server.content.attunable-of-classes
#[test]
fn an_item_whose_class_binds_it_reads_attunable() {
    let p = Portal::open();
    for t in [
        0x4700_11C8,
        0x4700_187E,
        0x4700_08CF,
        0x4700_132D,
        0x4700_1AED,
    ] {
        assert!(p.template(t).props.flag("Item_IsAttunable"), "{t:#x}");
    }
    assert!(!p.template(0x4700_051C).props.flag("Item_IsAttunable"));
}

/// The table the server once kept of the templates bound by their classes, against the install:
/// every one of its 118 reads attunable, and of every other template the read binds only the two
/// training halls' glyphs, whose classes bind them too.
/// Behaviour: server.content.attunable-of-classes
#[test]
fn the_kept_attunable_table_is_the_classes_but_for_the_training_halls_glyphs() {
    const KEPT: [u32; 118] = [
        0x4700_0605,
        0x4700_0606,
        0x4700_0608,
        0x4700_0609,
        0x4700_060A,
        0x4700_060B,
        0x4700_060C,
        0x4700_0615,
        0x4700_0616,
        0x4700_0617,
        0x4700_0618,
        0x4700_0619,
        0x4700_061E,
        0x4700_0620,
        0x4700_0621,
        0x4700_0675,
        0x4700_080B,
        0x4700_0895,
        0x4700_0896,
        0x4700_0897,
        0x4700_0898,
        0x4700_08CF,
        0x4700_08D0,
        0x4700_08D1,
        0x4700_0AE9,
        0x4700_0AEA,
        0x4700_0AEB,
        0x4700_0AEC,
        0x4700_0AED,
        0x4700_0AEE,
        0x4700_11C7,
        0x4700_11C8,
        0x4700_11C9,
        0x4700_11CA,
        0x4700_11CB,
        0x4700_11CC,
        0x4700_11CD,
        0x4700_11CE,
        0x4700_11D1,
        0x4700_11D4,
        0x4700_11D5,
        0x4700_11D6,
        0x4700_11D7,
        0x4700_11D8,
        0x4700_11D9,
        0x4700_11DA,
        0x4700_11DB,
        0x4700_11DC,
        0x4700_132D,
        0x4700_1330,
        0x4700_1331,
        0x4700_1332,
        0x4700_1333,
        0x4700_1334,
        0x4700_1335,
        0x4700_1345,
        0x4700_1346,
        0x4700_13F5,
        0x4700_1469,
        0x4700_146A,
        0x4700_146B,
        0x4700_146C,
        0x4700_14CE,
        0x4700_14CF,
        0x4700_14D0,
        0x4700_14D1,
        0x4700_14D2,
        0x4700_14D3,
        0x4700_14D4,
        0x4700_14D5,
        0x4700_14D7,
        0x4700_14D8,
        0x4700_14DB,
        0x4700_14DC,
        0x4700_14DD,
        0x4700_14DE,
        0x4700_14DF,
        0x4700_14E0,
        0x4700_14E1,
        0x4700_14E2,
        0x4700_14E3,
        0x4700_16C0,
        0x4700_16C1,
        0x4700_16C2,
        0x4700_1863,
        0x4700_1864,
        0x4700_1865,
        0x4700_1866,
        0x4700_1868,
        0x4700_1869,
        0x4700_186A,
        0x4700_186B,
        0x4700_186C,
        0x4700_186E,
        0x4700_1870,
        0x4700_1871,
        0x4700_1872,
        0x4700_1873,
        0x4700_1874,
        0x4700_1875,
        0x4700_1876,
        0x4700_1877,
        0x4700_1878,
        0x4700_187B,
        0x4700_187C,
        0x4700_187D,
        0x4700_187E,
        0x4700_188C,
        0x4700_1A53,
        0x4700_1A54,
        0x4700_1A55,
        0x4700_1AF1,
        0x4700_1CE4,
        0x4700_1CE5,
        0x4700_1CE6,
        0x4700_2056,
        0x4700_2057,
        0x4700_2058,
    ];
    let p = Portal::open();
    let items = every_item(&p);
    let mut kept = 0;
    let mut more = BTreeSet::new();
    let mut fewer = BTreeSet::new();
    for (t, own, read) in &items {
        let was = KEPT.contains(t) || own.flag("Item_IsAttunable");
        let is = read.flag("Item_IsAttunable");
        match (was, is) {
            (true, true) if KEPT.contains(t) => kept += 1,
            (false, true) => {
                more.insert(*t);
            }
            (true, false) => {
                fewer.insert(*t);
            }
            _ => {}
        }
    }
    assert!(items.len() > 9_000, "{}", items.len());
    assert_eq!(kept, 118);
    assert_eq!(fewer, BTreeSet::new());
    assert_eq!(more, BTreeSet::from([0x4700_1AEC, 0x4700_1AED]));
}

/// A glyph carries the key its class gives it: Esper's, Gevoth's and Cragstone's the keys of their
/// vaults.
/// Behaviour: server.content.glyph-keys-of-classes
#[test]
fn a_glyph_carries_the_key_its_class_gives_it() {
    let p = Portal::open();
    for (t, key) in [
        (0x4700_10B1, 0x4000_0021),
        (0x4700_10B3, 0x4000_0023),
        (0x4700_10AC, 0x4000_0004),
    ] {
        assert_eq!(p.template(t).props.word("Lock_KeyID"), Some(key), "{t:#x}");
    }
}

/// The table the server once kept of the key each glyph carries, against the install: its 22
/// glyphs carry the keys their classes give them, and no other item carries one of those keys; the
/// training halls' two glyphs carry a key of their own.
/// Behaviour: server.content.glyph-keys-of-classes
#[test]
fn the_kept_glyph_keys_are_the_classes() {
    const KEPT: [(u32, u32); 22] = [
        (0x4700_10AC, 0x4000_0004),
        (0x4700_10AD, 0x4000_002B),
        (0x4700_10AE, 0x4000_002D),
        (0x4700_10AF, 0x4000_0020),
        (0x4700_10B0, 0x4000_001F),
        (0x4700_10B1, 0x4000_0021),
        (0x4700_10B2, 0x4000_001C),
        (0x4700_10B3, 0x4000_0023),
        (0x4700_10B4, 0x4000_0029),
        (0x4700_10B5, 0x4000_002E),
        (0x4700_10B6, 0x4000_002C),
        (0x4700_10B7, 0x4000_0022),
        (0x4700_10B8, 0x4000_0028),
        (0x4700_10B9, 0x4000_001B),
        (0x4700_10BA, 0x4000_0025),
        (0x4700_10BB, 0x4000_001A),
        (0x4700_10BC, 0x4000_001E),
        (0x4700_10BD, 0x4000_0027),
        (0x4700_10BE, 0x4000_0026),
        (0x4700_10BF, 0x4000_001D),
        (0x4700_10C0, 0x4000_002A),
        (0x4700_10C1, 0x4000_0024),
    ];
    let p = Portal::open();
    let items = every_item(&p);
    let key = |t: u32| {
        items
            .iter()
            .find(|(i, _, _)| *i == t)
            .and_then(|(_, _, read)| read.word("Lock_KeyID"))
    };
    for (t, k) in KEPT {
        assert_eq!(key(t), Some(k), "{t:#x}");
    }
    let opening: BTreeSet<u32> = items
        .iter()
        .filter(|(_, _, read)| {
            read.word("Lock_KeyID")
                .is_some_and(|k| KEPT.iter().any(|g| g.1 == k))
        })
        .map(|(t, _, _)| *t)
        .collect();
    assert_eq!(opening, KEPT.iter().map(|g| g.0).collect());
    assert_eq!(key(0x4700_1AEC), Some(0x4000_004A));
    assert_eq!(key(0x4700_1AED), Some(0x4000_004A));
}

/// A vault door's key carries the key its class gives it, and a door asks that of it: Ariaki's,
/// Citadel's and Cobalt's.
/// Behaviour: server.content.vault-keys-of-classes
#[test]
fn a_vault_doors_key_carries_the_key_its_class_gives_it() {
    let p = Portal::open();
    let mut h = yalain_testkit::Harness::new();
    for (t, key) in [
        (0x4700_1126, 0x4000_0032),
        (0x4700_1127, 0x4000_0034),
        (0x4700_1128, 0x4000_0033),
    ] {
        h.server.catalog.insert(p.template(t));
        assert_eq!(
            yalain_world::usable::doors::key_id(&h.server, t),
            Some(key),
            "{t:#x}"
        );
    }
}

/// The band the server once kept of the vault doors' keys, against the install: 16 of its 18 keys
/// carry what their classes give them; Citadel's and Cobalt's classes give each other's, as their
/// doors asked of them in 2017.
/// Behaviour: server.content.vault-keys-of-classes
#[test]
fn the_kept_vault_key_band_is_the_classes_but_for_citadels_and_cobalts() {
    const KEPT: (u32, u32, u32) = (0x4700_1126, 0x4700_1137, 0x4000_0032);
    let p = Portal::open();
    let items = every_item(&p);
    let key = |t: u32| {
        items
            .iter()
            .find(|(i, _, _)| *i == t)
            .and_then(|(_, _, read)| read.word("Lock_KeyID"))
    };
    let mut same = 0;
    let mut swapped = Vec::new();
    for t in KEPT.0..=KEPT.1 {
        let kept = KEPT.2 + (t - KEPT.0);
        match key(t) {
            Some(k) if k == kept => same += 1,
            Some(k) => swapped.push((t, kept, k)),
            None => panic!("{t:#x} carries no key"),
        }
    }
    assert_eq!(same, 16);
    assert_eq!(
        swapped,
        [
            (0x4700_1127, 0x4000_0033, 0x4000_0034),
            (0x4700_1128, 0x4000_0034, 0x4000_0033)
        ]
    );
}

/// The table the server once kept of the classes whose items their use destroys, against the
/// install: of every template, whether its use spends it reads the same from its own data and its
/// class's, but for three keys whose own data says their use doesn't, which the table's kind
/// overrode; the forager's food, which its class destroys on use, wears instead, as the table
/// held.
/// Behaviour: server.content.destroyed-on-use-of-classes
#[test]
fn the_kept_destroyed_on_use_kinds_are_the_classes_but_for_three_keys() {
    const KEPT: [(&str, bool); 15] = [
        ("KeyTemplate", true),
        ("GlyphTemplate", true),
        ("PotionTemplate", true),
        ("PotionTutorialTemplate", true),
        ("SaddleTemplate", true),
        ("Totem", true),
        ("CharmRatSmasher", true),
        ("ElixirBeetleSlayer", true),
        ("ElixirWaspBane", true),
        ("SealAcademy", true),
        ("MedallionOpening", true),
        ("Coin", false),
        ("HeartTyrantLiving", false),
        ("HeroSkillCreditToken", false),
        ("FoodForaged", false),
    ];
    use yalain_testkit::FOOD;
    let rows = yalain_testkit::test_item_rows();
    let spends = |t: u32, p: &yalain_content::Props| yalain_world::item_use::spends(&rows, t, p);
    let p = Portal::open();
    let lib = p.library();
    let entity = |id: u32| auberean_assets::decode(id, &p.dat.read(id).ok()?, &p.ctx).ok();
    let kinds: Vec<(u32, bool)> = KEPT
        .iter()
        .filter_map(|(n, on)| Some((lib.package_id(n)?, *on)))
        .collect();
    assert_eq!(kinds.len(), 15);
    // The nearest of the kinds a class derives from says.
    let by_kind = |class: u32| {
        let of: Vec<(u32, bool)> = kinds
            .iter()
            .copied()
            .filter(|(k, _)| lib.derives_from(class, *k))
            .collect();
        of.iter()
            .find(|(k, _)| of.iter().all(|(o, _)| lib.derives_from(*k, *o)))
            .is_some_and(|(_, on)| *on)
    };
    let items = every_item(&p);
    let mut same = 0;
    let mut differ = BTreeSet::new();
    for (t, own, read) in &items {
        let Some(class) = Props::class_of(*t, &entity) else {
            continue;
        };
        let was = own.flag("Item_IsDestroyOnUse") || by_kind(class);
        if was == spends(*t, read) {
            same += 1;
        } else {
            assert!(was, "{t:#x} spent now, not before");
            differ.insert(*t);
        }
    }
    assert_eq!(same, items.len() - 3);
    assert_eq!(
        differ,
        BTreeSet::from([0x4700_1106, 0x4700_166B, 0x4700_166C])
    );
    // The wear rule changes the food alone: no other template both wears and is destroyed on use.
    let worn: BTreeSet<u32> = items
        .iter()
        .filter(|(t, _, read)| {
            read.flag("Item_IsDestroyOnUse") && yalain_world::item_use::wears(&rows, *t, read)
        })
        .map(|(t, _, _)| *t)
        .collect();
    assert_eq!(worn, BTreeSet::from([FOOD]));
    let food = items.iter().find(|(t, _, _)| *t == FOOD).expect("the food");
    assert!(food.2.flag("Item_IsDestroyOnUse"), "its class destroys it");
    assert!(!spends(FOOD, &food.2), "it wears");
}

/// Three keys whose own data says their use doesn't destroy them read so, over their class, which
/// says it does: the Study Key and the two Skyport keys. No 2017 recording holds one of them.
/// Behaviour: server.content.destroyed-on-use-of-classes
#[test]
fn keys_whose_own_data_keeps_them_are_not_destroyed_on_use() {
    let rows = yalain_testkit::test_item_rows();
    let spends = |t: u32, p: &yalain_content::Props| yalain_world::item_use::spends(&rows, t, p);
    let p = Portal::open();
    for t in [0x4700_1106, 0x4700_166B, 0x4700_166C] {
        let key = p.template(t);
        assert!(!key.props.flag("Item_IsDestroyOnUse"), "{t:#x}");
        assert!(!spends(t, &key.props), "{t:#x}");
    }
    let potion = p.template(0x4700_11B4);
    assert!(spends(0x4700_11B4, &potion.props));
}

/// The Tesserae's class says its use destroys them on both sides and that it doesn't on the
/// server's own: the server's own start-up comes later and stands.
/// Behaviour: server.content.destroyed-on-use-of-classes
#[test]
fn a_classs_own_server_start_up_stands_over_its_shared_one() {
    let p = Portal::open();
    let lib = p.library();
    let class = lib.package_id("HeroSkillCreditToken").expect("the class");
    let said = yalain_world_db::install::class_properties(&lib, class);
    assert_eq!(
        said.iter()
            .find(|(n, _)| *n == "Item_IsDestroyOnUse")
            .map(|(_, v)| v.clone()),
        Some(yalain_content::Prop::Bool(false))
    );
}

/// What an attack's accuracy and its target's evasion are made of is read from its skill's class:
/// a creature's plain melee strike and a player's melee basic attack rate the melee masteries at
/// 2.4, 4.4 and 6.4 a point, the attack attribute and the level part for accuracy, and the masteries
/// at 2, 4 and 6, the defense attribute and the level part for evasion; missile and magic attacks
/// rate their own; a class's own skills add its adept skill at 2 a point on both sides; a class that
/// adds none takes its base class's.
/// Behaviour: server.skills.rating-parts-read
#[test]
fn an_attacks_rating_parts_are_read_from_its_skills_class() {
    use yalain_world::skills::{Rated, RatingPart};
    let t = Portal::open().skills();
    let sorted = |parts: &[RatingPart]| {
        let mut v: Vec<(Rated, f32)> = parts.iter().map(|p| (p.of, p.weight)).collect();
        v.sort_by_key(|(of, _)| match of {
            Rated::Level => 0,
            Rated::Skill(k) => *k,
        });
        v
    };
    let kinds = [
        // Magic: attack 1, defense 2, grandmastery 3, mastery 4, paragon 5.
        (1840, (1, 2, 3, 4, 5)),
        // Melee: attack 7, defense 8, grandmastery 9, mastery 10, paragon 11.
        (1845, (7, 8, 9, 10, 11)),
        (122, (7, 8, 9, 10, 11)),
        // A class that adds none of its own takes its base's.
        (493, (7, 8, 9, 10, 11)),
        // Missile: attack 12, defense 13, grandmastery 14, mastery 15, paragon 16.
        (1861, (12, 13, 14, 15, 16)),
        (207, (12, 13, 14, 15, 16)),
    ];
    for (skill, (attack, defense, grand, master, paragon)) in kinds {
        let d = &t.skills[&skill];
        assert_eq!(
            sorted(&d.accuracy),
            vec![
                (Rated::Level, 1.0),
                (Rated::Skill(attack), 1.0),
                (Rated::Skill(grand), 4.4),
                (Rated::Skill(master), 2.4),
                (Rated::Skill(paragon), 6.4),
            ],
            "{skill}"
        );
        assert_eq!(
            sorted(&d.evasion),
            vec![
                (Rated::Level, 1.0),
                (Rated::Skill(defense), 1.0),
                (Rated::Skill(grand), 4.0),
                (Rated::Skill(master), 2.0),
                (Rated::Skill(paragon), 6.0),
            ],
            "{skill}"
        );
    }
    // A class's own attack: the adept skill on both sides.
    let adept = t
        .skills
        .values()
        .filter(|d| d.usage.is_some())
        .filter(|d| {
            d.accuracy
                .iter()
                .any(|p| matches!(p.of, Rated::Skill(k) if k > 16) && p.weight == 2.0)
        })
        .count();
    assert!(adept > 400, "class attacks with an adept part: {adept}");
    let rated = t
        .skills
        .values()
        .filter(|d| d.usage.is_some() && !d.accuracy.is_empty())
        .count();
    assert!(rated > 1_400, "used skills with accuracy parts: {rated}");
}

/// A hero's Heroic Mastery adds its rating, a point a point, to the level part of its accuracy and
/// evasion, on top of 8 a level up to level 50; raised by experience, its rating is what its
/// advancement table gives.
/// Behaviour: server.combat.ratings-from-skills
#[test]
fn heroic_mastery_adds_its_rating_to_the_level_part() {
    use yalain_world::ratings::{self, Rater};
    use yalain_world::skills::{Rated, RatingPart};
    let mut h = yalain_testkit::Harness::new();
    h.server.skill_table = Portal::open().skills();
    let (_, _) = h.player_in_world("one", "Elara");
    let world = *h.server.players.keys().next().expect("in the world");
    let level = [RatingPart {
        of: Rated::Level,
        weight: 1.0,
    }];
    let p = h.server.players.get_mut(&world).expect("the player");
    yalain_world::quals::set_int(&mut p.qualities, yalain_world::examine::LEVEL, 60);
    let without = ratings::sum(&h.server, Rater::Player(world), &level);
    let p = h.server.players.get_mut(&world).expect("the player");
    p.play.skills.push(yalain_world::state::SkillState {
        skill: 2128,
        granted: 9,
        mask: yalain_world::skills::mask::TRAINED,
        cost: 0,
        children: 0,
        dependents: 0,
        spent: 0,
    });
    let with = ratings::sum(&h.server, Rater::Player(world), &level);
    assert!((without - 400.0).abs() < 1e-9, "{without}");
    assert!((with - 409.0).abs() < 1e-9, "{with}");
    // Raised by experience, its rating is the level its advancement table gives: a level 85 hero of
    // 2017 held 135,535,620,313 in it, and its accuracy read 400 points over the level part's 400.
    let p = h.server.players.get_mut(&world).expect("the player");
    p.play.skills.retain(|k| k.skill != 2128);
    p.play.skills.push(yalain_world::state::SkillState {
        skill: 2123,
        granted: 0,
        mask: yalain_world::skills::mask::TRAINED,
        cost: 0,
        children: 0,
        dependents: 0,
        spent: 135_535_620_313,
    });
    yalain_world::quals::set_int(&mut p.qualities, yalain_world::examine::LEVEL, 85);
    let raised = ratings::sum(&h.server, Rater::Player(world), &level);
    assert!((raised - 800.0).abs() < 1e-9, "{raised}");
}

/// Every used skill reads the chance its class sets that a landed blow takes the damage pool, and
/// its measure of the pool: the basic attacks 0.1, the specials 0.4, 0.6 or 0.8 by their strength,
/// the rest 0 (a few basic attacks of a class's own 0.4 or 0.6); every measure 1 but Backstab's half
/// and Nibble's none. A skill not used reads none.
/// Behaviour: server.skills.damage-pool-read
#[test]
fn every_used_skill_reads_its_damage_pool_chance_and_measure() {
    let t = Portal::open().skills();
    let of = |k: u32| {
        let d = &t.skills[&k];
        (d.damage_pool_chance, d.damage_pool_modifier)
    };
    // Hum_Me_AutoAttack1, Hum_Me_FindHole, Hum_Me_Backstab, Mn_Me_Nibble.
    assert_eq!(of(122), (Some(0.1), Some(1.0)));
    assert_eq!(of(171), (Some(0.4), Some(1.0)));
    assert_eq!(of(163), (Some(0.6), Some(0.5)));
    assert_eq!(of(2179), (Some(0.0), Some(0.0)));
    let mut by_chance = std::collections::BTreeMap::new();
    for d in t.skills.values() {
        assert_eq!(d.active, d.damage_pool_chance.is_some(), "{}", d.id);
        assert_eq!(d.active, d.damage_pool_modifier.is_some(), "{}", d.id);
        if let Some(c) = d.damage_pool_chance {
            *by_chance
                .entry(dereth_primitives::num::to_i32((c * 10.0).round()))
                .or_insert(0) += 1;
        }
    }
    let n = |c: i32| by_chance.get(&c).copied().unwrap_or(0);
    assert_eq!(
        (n(1), n(4), n(6), n(8)),
        (166, 245, 143, 91),
        "{by_chance:?}"
    );
    assert_eq!(
        by_chance.keys().copied().collect::<Vec<_>>(),
        [0, 1, 4, 6, 8]
    );
}

/// A hero's damage pool is its damage attribute (the rating of its class's damage attribute skill,
/// raised by experience through that skill's own table, times the skill's measure) times the
/// attack's measure, at the attack's chance: a Berserker of 2017 held 706,840,738,672 in it, a pool
/// of 610 for a basic attack at a tenth and 305 for Backstab; a Templar's skill at the same
/// experience rates 460 by its own table and measures 0.005 a point, 2. A player holding none has
/// none.
/// Behaviour: server.combat.damage-pool
#[test]
fn a_heros_damage_attribute_is_its_damage_pool() {
    use yalain_world::combat::damage_pool;
    use yalain_world::rules::DamagePool;
    let mut h = yalain_testkit::Harness::new();
    h.server.skill_table = Portal::open().skills();
    let (_, _) = h.player_in_world("one", "Elara");
    let world = *h.server.players.keys().next().expect("in the world");
    // Hum_Me_AutoAttack1 and Hum_Me_Backstab.
    let (basic, backstab) = (122, 163);
    assert_eq!(damage_pool(&h.server, world, basic).points, 0);
    let hold = |h: &mut yalain_testkit::Harness, skill: u32| {
        let p = h.server.players.get_mut(&world).expect("the player");
        p.play.skills.retain(|k| ![1142, 1516].contains(&k.skill));
        p.play.skills.push(yalain_world::state::SkillState {
            skill,
            granted: 0,
            mask: yalain_world::skills::mask::TRAINED,
            cost: 0,
            children: 0,
            dependents: 0,
            spent: 706_840_738_672,
        });
    };
    // Lug_Me_Berserker_Hero_AttrMajorDamage.
    hold(&mut h, 1142);
    assert_eq!(
        damage_pool(&h.server, world, basic),
        DamagePool {
            points: 610,
            chance: 0.1
        }
    );
    assert_eq!(
        damage_pool(&h.server, world, backstab),
        DamagePool {
            points: 305,
            chance: 0.6
        }
    );
    // Emp_Me_Templar_Hero_AttrDamage.
    hold(&mut h, 1516);
    assert_eq!(damage_pool(&h.server, world, basic).points, 2);
}
