//! Character-creation arithmetic: the per-heritage skill costs, the skill-credit sum, the default
//! skill classes, the summary page's skill score and the name length.
//!
//! The character-creation wizard's state (the attribute balancing with its rotating start index,
//! the randomisers, the appearance) lives in the UI and calls these. They are functions of the dat
//! tables and plain values, so the server can check a character-creation request with the
//! client's own arithmetic.

use dereth_assets::tables::{CharGen, SkillTable};
use dereth_primitives::EraId;

/// The skill advancement class. ACE's `SkillAdvancementClass` has the same values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
#[allow(missing_docs)]
pub enum SkillAdvancementClass {
    #[default]
    Inactive = 0,
    Untrained = 1,
    Trained = 2,
    Specialized = 3,
}

/// The **minimum** every attribute has, and the value the remaining-credits count charges an
/// unlocked attribute that is not being edited.
pub const ATTR_MIN: i32 = 10;

/// The length of the skill array on the wire.
///
/// ACE hard-codes it — `PlayerFactory.Create`: "`if (characterCreateInfo.SkillAdvancementClasses
/// .Count != 55) return ClientServerSkillsMismatch`", and a mismatch **boots the account** with
/// `ClientVersionIncorrect`. The client writes exactly that many entries.
pub const TOTAL_NUM_SKILLS: usize = 55;

/// The longest name the wizard stores: the name buffer is 33 bytes, so **32 characters**.
pub const NAME_MAX_CHARS: usize = 32;

/// The heritage's `Skills` overrides layered on the `SkillTable` defaults — the trained and
/// specialized costs, which ACE's `PlayerFactory.Create` reimplements one-for-one.
#[must_use]
pub fn skill_costs(cg: &CharGen, skills: &SkillTable, heritage: u32, skill: u32) -> (i32, i32) {
    let base = skills.skills.get(&skill);
    // The char-gen data's trained-skill cost read and
    // its specialized-skill cost read fall off the end of both lookups with **-1**, which
    // is the value the skill-level reset's `trained >= 0 && specialized >= 0` gate is
    // written against, so it must not be `i32::MAX`.
    let mut trained = base.map_or(-1, |s| s.trained_cost);
    let mut specialized = base.map_or(-1, |s| s.specialized_cost);
    if let Some(hg) = cg.heritage_groups.get(&heritage) {
        if let Some((_, n, p)) = hg.skills.iter().find(|(id, _, _)| *id == skill) {
            trained = *n;
            specialized = *p;
        }
    }
    (trained, specialized)
}

/// The class the skill-level reset gives skill `id` for this heritage.
///
/// A skill the table does not name, or whose either cost is negative, stays **`Inactive`** (the
/// array's zero). Otherwise a skill that costs nothing to train is granted: `Specialized` when
/// specialising is free as well, else `Trained`. Every other skill starts `Untrained`.
#[must_use]
pub fn default_skill_class(
    cg: &CharGen,
    skills: &SkillTable,
    heritage: u32,
    id: u32,
) -> SkillAdvancementClass {
    if !skills.skills.contains_key(&id) {
        return SkillAdvancementClass::Inactive;
    }
    let (trained, specialized) = skill_costs(cg, skills, heritage, id);
    if trained < 0 || specialized < 0 {
        return SkillAdvancementClass::Inactive;
    }
    if trained < 1 {
        if specialized < 1 {
            SkillAdvancementClass::Specialized
        } else {
            SkillAdvancementClass::Trained
        }
    } else {
        SkillAdvancementClass::Untrained
    }
}

/// The eleven pre-2012 weapon skill ids (Axe through Unarmed Combat), which the end-of-retail
/// server skill table holds at zero cost while the creation wizard does not offer them.
pub const RETIRED_WEAPON_SKILLS: [u32; 11] = [1, 2, 3, 4, 5, 8, 9, 10, 11, 12, 13];

