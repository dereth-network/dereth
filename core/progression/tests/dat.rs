//! Builds from both eras' own tables: the end of retail's and February 2005's.
//! Behaviour: none (tooling: the playtest character builder, checked against the retail tables)

use dereth_assets::tables::{Attribute2ndTable, CharGen, SkillTable, XpTable};
use dereth_assets::Decode;
use dereth_dat::{ContainerEra, RetailDatStore};
use dereth_primitives::DataId;
use dereth_progression::archetypes::{builtin, offered, Archetype};
use dereth_progression::{build, check, heritage, Budget, Sac, Tables};

mod extras;

struct Owned {
    xp: XpTable,
    skills: SkillTable,
    chargen: CharGen,
    vitals: Attribute2ndTable,
}

impl Owned {
    fn tables(&self) -> Tables<'_> {
        Tables {
            xp: &self.xp,
            skills: &self.skills,
            chargen: &self.chargen,
            vitals: &self.vitals,
        }
    }
}

fn era(classic: bool) -> Owned {
    let (store, era) = if classic {
        (
            dereth_dat::testing::open_classic_store_or_fail(),
            ContainerEra::Classic,
        )
    } else {
        (
            RetailDatStore::open_dir(&dereth_dat::testing::dat_dir()).unwrap(),
            ContainerEra::Modern,
        )
    };
    let read = |id: u32| store.read_portal(DataId(id)).unwrap();
    Owned {
        xp: XpTable::decode_payload_in(era, DataId(0x0E00_0018), &read(0x0E00_0018)).unwrap(),
        skills: SkillTable::decode_payload_in(era, DataId(0x0E00_0004), &read(0x0E00_0004))
            .unwrap(),
        chargen: CharGen::decode_payload_in(era, DataId(0x0E00_0002), &read(0x0E00_0002)).unwrap(),
        vitals: Attribute2ndTable::decode_payload_in(era, DataId(0x0E00_0003), &read(0x0E00_0003))
            .unwrap(),
    }
}

#[test]
fn a_level_brings_the_end_of_retail_credits_and_experience_the_files_list() {
    let owned = era(false);
    let t = owned.tables();
    assert_eq!(t.max_level(), 275);
    let at = |level| Budget::of(&t, level, heritage::ALUVIAN).unwrap();
    assert_eq!(at(1).credits(), 52);
    assert_eq!(at(80).credits(), 80, "52 at creation and 28 from levels");
    assert_eq!(at(126).credits(), 89);
    assert_eq!(at(275).credits(), 98, "46 credits from levels");
    assert_eq!(at(126).experience, 4_286_609_098);
    assert_eq!(at(275).experience, 191_226_310_247);
    assert!(Budget::of(&t, 276, heritage::ALUVIAN).is_err());
    assert!(Budget::of(&t, 0, heritage::ALUVIAN).is_err());
}

#[test]
fn february_2005_caps_at_126_with_37_credits_from_levels_and_no_heavy_weapons() {
    let owned = era(true);
    let t = owned.tables();
    assert_eq!(t.max_level(), 126);
    let b = Budget::of(&t, 126, heritage::ALUVIAN).unwrap();
    assert_eq!(
        b.level_credits, 37,
        "one at each of 37 levels from 2 to 125"
    );
    let ids: Vec<String> = offered(&t).into_iter().map(|a| a.id).collect();
    assert_eq!(ids, ["melee-heavy", "archer", "war-mage"]);
    let melee = Archetype::named("melee-heavy").unwrap();
    assert_eq!(t.skill_name(melee.main_skill(&t).unwrap()), "Sword");
}

