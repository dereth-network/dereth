// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.DatLoader/FileTypes/SkillTable.cs, Source/ACE.DatLoader/Entity/SkillFormula.cs, Source/ACE.DatLoader/Entity/SkillBase.cs
//! `SkillTable.AddRetiredSkills`, which `DatManager.Initialize` runs on the loaded table.

use dereth_assets::tables::{SkillBase, SkillFormula};
use dereth_assets::SkillTable;

use empyrean_entity::enums::{PropertyAttribute, Skill};

use super::AceThrow;

// `(uint)Skill` and `(uint)PropertyAttribute`: the dat table is keyed by the raw values.
#[allow(clippy::cast_sign_loss)]
const fn skill(s: Skill) -> u32 {
    s.0 as u32
}

fn attr(a: PropertyAttribute) -> u32 {
    u32::from(a.0)
}

/// ACE's `new SkillFormula(attr1, attr2, divisor)`: `X = 1`, `Z = divisor`, `W = Y = 0`.
// ACE: SkillFormula.SkillFormula
#[must_use]
pub fn skill_formula(attr1: u32, attr2: u32, divisor: u32) -> SkillFormula {
    SkillFormula {
        w: 0,
        x: 1,
        y: 0,
        z: divisor,
        attr1,
        attr2,
    }
}

/// ACE's `new SkillBase(formula)`: every other field at its default (ACE's strings are `null`;
/// here they are empty).
// ACE: SkillBase.SkillBase
#[must_use]
pub fn skill_base(formula: SkillFormula) -> SkillBase {
    SkillBase {
        description: String::new(),
        name: String::new(),
        icon: 0,
        trained_cost: 0,
        specialized_cost: 0,
        category: 0,
        chargen_use: 0,
        min_level: 0,
        formula,
        upper_bound: 0.0,
        lower_bound: 0.0,
        learn_mod: 0.0,
    }
}

/// ACE's `SkillBase.UpgradeCostFromTrainedToSpecialized`.
#[must_use]
pub fn upgrade_cost_from_trained_to_specialized(skill: &SkillBase) -> i32 {
    skill.specialized_cost.wrapping_sub(skill.trained_cost)
}

/// ACE's `SkillTable` helpers.
pub trait SkillTableExt {
    /// Add the ten retired weapon skills, which the retail table no longer carries but old
    /// characters and content still name. `Err` where ACE's `Dictionary.Add` throws: the table
    /// already has one of them.
    ///
    /// # Errors
    ///
    /// [`AceThrow::DuplicateKey`] for the first retired skill already present.
    fn add_retired_skills(&mut self) -> Result<(), AceThrow>;
}

impl SkillTableExt for SkillTable {
    // ACE: SkillTable.AddRetiredSkills
    fn add_retired_skills(&mut self) -> Result<(), AceThrow> {
        let adds = [
            (
                skill(Skill::Axe),
                skill_formula(
                    attr(PropertyAttribute::Strength),
                    attr(PropertyAttribute::Coordination),
                    3,
                ),
            ),
            (
                skill(Skill::Bow),
                skill_formula(
                    attr(PropertyAttribute::Coordination),
                    attr(PropertyAttribute::Undef),
                    2,
                ),
            ),
            (
                skill(Skill::Crossbow),
                skill_formula(
                    attr(PropertyAttribute::Coordination),
                    attr(PropertyAttribute::Undef),
                    2,
                ),
            ),
            (
                skill(Skill::Dagger),
                skill_formula(
                    attr(PropertyAttribute::Quickness),
                    attr(PropertyAttribute::Coordination),
                    3,
                ),
            ),
            (
                skill(Skill::Mace),
                skill_formula(
                    attr(PropertyAttribute::Strength),
                    attr(PropertyAttribute::Coordination),
                    3,
                ),
            ),
            (
                skill(Skill::Spear),
                skill_formula(
                    attr(PropertyAttribute::Strength),
                    attr(PropertyAttribute::Coordination),
                    3,
                ),
            ),
            (
                skill(Skill::Staff),
                skill_formula(
                    attr(PropertyAttribute::Strength),
                    attr(PropertyAttribute::Coordination),
                    3,
                ),
            ),
            (
                skill(Skill::Sword),
                skill_formula(
                    attr(PropertyAttribute::Strength),
                    attr(PropertyAttribute::Coordination),
                    3,
                ),
            ),
            (
                skill(Skill::ThrownWeapon),
                skill_formula(
                    attr(PropertyAttribute::Coordination),
                    attr(PropertyAttribute::Undef),
                    2,
                ),
            ),
            (
                skill(Skill::UnarmedCombat),
                skill_formula(
                    attr(PropertyAttribute::Strength),
                    attr(PropertyAttribute::Coordination),
                    3,
                ),
            ),
        ];
        for (skill, formula) in adds {
            if self.skills.contains_key(&skill) {
                return Err(AceThrow::DuplicateKey(skill));
            }
            self.skills.insert(skill, skill_base(formula));
        }
        Ok(())
    }
}