/// The 36 skills eligible at character creation in February 2005 (Infiltration): Axe (1) through
/// Unarmed Combat (13) without Sling (8), and none of the post-2005 weapon or magic skills.
const INFILTRATION_CREATION_SKILLS: [u32; 36] = [
    1, 2, 3, 4, 5, 6, 7, 9, 10, 11, 12, 13, 14, 15, 16, 18, 19, 20, 21, 22, 23, 24, 27, 28, 29, 30,
    31, 32, 33, 34, 35, 36, 37, 38, 39, 40,
];

/// The 35 skills eligible at character creation in November 1999 (Launch): February 2005's set
/// without Salvaging (40).
const LAUNCH_CREATION_SKILLS: [u32; 35] = [
    1, 2, 3, 4, 5, 6, 7, 9, 10, 11, 12, 13, 14, 15, 16, 18, 19, 20, 21, 22, 23, 24, 27, 28, 29, 30,
    31, 32, 33, 34, 35, 36, 37, 38, 39,
];

/// The skill ids eligible at character creation in `era`, or `None` when every skill in the
/// portal skill table is eligible (end of retail).
#[must_use]
pub const fn era_creation_skills(era: EraId) -> Option<&'static [u32]> {
    match era {
        EraId::Launch => Some(&LAUNCH_CREATION_SKILLS),
        EraId::Infiltration => Some(&INFILTRATION_CREATION_SKILLS),
        EraId::Eor => None,
    }
}

/// The minimum skill advancement class character creation requires for `id` on `heritage` in
/// `era`: `Trained` when training costs nothing, `Specialized` when specialising costs nothing as
/// well, and `Inactive` otherwise (including when `id` is outside `era`'s creation skill set, is
/// one of [`RETIRED_WEAPON_SKILLS`], is missing from `skills`, or has a negative cost).
#[must_use]
pub fn required_free_skill_class(
    cg: &CharGen,
    skills: &SkillTable,
    heritage: u32,
    era: EraId,
    id: u32,
) -> SkillAdvancementClass {
    if era_creation_skills(era).is_some_and(|allowed| !allowed.contains(&id))
        || RETIRED_WEAPON_SKILLS.contains(&id)
    {
        return SkillAdvancementClass::Inactive;
    }
    match default_skill_class(cg, skills, heritage, id) {
        SkillAdvancementClass::Trained => SkillAdvancementClass::Trained,
        SkillAdvancementClass::Specialized => SkillAdvancementClass::Specialized,
        SkillAdvancementClass::Inactive | SkillAdvancementClass::Untrained => {
            SkillAdvancementClass::Inactive
        }
    }
}

/// The [`TOTAL_NUM_SKILLS`]-entry skill advancement class array seeded with `heritage`'s required
/// free baseline skills in `era` ([`required_free_skill_class`]) and every other entry left
/// `Inactive`.
#[must_use]
pub fn default_creation_skill_classes(
    cg: &CharGen,
    skills: &SkillTable,
    heritage: u32,
    era: EraId,
) -> Vec<SkillAdvancementClass> {
    (0..TOTAL_NUM_SKILLS)
        .map(|i| {
            u32::try_from(i).map_or(SkillAdvancementClass::Inactive, |id| {
                required_free_skill_class(cg, skills, heritage, era, id)
            })
        })
        .collect()
}

/// The [`TOTAL_NUM_SKILLS`]-entry skill advancement class array starting from `heritage`'s
/// required free baseline skills in `era` ([`default_creation_skill_classes`]) and raising each
/// `(skill_id, class)` in `overrides` to at least `class` when eligible in `era`.
///
/// Required free skills are never lowered below their baseline floor, and non-free skills outside
/// `overrides` remain `Inactive`.
#[must_use]
pub fn creation_skill_classes(
    cg: &CharGen,
    skills: &SkillTable,
    heritage: u32,
    era: EraId,
    overrides: &[(u32, SkillAdvancementClass)],
) -> Vec<SkillAdvancementClass> {
    let mut classes = default_creation_skill_classes(cg, skills, heritage, era);
    let allowed = era_creation_skills(era);
    for &(id, sac) in overrides {
        if id == 0 || allowed.is_some_and(|set| !set.contains(&id)) {
            continue;
        }
        if let Some(slot) = usize::try_from(id).ok().and_then(|i| classes.get_mut(i)) {
            *slot = (*slot).max(sac);
        }
    }
    classes
}