#[test]
fn every_archetype_reconciles_at_each_level_in_both_eras() {
    for classic in [false, true] {
        let owned = era(classic);
        let t = owned.tables();
        for a in offered(&t) {
            for level in [1, 20, 80, 126, 200, 275] {
                if level > t.max_level() {
                    continue;
                }
                let b = build(&t, level, heritage::ALUVIAN, &a.plan)
                    .unwrap_or_else(|e| panic!("{} at {level}: {e}", a.id));
                let findings = check(&t, &b.sheet());
                assert!(
                    findings.is_empty(),
                    "{} at {level} (classic {classic}): {findings:?}",
                    a.id
                );
                let main = a.main_skill(&t).unwrap();
                let s = b.skills.iter().find(|s| s.id == main).unwrap();
                assert_eq!(s.sac, Sac::Specialized, "{} at {level}", a.id);
                assert_eq!(
                    b.spent_xp() - b.creation_xp + b.unspent_xp,
                    b.budget.experience,
                    "nothing is granted beyond the budget and creation's free ranks"
                );
            }
        }
    }
}

#[test]
fn a_level_80_melee_specializes_its_weapon_and_defense_and_spends_nearly_everything() {
    let owned = era(false);
    let t = owned.tables();
    let melee = Archetype::named("melee-heavy").unwrap();
    let b = build(&t, 80, heritage::ALUVIAN, &melee.plan).unwrap();
    let heavy = t.skill_named("Heavy Weapons").unwrap();
    let defense = t.skill_named("Melee Defense").unwrap();
    for (id, name) in [(heavy, "Heavy Weapons"), (defense, "Melee Defense")] {
        let s = b.skills.iter().find(|s| s.id == id).unwrap();
        assert_eq!((s.sac, s.start), (Sac::Specialized, 10), "{name}");
    }
    assert!(
        b.unspent_credits >= 0 && b.unspent_credits < 6,
        "{}",
        b.unspent_credits
    );
    // What is left is less than any raise would cost: the curves' first steps are tiny.
    assert!(b.unspent_xp < b.budget.experience / 50, "{}", b.unspent_xp);
    let weapon = b.skill(&t, heavy);
    let attributes = b.attributes.map(|a| a.innate + a.ranks);
    println!(
        "Heavy Weapons {weapon}, Melee Defense {}, attributes {attributes:?}, vitals {:?}",
        b.skill(&t, defense),
        b.vitals.map(|v| v.ranks)
    );
    // Level 70 to 90 heavy weapon users at the end of retail had a base Heavy Weapons of 310 to
    // 358, and every attribute raised; those who had spent all their experience, about 350.
    assert!((330..=375).contains(&weapon), "{weapon}");
    assert!(
        attributes.iter().all(|a| (110..=260).contains(a)),
        "{attributes:?}"
    );
}

#[test]
fn ranks_rise_in_their_weights_shares_and_salvaging_is_never_raised() {
    let owned = era(false);
    let t = owned.tables();
    let melee = Archetype::named("melee-heavy").unwrap();
    let ranks = |b: &dereth_progression::Build, name: &str| {
        let id = t.skill_named(name).unwrap();
        b.skills.iter().find(|s| s.id == id).unwrap().ranks
    };
    for level in [60, 110, 150] {
        let b = build(&t, level, heritage::ALUVIAN, &melee.plan).unwrap();
        let main = f64::from(ranks(&b, "Heavy Weapons"));
        // Melee Defense's weight is 89 to the weapon's 100, Strength's 76.
        let defense = f64::from(ranks(&b, "Melee Defense")) / main;
        let strength = f64::from(b.attributes[0].ranks) / main;
        assert!((0.84..=0.94).contains(&defense), "{level}: {defense}");
        assert!((0.71..=0.81).contains(&strength), "{level}: {strength}");
        assert_eq!(
            ranks(&b, "Salvaging"),
            5,
            "{level}: creation's free ranks only"
        );
    }
}

