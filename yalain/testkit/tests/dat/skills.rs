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
    use yalain_world::effect_defs::Kind;
    use yalain_world::pets::{mercenary, Pets};
    let portal = Portal::open();
    let kinds = Pets::default();
    for contract in [mercenary::CRAFTER, mercenary::KEEPER, mercenary::MEDIC] {
        let t = portal.template(contract);
        assert!(yalain_world::quests::acquiring_does_more_than_quests(
            &t.props
        ));
        let pet = portal.template(kinds.kind_of(contract).expect("a kind").template);
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

/// A new character of each of the five races holds the weapons its data names, and no clothes: a
/// Drudge two, the others three.
/// Behaviour: server.characters.starting-pack-from-the-install
#[test]
fn a_new_character_of_each_race_holds_the_weapons_its_data_names() {
    use auberean_protocol::login::{CharacterCreated, CreateCharacter};
    use auberean_protocol::table::{Table, SMALL};
    use auberean_protocol::text::{NarrowString, WideString};
    use auberean_protocol::{queue, ClientMessage, ServerMessage};
    let p = Portal::open();
    let rules = p.start_rules();
    let lib = p.library();
    let mut catalog = yalain_content::Catalog::default();
    for t in rules.items.values().flatten() {
        catalog.insert(p.template(*t));
    }
    catalog.implements =
        yalain_world::skill_use::implements_of_catalog(&catalog, |package, class| {
            lib.package_id(class)
                .is_some_and(|c| lib.derives_from(package, c))
        });
    for (race, want) in [
        (1, vec![0x4700_051B, 0x4700_051C, 0x4700_051D]),
        (2, vec![0x4700_0515, 0x4700_0516, 0x4700_0517]),
        (4, vec![0x4700_0521, 0x4700_0522, 0x4700_0523]),
        (8, vec![0x4700_052C, 0x4700_052D]),
        (16, vec![0x4700_0526, 0x4700_0527, 0x4700_0528]),
    ] {
        let mut h = yalain_testkit::Harness::new();
        let areas = h.server.start.by_race.clone();
        h.server.start = rules.clone();
        h.server.start.by_race = areas;
        h.server.start.areas = yalain_testkit::test_start().areas;
        h.server.catalog = catalog.clone();
        let account = format!("race{race}");
        let b = h.at_character_list(&account);
        let before = h.bots[b].inbox.len();
        let m = ClientMessage::CreateCharacter(CreateCharacter {
            account: NarrowString::from(account.as_str()),
            name: WideString::from("Newcomer"),
            entity_did: 0x4700_110F,
            start_area: 0x4000_0001,
            race,
            sex: 0x1000,
            class: 2,
            implement: 1,
            physique: Table::from_inserted(SMALL, [(1, 1.0), (2, 1.0)]),
        });
        h.bots[b].send(false, queue::LOGON, &m);
        assert!(h.run_until(1000, |h| h.bots[b].inbox.len() > before));
        let Some(ServerMessage::CharacterCreated(CharacterCreated { identity, .. })) =
            h.bots[b].inbox[before].message()
        else {
            panic!("race {race} is not answered with a character");
        };
        assert_ne!(identity.id, 0, "race {race} makes a character");
        h.enter_world(b, &account, identity.id);
        let me = h
            .server
            .players
            .values()
            .find(|p| p.character == identity.id)
            .expect("in the world");
        let held: Vec<u32> = me.play.items.iter().map(|i| i.template).collect();
        assert_eq!(held, want, "race {race}");
    }
}