/// The skill credits `levels` spend — a **full re-sum**, not a delta.
///
/// Walks skill ids `1..TOTAL_NUM_SKILLS` (the loop starts at **1**) and adds each trained skill's
/// trained cost and each specialised skill's specialised cost. An unaffordable choice simply drives
/// the remainder negative; affordability is the skills page's test. `levels` is indexed by skill
/// id; an id past its end counts as `Inactive`.
#[must_use]
pub fn skill_credits_used(
    cg: &CharGen,
    skills: &SkillTable,
    heritage: u32,
    levels: &[SkillAdvancementClass],
) -> i32 {
    let mut used = 0i32;
    for i in 1..TOTAL_NUM_SKILLS {
        let id = u32::try_from(i).unwrap_or(0);
        let (trained, specialized) = skill_costs(cg, skills, heritage, id);
        used += match levels.get(i).copied().unwrap_or_default() {
            SkillAdvancementClass::Trained => trained,
            SkillAdvancementClass::Specialized => specialized,
            _ => 0,
        };
    }
    used
}

/// What the **summary page** prints beside a skill.
///
/// 0 when the skill is not in the table, when its level is below the skill's minimum level, or
/// when the formula fails; otherwise the formula over the skill's two attributes, plus 5 when
/// trained or 10 when specialized. `attribute` answers an attribute by its enum number (1 strength
/// … 6 self), and 0 for anything else.
///
/// The formula is exactly the shared skill formula,
/// [`crate::attributes::skill_formula_calculate`], over the two attribute values (an attribute's
/// bit pattern is read as unsigned, as there), and its result is read back as a signed number
/// before the training bonus is added.
#[must_use]
pub fn skill_score(
    skills: &SkillTable,
    skill: u32,
    level: SkillAdvancementClass,
    attribute: impl Fn(u32) -> i32,
) -> i32 {
    let Some(base) = skills.skills.get(&skill) else {
        return 0;
    };
    if base.min_level > level as u32 {
        return 0;
    }
    let f = &base.formula;
    let bits = |v: i32| u32::from_ne_bytes(v.to_ne_bytes());
    let Some(v) = crate::attributes::skill_formula_calculate(
        f,
        bits(attribute(f.attr1)),
        bits(attribute(f.attr2)),
    ) else {
        return 0;
    };
    let out = i32::from_ne_bytes(v.to_ne_bytes());
    match level {
        SkillAdvancementClass::Trained => out.wrapping_add(5),
        SkillAdvancementClass::Specialized => out.wrapping_add(10),
        _ => out,
    }
}