#[test]
fn every_archetype_reconciles_for_every_heritage_in_both_eras() {
    for classic in [false, true] {
        let owned = era(classic);
        let t = owned.tables();
        let heritages = t.heritages();
        assert!(heritages.iter().all(|(id, _)| *id < heritage::OLTHOI));
        assert_eq!(heritages.len(), if classic { 3 } else { 11 });
        for a in offered(&t) {
            for (h, name) in &heritages {
                for level in [1, 80, t.max_level()] {
                    let b = build(&t, level, *h, &a.plan)
                        .unwrap_or_else(|e| panic!("{} {name} at {level}: {e}", a.id));
                    let findings = check(&t, &b.sheet());
                    assert!(
                        findings.is_empty(),
                        "{} {name} at {level}: {findings:?}",
                        a.id
                    );
                }
            }
        }
        assert_eq!(t.heritage_named("gharundim"), Some(heritage::GHARUNDIM));
        assert_eq!(t.heritage_named("Olthoi"), None);
    }
}

#[test]
fn creation_gives_each_skill_it_trains_five_free_ranks_and_the_checks_count_them_exactly() {
    for classic in [false, true] {
        let owned = era(classic);
        assert_eq!(
            owned.tables().xp.trained_xp[5],
            526,
            "the free ranks' experience, as the server's creation gives it"
        );
    }
    let owned = era(false);
    let t = owned.tables();
    let melee = Archetype::named("melee-heavy").unwrap();
    let b = build(&t, 80, heritage::ALUVIAN, &melee.plan).unwrap();
    let created: Vec<&str> = b
        .skills
        .iter()
        .filter(|s| s.creation)
        .map(|s| s.name.as_str())
        .collect();
    // What costs nothing, then the plan's choices while 52 credits pay for them: Heavy Weapons
    // and Melee Defense specialized (32, no free ranks), Healing, Item Enchantment and Mana
    // Conversion trained (20); Creature Enchantment and the rest came with later levels.
    let mut want = vec![
        "Arcane Lore",
        "Healing",
        "Item Enchantment",
        "Jump",
        "Loyalty",
        "Magic Defense",
        "Mana Conversion",
        "Run",
        "Salvaging",
    ];
    want.sort_unstable();
    let mut got = created.clone();
    got.sort_unstable();
    assert_eq!(got, want);
    let salvaging = b.skills.iter().find(|s| s.name == "Salvaging").unwrap();
    assert_eq!(
        (salvaging.ranks, salvaging.xp),
        (5, 526),
        "no share, its free ranks only"
    );
    assert_eq!(b.creation_xp, 526 * 9);
    assert_eq!(
        b.spent_xp() - b.creation_xp + b.unspent_xp,
        b.budget.experience
    );
    assert!(check(&t, &b.sheet()).is_empty());
    // Known creation: exactly its free ranks. A free rank too many is refused.
    let mut s = b.sheet();
    s.creation_xp = s.creation_xp.map(|x| x + 526);
    assert!(!check(&t, &s).is_empty());
    // A character as stored, its creation unknown: a whole number of free ranks, as `verify-xp`
    // reads it, and nothing else.
    let mut s = b.sheet();
    s.creation_xp = None;
    assert!(check(&t, &s).is_empty());
    s.available_xp -= 1;
    assert!(!check(&t, &s).is_empty());
}

#[test]
fn a_skill_chosen_again_moves_to_the_end_of_its_list_and_the_credits_follow() {
    let owned = era(false);
    let t = owned.tables();
    let mut plan = Archetype::named("melee-heavy").unwrap().plan;
    let light = t.skill_named("Light Weapons").unwrap();
    let healing = t.skill_named("Healing").unwrap();
    let before = build(&t, 80, heritage::ALUVIAN, &plan).unwrap();
    assert_eq!(plan.wants(&t, light), Sac::Untrained);
    plan.choose(&t, light, Sac::Trained);
    assert_eq!(
        plan.trained.last().map(String::as_str),
        Some("LightWeapons")
    );
    assert_eq!(plan.wants(&t, light), Sac::Trained);
    plan.choose(&t, healing, Sac::Untrained);
    assert_eq!(plan.wants(&t, healing), Sac::Untrained);
    let after = build(&t, 80, heritage::ALUVIAN, &plan).unwrap();
    let class =
        |b: &dereth_progression::Build, id| b.skills.iter().find(|s| s.id == id).unwrap().sac;
    assert_eq!(class(&after, light), Sac::Trained);
    assert_eq!(class(&after, healing), Sac::Untrained);
    // Healing's credits back pay for Missile Defense too, which did not fit before.
    let missile = t.skill_named("Missile Defense").unwrap();
    assert_eq!(class(&before, missile), Sac::Untrained);
    assert_eq!(class(&after, missile), Sac::Trained);
    assert_eq!((before.unspent_credits, after.unspent_credits), (4, 0));
    assert!(check(&t, &after.sheet()).is_empty());
}

