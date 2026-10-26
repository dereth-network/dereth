//! Augmentations, luminance and quest credits, against the end of retail's own tables.
//! Behaviour: none (tooling: the playtest character builder's extras, checked against the retail tables)

use std::collections::BTreeMap;

use dereth_progression::augmentations::Costs;
use dereth_progression::extras::{build, Extras, Luminous, Mastery};
use dereth_progression::{check, heritage, Archetype, Sac};

/// What each gem costs in the world database the server ships with.
fn costs() -> Costs {
    Costs::of(|gem, _| {
        let billion = 1_000_000_000;
        Some(match gem {
            g if g.starts_with("gemaugmentationatt") => billion / 2,
            "gemaugmentationbonussalvage"
            | "gemaugmentationcarryingcapacityi"
            | "gemaugmentationcriticaldefense"
            | "gemaugmentationfastregen"
            | "gemaugmentationspellduration" => billion,
            g if g.starts_with("gemaugmentationtinkeringspec") => billion,
            "gemaugmentationbonusxp"
            | "gemaugmentationpackslot"
            | "gemaugmentationdeathspellsremain"
            | "ace41482-eyeoftheremorseless"
            | "ace41481-handoftheremorseless"
            | "ace43167-jackofalltrades" => 4 * billion,
            _ => 2 * billion,
        })
    })
}

fn bought(list: &[(&str, u32)]) -> BTreeMap<String, u32> {
    list.iter().map(|(id, n)| ((*id).to_owned(), *n)).collect()
}

#[test]
fn a_level_275_melee_with_thirty_augmentations_spends_their_experience_and_still_reconciles() {
    let owned = super::era(false);
    let t = owned.tables();
    let melee = Archetype::named("melee-heavy").unwrap().plan;
    let extras = Extras {
        augmentations: bought(&[
            ("reinforcement-of-the-lugians", 0),
            ("clutch-of-the-miser", 3),
            ("enduring-enchantment", 1),
            ("archmages-endurance", 5),
            ("critical-protection", 1),
            ("might-of-the-seventh-mule", 5),
            ("master-of-the-steel-circle", 1),
            ("frenzy-of-the-slayer", 1),
            ("iron-skin-of-the-invincible", 1),
            ("enhancement-of-the-blade-turner", 2),
            ("ciandras-fortune", 4),
            ("innate-renewal", 2),
            ("quick-learner", 1),
            ("hand-of-the-remorseless", 1),
            ("eye-of-the-remorseless", 1),
            ("shadow-of-the-seventh-mule", 1),
        ]),
        ..Extras::default()
    };
    let (b, first) = build(&t, 275, heritage::ALUVIAN, &melee, &extras, &costs()).unwrap();
    assert!(first.iter().all(|f| f.warning), "{first:?}");
    assert_eq!(b.augmentation_xp, 53_000_000_000);
    assert_eq!(
        b.spent_xp() + b.unspent_xp + b.augmentation_xp,
        b.budget.experience + b.creation_xp,
        "the augmentations come out of the budget"
    );
    // Short of the cap, less is left to raise the rest with.
    let (short, _) = build(&t, 230, heritage::ALUVIAN, &melee, &extras, &costs()).unwrap();
    let plain = dereth_progression::build(&t, 230, heritage::ALUVIAN, &melee).unwrap();
    assert!(short.spent_xp() < plain.spent_xp());
    let sheet = b.sheet();
    assert_eq!(sheet.augmentations.get(&326), Some(&1), "the free one");
    assert_eq!(sheet.augmentations.get(&231), Some(&3));
    let findings = check(&t, &sheet);
    assert!(findings.is_empty(), "{findings:?}");
    // Past the band for its level is a warning, never a refusal.
    let three = Extras {
        augmentations: bought(&[("ciandras-fortune", 3)]),
        ..Extras::default()
    };
    let (_, at_150) = build(&t, 150, heritage::ALUVIAN, &melee, &three, &costs()).unwrap();
    assert!(
        !at_150.is_empty() && at_150.iter().all(|f| f.warning),
        "{at_150:?}"
    );
}

/// What the checks find of a build, or why there is none.
fn found(
    r: Result<(dereth_progression::Build, Vec<dereth_progression::Finding>), String>,
) -> Vec<dereth_progression::Finding> {
    match r {
        Ok((_, f)) => f,
        Err(e) => vec![dereth_progression::Finding {
            field: dereth_progression::Field::Augmentation,
            text: e,
            warning: false,
        }],
    }
}

#[test]
fn an_innate_augmentation_adds_five_to_its_attribute_and_never_past_one_hundred() {
    let owned = super::era(false);
    let t = owned.tables();
    // Heavy melee is made with Self 10 and Strength 100.
    let melee = Archetype::named("melee-heavy").unwrap().plan;
    let aug = |id: &str, n| Extras {
        augmentations: bought(&[(id, n)]),
        ..Extras::default()
    };
    let (b, first) = build(
        &t,
        200,
        heritage::ALUVIAN,
        &melee,
        &aug("steadfast-will", 2),
        &costs(),
    )
    .unwrap();
    assert!(first.iter().all(|f| f.warning), "{first:?}");
    assert_eq!(b.attributes[5].innate, melee.innate[5] + 10);
    assert!(check(&t, &b.sheet()).is_empty());
    // The same innate value without the augmentation is no creation.
    let mut forged = b.sheet();
    forged.augmentations.remove(&223);
    assert!(!check(&t, &forged).is_empty());
    let past = found(build(
        &t,
        200,
        heritage::ALUVIAN,
        &melee,
        &aug("reinforcement-of-the-lugians", 1),
        &costs(),
    ));
    assert!(past.iter().any(|f| !f.warning), "{past:?}");
}

