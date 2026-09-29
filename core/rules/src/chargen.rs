//! Character-creation arithmetic: the per-heritage skill costs, the skill-credit sum, the default
//! skill classes, the summary page's skill score and the name length.
//!
//! The character-creation wizard's state (the attribute balancing with its rotating start index,
//! the randomisers, the appearance) lives in the UI and calls these. They are functions of the dat
//! tables and plain values, so the server can check a character-creation request with the
//! client's own arithmetic.

use dereth_assets::tables::{CharGen, SkillTable};

/// The skill advancement class. ACE's `SkillAdvancementClass` has the same values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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
}