#[test]
fn a_build_that_spends_more_than_it_earned_or_lacks_its_free_augmentation_is_refused() {
    let owned = era(false);
    let t = owned.tables();
    let melee = Archetype::named("melee-heavy").unwrap();
    let b = build(&t, 80, heritage::ALUVIAN, &melee.plan).unwrap();
    let mut s = b.sheet();
    s.attributes[0].2 += 1;
    s.attributes[0].3 = t.xp.attribute_xp[s.attributes[0].2 as usize];
    assert!(!check(&t, &s).is_empty(), "more spent than earned");
    let mut s = b.sheet();
    s.available_credits += 1;
    assert!(!check(&t, &s).is_empty(), "a credit from nowhere");
    let mut s = b.sheet();
    s.heritage_augmentation = Some(0);
    assert!(!check(&t, &s).is_empty());
    let mut s = b.sheet();
    s.attributes[3].1 += 5;
    assert!(!check(&t, &s).is_empty(), "335 innate points");
    let mut s = b.sheet();
    s.level = 81;
    assert!(!check(&t, &s).is_empty(), "level 80's experience at 81");
    let mut plan = melee.plan.clone();
    plan.innate[0] = 101;
    plan.innate[5] = 9;
    assert!(build(&t, 80, heritage::ALUVIAN, &plan).is_err());
}

#[test]
fn the_same_plan_and_tables_always_give_the_same_build() {
    let owned = era(false);
    let t = owned.tables();
    for a in builtin() {
        let one = build(&t, 150, heritage::SHO, &a.plan).unwrap();
        let two = build(&t, 150, heritage::SHO, &a.plan).unwrap();
        assert_eq!(one, two, "{}", a.id);
    }
}

#[test]
fn a_stored_characters_numbers_give_back_the_base_values_of_the_build_they_were_made_from() {
    for classic in [false, true] {
        let owned = era(classic);
        let t = owned.tables();
        for a in offered(&t) {
            let made = build(&t, 100, heritage::GHARUNDIM, &a.plan).unwrap();
            let read = dereth_progression::Build::of_sheet(&t, &made.sheet()).unwrap();
            for id in dereth_progression::ATTRIBUTES {
                assert_eq!(read.attribute(id), made.attribute(id), "{} {id}", a.id);
            }
            for id in dereth_progression::VITALS {
                assert_eq!(read.vital(&t, id), made.vital(&t, id), "{} {id}", a.id);
            }
            for s in &made.skills {
                assert_eq!(
                    read.skill(&t, s.id),
                    made.skill(&t, s.id),
                    "{} {}",
                    a.id,
                    s.name
                );
            }
            assert_eq!(read.budget, made.budget);
            assert_eq!(check(&t, &read.sheet()), check(&t, &made.sheet()));
        }
        let mut wrong = build(
            &t,
            100,
            heritage::ALUVIAN,
            &Archetype::named("archer").unwrap().plan,
        )
        .unwrap()
        .sheet();
        wrong.level = t.max_level() + 1;
        assert!(dereth_progression::Build::of_sheet(&t, &wrong).is_err());
    }
}
