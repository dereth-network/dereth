// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/AttributeFormula.cs
//! Port of `Source/ACE.Server/Entity/AttributeFormula.cs`.
//!
//! Calculates the amount to add to a creature's skills and vitals from its primary attributes,
//! with the dat formulas (`SkillTable`, `SecondaryAttributeTable`). ACE's three `GetFormula`
//! overloads are [`get_formula_skill`], [`get_formula_vital`] and [`get_formula`].

use std::sync::Arc;

use dereth_assets::tables::SkillFormula;
use empyrean_common::dotnet::CsCast;
use empyrean_common::extensions::float_extensions;
use empyrean_entity::enums::{PropertyAttribute, PropertyAttribute2nd, Skill};

use crate::world_objects::entity::creature_attribute::{uint_to_f32, StatCtx};

// ACE: AttributeFormula.GetFormula
/// The amount to add to a creature's skill from its attributes (`current`: enchanted), by the
/// skill's `SkillTable` formula; 0 for a skill the table lacks.
#[must_use]
pub fn get_formula_skill(c: &mut StatCtx<'_>, skill: Skill, current: bool) -> u32 {
    let dats = Arc::clone(&c.world().dats);
    let skill_table = dats.portal_dat().skill_table();

    let key: u32 = skill.0.cs_cast();
    let Some(skill_base) = skill_table.skills.get(&key) else {
        return 0;
    };

    get_formula(c, &skill_base.formula, current)
}

// ACE: AttributeFormula.GetFormula
/// The amount to add to a creature's max vital from its attributes, by the
/// `SecondaryAttributeTable` formula; 0 for anything but the three max vitals.
#[must_use]
pub fn get_formula_vital(c: &mut StatCtx<'_>, vital: PropertyAttribute2nd, current: bool) -> u32 {
    let dats = Arc::clone(&c.world().dats);
    let vital_table = dats.portal_dat().secondary_attribute_table();

    match vital {
        PropertyAttribute2nd::MaxHealth => get_formula(c, &vital_table.health, current),
        PropertyAttribute2nd::MaxStamina => get_formula(c, &vital_table.stamina, current),
        PropertyAttribute2nd::MaxMana => get_formula(c, &vital_table.mana, current),
        _ => 0,
    }
}

// ACE: AttributeFormula.GetFormula
/// Applies a dat `SkillFormula` to the creature's primary attributes: 0 when `X` is 0; else
/// `attr1 (+ attr2)`, divided by `Z` in `float` and rounded half away from zero unless `Z` is 1.
/// (ACE reads only `X` as an on/off switch, never `W` or `Y`.)
///
/// # Panics
/// When the formula names an attribute the creature lacks (ACE: `KeyNotFoundException`).
#[must_use]
pub fn get_formula(c: &mut StatCtx<'_>, formula: &SkillFormula, current: bool) -> u32 {
    if formula.x == 0 {
        return 0;
    }

    let attr1 = PropertyAttribute(formula.attr1.cs_cast());
    let attr2 = PropertyAttribute(formula.attr2.cs_cast());
    let divisor = formula.z;

    let mut attribute = |a: PropertyAttribute| {
        let ca = c
            .creature()
            .attributes()
            .get(&a)
            .copied()
            .unwrap_or_else(|| {
                panic!(
                    "KeyNotFoundException: Creature.Attributes[{}]",
                    a.to_dotnet_string()
                )
            });
        if current {
            ca.current(c)
        } else {
            ca.base(c.creature())
        }
    };

    let mut total = attribute(attr1);
    if attr2 != PropertyAttribute::Undef {
        total = total.wrapping_add(attribute(attr2));
    }

    if divisor != 1 {
        let rounded = float_extensions::round(uint_to_f32(total) / uint_to_f32(divisor), 0);
        total = rounded.cs_cast();
    }

    total
}