#[test]
fn a_tinkering_augmentation_specializes_the_skill_it_trains_for_its_trained_credits() {
    let owned = super::era(false);
    let t = owned.tables();
    let melee = Archetype::named("melee-heavy").unwrap().plan;
    let armor = t.skill_named("Armor Tinkering").unwrap();
    let extras = Extras {
        augmentations: bought(&[("jibrils-essence", 1)]),
        ..Extras::default()
    };
    let (b, first) = build(&t, 250, heritage::ALUVIAN, &melee, &extras, &costs()).unwrap();
    assert!(first.iter().all(|f| f.warning), "{first:?}");
    let s = b.skills.iter().find(|s| s.id == armor).unwrap();
    assert_eq!((s.sac, s.start), (Sac::Specialized, 10));
    assert!(
        check(&t, &b.sheet()).is_empty(),
        "{:?}",
        check(&t, &b.sheet())
    );
    let mut forged = b.sheet();
    forged.augmentations.remove(&226);
    assert!(
        !check(&t, &forged).is_empty(),
        "specialized without its augmentation"
    );
}

#[test]
fn quest_credits_add_their_credits_and_luminance_auras_only_from_level_200_with_one_seer() {
    let owned = super::era(false);
    let t = owned.tables();
    let melee = Archetype::named("melee-heavy").unwrap().plan;
    let luminous = |auras: &[(&str, u32)], seer: Option<&str>| Extras {
        luminance: Some(Luminous {
            auras: bought(auras),
            seer: seer.map(str::to_owned),
        }),
        quest_credits: true,
        ..Extras::default()
    };
    let e = luminous(
        &[("aura-of-valor", 10), ("aura-of-glory", 5)],
        Some("liam-of-gelid"),
    );
    let (b, first) = build(&t, 250, heritage::ALUVIAN, &melee, &e, &costs()).unwrap();
    assert!(first.iter().all(|f| f.warning), "{first:?}");
    let plain = dereth_progression::build(&t, 250, heritage::ALUVIAN, &melee).unwrap();
    assert_eq!(b.quest_credits, 4);
    assert_eq!(b.sheet().total_credits, plain.budget.credits() + 4);
    assert_eq!(b.held.get(&333), Some(&10));
    assert!(
        check(&t, &b.sheet()).is_empty(),
        "{:?}",
        check(&t, &b.sheet())
    );
    // Ranks past Nalicana's without the Seer who sells them.
    let other = luminous(&[("aura-of-valor", 10)], Some("lord-tyragar"));
    let refused = found(build(&t, 250, heritage::ALUVIAN, &melee, &other, &costs()));
    assert!(refused.iter().any(|f| !f.warning), "{refused:?}");
    let mut two_seers = b.sheet();
    two_seers.augmentations.insert(336, 6);
    assert!(!check(&t, &two_seers).is_empty(), "two Seers' auras");
    // Below 200 there is no luminance: auras refused, and its credits not had.
    let early = found(build(&t, 150, heritage::ALUVIAN, &melee, &e, &costs()));
    assert!(early.iter().any(|f| !f.warning), "{early:?}");
    let mut early_sheet = b.sheet();
    early_sheet.level = 150;
    assert!(!check(&t, &early_sheet).is_empty());
}

#[test]
fn augmentations_that_cost_more_than_the_level_has_are_refused() {
    let owned = super::era(false);
    let t = owned.tables();
    let melee = Archetype::named("melee-heavy").unwrap().plan;
    let extras = Extras {
        augmentations: bought(&[("quick-learner", 1)]),
        ..Extras::default()
    };
    assert!(build(&t, 80, heritage::ALUVIAN, &melee, &extras, &costs()).is_err());
    let none = Costs::default();
    let err = build(&t, 275, heritage::ALUVIAN, &melee, &extras, &none).unwrap_err();
    assert!(err.contains("no gem"), "{err}");
}

#[test]
fn a_summoning_mastery_before_level_50_is_warned_of_and_never_refused() {
    let owned = super::era(false);
    let t = owned.tables();
    let melee = Archetype::named("melee-heavy").unwrap().plan;
    let extras = Extras {
        summoning_mastery: Some(Mastery::Naturalist),
        ..Extras::default()
    };
    let (_, early) = build(&t, 30, heritage::ALUVIAN, &melee, &extras, &costs()).unwrap();
    assert!(
        early
            .iter()
            .any(|f| f.warning && f.text.contains("from level 50")),
        "{early:?}"
    );
    assert!(early.iter().all(|f| f.warning), "{early:?}");
    let (_, at_50) = build(&t, 50, heritage::ALUVIAN, &melee, &extras, &costs()).unwrap();
    assert!(
        !at_50.iter().any(|f| f.text.contains("summoning")),
        "{at_50:?}"
    );
}