/// Whether the summary page's name box may store `s`: not empty, and at most [`NAME_MAX_CHARS`]
/// characters.
#[must_use]
pub fn name_length_ok(s: &str) -> bool {
    !s.is_empty() && s.chars().count() <= NAME_MAX_CHARS
}

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_assets::tables::{SkillBase, SkillFormula};
    use dereth_primitives::DataId;
    use std::collections::BTreeMap;

    fn table(trained: i32, specialized: i32) -> SkillTable {
        let mut skills = BTreeMap::new();
        skills.insert(
            6u32,
            SkillBase {
                description: String::new(),
                name: String::new(),
                icon: 0,
                trained_cost: trained,
                specialized_cost: specialized,
                category: 1,
                chargen_use: 0,
                min_level: 1,
                formula: SkillFormula {
                    w: 0,
                    x: 1,
                    y: 1,
                    z: 3,
                    attr1: 4,
                    attr2: 3,
                },
                upper_bound: 0.0,
                lower_bound: 0.0,
                learn_mod: 0.0,
            },
        );
        SkillTable {
            id: DataId(0x0E00_0004),
            buckets: 11,
            skills,
        }
    }

    fn no_heritages() -> CharGen {
        CharGen {
            help_strings: vec![],
            heritage_order: vec![],
            id: DataId(0x0E00_0002),
            second_data_id: DataId(0),
            starter_areas: Vec::new(),
            hg_table_marker: 0,
            heritage_groups: BTreeMap::new(),
        }
    }

    /// Oracle: the reset's cost gate — a missing row is `-1`, a free train is granted, a free
    /// train and specialisation is granted specialised.
    #[test]
    fn default_classes_follow_the_two_costs() {
        let cg = no_heritages();
        assert_eq!(
            skill_costs(&cg, &table(4, 8), 1, 7),
            (-1, -1),
            "no row: both costs -1"
        );
        assert_eq!(
            default_skill_class(&cg, &table(4, 8), 1, 7),
            SkillAdvancementClass::Inactive
        );
        assert_eq!(
            default_skill_class(&cg, &table(4, 8), 1, 6),
            SkillAdvancementClass::Untrained
        );
        assert_eq!(
            default_skill_class(&cg, &table(0, 8), 1, 6),
            SkillAdvancementClass::Trained
        );
        assert_eq!(
            default_skill_class(&cg, &table(0, 0), 1, 6),
            SkillAdvancementClass::Specialized
        );
        assert_eq!(
            default_skill_class(&cg, &table(-1, 0), 1, 6),
            SkillAdvancementClass::Inactive
        );
    }

    /// Oracle: the full re-sum over ids 1..55 — id 0 is never charged.
    #[test]
    fn credits_are_a_full_resum_from_skill_one() {
        let cg = no_heritages();
        let mut levels = vec![SkillAdvancementClass::Inactive; TOTAL_NUM_SKILLS];
        assert_eq!(skill_credits_used(&cg, &table(4, 8), 1, &levels), 0);
        levels[6] = SkillAdvancementClass::Trained;
        assert_eq!(skill_credits_used(&cg, &table(4, 8), 1, &levels), 4);
        levels[6] = SkillAdvancementClass::Specialized;
        assert_eq!(skill_credits_used(&cg, &table(4, 8), 1, &levels), 8);
    }

    /// Oracle: the summary page's formula, `floor((a1 + a2) / 3 + 0.5)` plus the class bonus.
    #[test]
    fn the_summary_score_adds_five_or_ten() {
        let t = table(4, 8);
        let attr = |n: u32| {
            if n == 4 {
                100
            } else if n == 3 {
                50
            } else {
                0
            }
        };
        assert_eq!(
            skill_score(&t, 6, SkillAdvancementClass::Untrained, attr),
            50
        );
        assert_eq!(skill_score(&t, 6, SkillAdvancementClass::Trained, attr), 55);
        assert_eq!(
            skill_score(&t, 6, SkillAdvancementClass::Specialized, attr),
            60
        );
        assert_eq!(
            skill_score(&t, 6, SkillAdvancementClass::Inactive, attr),
            0,
            "below min level"
        );
        assert_eq!(
            skill_score(&t, 9, SkillAdvancementClass::Trained, attr),
            0,
            "not in the table"
        );
    }

    /// Oracle: the summary score is the shared skill formula, single-precision division and all,
    /// plus 5 trained or 10 specialized.
    #[test]
    fn the_summary_skill_score_is_the_skill_formula_plus_its_training_bonus() {
        let mut t = table(4, 8);
        for (w, x, y, z) in [
            (0, 1, 1, 3),
            (0, 1, 0, 1),
            (10, 2, 1, 6),
            (0, 3, 2, 7),
            (5, 0, 1, 2),
        ] {
            let row = t.skills.get_mut(&6).expect("the row");
            row.formula = SkillFormula {
                w,
                x,
                y,
                z,
                attr1: 4,
                attr2: 3,
            };
            let f = row.formula;
            for a in 10..=100i32 {
                for b in [10i32, 55, 100] {
                    let attr = |n: u32| {
                        if n == 4 {
                            a
                        } else if n == 3 {
                            b
                        } else {
                            0
                        }
                    };
                    let raw = crate::attributes::skill_formula_calculate(
                        &f,
                        a.unsigned_abs(),
                        b.unsigned_abs(),
                    )
                    .expect("z is non-zero");
                    let raw = i32::try_from(raw).expect("small");
                    for (level, bonus) in [
                        (SkillAdvancementClass::Untrained, 0),
                        (SkillAdvancementClass::Trained, 5),
                        (SkillAdvancementClass::Specialized, 10),
                    ] {
                        assert_eq!(skill_score(&t, 6, level, attr), raw + bonus);
                    }
                }
            }
        }
        // The division is single precision: 2^24 + 1 over 1 is 2^24, which double precision would
        // not round away.
        t.skills.get_mut(&6).expect("the row").formula = SkillFormula {
            w: 0,
            x: 1,
            y: 0,
            z: 1,
            attr1: 4,
            attr2: 3,
        };
        let big = |n: u32| if n == 4 { 16_777_217 } else { 0 };
        assert_eq!(
            skill_score(&t, 6, SkillAdvancementClass::Untrained, big),
            16_777_216
        );
        // And a zero divisor scores 0, bonus or not.
        t.skills.get_mut(&6).expect("the row").formula.z = 0;
        assert_eq!(
            skill_score(&t, 6, SkillAdvancementClass::Specialized, big),
            0
        );
    }

    #[test]
    fn the_name_limit_is_thirty_two_characters_and_empty_is_refused() {
        assert!(!name_length_ok(""));
        assert!(name_length_ok(&"a".repeat(32)));
        assert!(!name_length_ok(&"a".repeat(33)));
    }

    fn skill_entry(trained: i32, specialized: i32) -> SkillBase {
        SkillBase {
            description: String::new(),
            name: String::new(),
            icon: 0,
            trained_cost: trained,
            specialized_cost: specialized,
            category: 1,
            chargen_use: 1,
            min_level: 1,
            formula: SkillFormula {
                w: 0,
                x: 1,
                y: 1,
                z: 3,
                attr1: 4,
                attr2: 3,
            },
            upper_bound: 0.0,
            lower_bound: 0.0,
            learn_mod: 0.0,
        }
    }

    fn chargen_with_heritage_skills(heritage: u32, skills: Vec<(u32, i32, i32)>) -> CharGen {
        use dereth_assets::tables::HeritageGroup;
        let mut cg = no_heritages();
        cg.heritage_order = vec![heritage];
        cg.heritage_groups.insert(
            heritage,
            HeritageGroup {
                description: None,
                name: "Test".into(),
                icon: 0,
                setup: DataId(0),
                environment_setup: DataId(0),
                attribute_credits: 330,
                skill_credits: 52,
                primary_start_areas: vec![0],
                secondary_start_areas: Vec::new(),
                skills,
                templates: Vec::new(),
                sex_order: Vec::new(),
                template_presentations: BTreeMap::new(),
                sex_table_marker: 0,
                sexes: BTreeMap::new(),
            },
        );
        cg
    }

    #[test]
    fn required_free_skills_respect_era_heritage_retired_and_default_classes() {
        let mut skills = SkillTable {
            id: DataId(0x0E00_0004),
            buckets: 11,
            skills: BTreeMap::new(),
        };
        // Retired weapon held at zero cost in EoR table.
        skills.skills.insert(11, skill_entry(0, 0));
        // Paid base skill discounted to free Trained by heritage 1.
        skills.skills.insert(14, skill_entry(4, 6));
        // Free Trained in base table.
        skills.skills.insert(22, skill_entry(0, 4));
        // Free Specialized in base table.
        skills.skills.insert(24, skill_entry(0, 0));
        // Paid skill not free for any heritage.
        skills.skills.insert(21, skill_entry(6, 12));
        // Negative-cost skill (unselectable).
        skills.skills.insert(17, skill_entry(-1, 0));
        // Salvaging: free in EoR/Infiltration, excluded in Launch.
        skills.skills.insert(40, skill_entry(0, 4));
        // Heavy Weapons: EoR only.
        skills.skills.insert(44, skill_entry(6, 12));

        let cg = chargen_with_heritage_skills(1, vec![(14, 0, 2)]);

        let eor = default_creation_skill_classes(&cg, &skills, 1, EraId::Eor);
        assert_eq!(eor.len(), TOTAL_NUM_SKILLS);
        assert_eq!(eor[11], SkillAdvancementClass::Inactive, "retired weapon");
        assert_eq!(
            eor[14],
            SkillAdvancementClass::Trained,
            "heritage-discounted free skill"
        );
        assert_eq!(eor[22], SkillAdvancementClass::Trained);
        assert_eq!(eor[24], SkillAdvancementClass::Specialized);
        assert_eq!(eor[40], SkillAdvancementClass::Trained);
        assert_eq!(
            eor[21],
            SkillAdvancementClass::Inactive,
            "paid skill stays Inactive"
        );
        assert_eq!(
            eor[17],
            SkillAdvancementClass::Inactive,
            "negative cost stays Inactive"
        );
        assert_eq!(
            eor[44],
            SkillAdvancementClass::Inactive,
            "paid EoR skill stays Inactive"
        );
        assert_eq!(
            eor.iter()
                .filter(|&&c| c != SkillAdvancementClass::Inactive)
                .count(),
            4
        );

        // Another heritage without the Arcane Lore discount does not get skill 14 free.
        let other = default_creation_skill_classes(&cg, &skills, 2, EraId::Eor);
        assert_eq!(other[14], SkillAdvancementClass::Inactive);
        assert_eq!(other[22], SkillAdvancementClass::Trained);
        assert_eq!(other[24], SkillAdvancementClass::Specialized);
        assert_eq!(other[40], SkillAdvancementClass::Trained);

        // Launch excludes Salvaging (40) even when free in the table.
        let launch = default_creation_skill_classes(&cg, &skills, 1, EraId::Launch);
        assert_eq!(launch[40], SkillAdvancementClass::Inactive);
        assert_eq!(launch[14], SkillAdvancementClass::Trained);
        assert_eq!(launch[22], SkillAdvancementClass::Trained);
        assert_eq!(launch[24], SkillAdvancementClass::Specialized);

        // Overrides raise eligible skills without lowering a free Specialized skill, and skip
        // skills outside the era's creation set.
        let overlaid = creation_skill_classes(
            &cg,
            &skills,
            1,
            EraId::Infiltration,
            &[
                (21, SkillAdvancementClass::Trained),
                (24, SkillAdvancementClass::Trained),
                (44, SkillAdvancementClass::Trained),
                (13, SkillAdvancementClass::Trained),
            ],
        );
        assert_eq!(overlaid[21], SkillAdvancementClass::Trained);
        assert_eq!(
            overlaid[24],
            SkillAdvancementClass::Specialized,
            "must not lower a free Specialized baseline skill"
        );
        assert_eq!(
            overlaid[44],
            SkillAdvancementClass::Inactive,
            "Heavy Weapons is outside Infiltration's creation skills"
        );
        assert_eq!(
            overlaid[13],
            SkillAdvancementClass::Trained,
            "explicit Infiltration weapon override is kept"
        );
        assert_eq!(overlaid[40], SkillAdvancementClass::Trained);

        // Skill 0 is never a valid creation skill even when unconstrained by an era list.
        let with_zero = creation_skill_classes(
            &cg,
            &skills,
            1,
            EraId::Eor,
            &[(0, SkillAdvancementClass::Trained)],
        );
        assert_eq!(with_zero[0], SkillAdvancementClass::Inactive);
    }
}
