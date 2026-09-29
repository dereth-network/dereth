// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/SkillFormula.cs
//! Port of `Source/ACE.Server/WorldObjects/SkillFormula.cs`.
//!
//! ACE declares a (never instantiated) class with static members: free functions here.

use empyrean_common::dotnet::math;

// ACE: SkillFormula.DefaultMod
/// Everything else: melee weapons (including finesse), thrown weapons, atlatls.
pub const DEFAULT_MOD: f32 = 0.011;

// ACE: SkillFormula.BowMod
/// Bows and crossbows.
pub const BOW_MOD: f32 = 0.008;

// ACE: SkillFormula.ArmorMod
/// `200.0f / 3.0f`, folded by the C# compiler in `float`.
pub const ARMOR_MOD: f32 = 200.0 / 3.0;

// ACE: SkillFormula.GetAttributeMod
/// `GetAttributeMod(int currentSkill, bool isBow = false)`: `max(1 + (skill - 55) * factor, 1)`.
#[must_use]
pub fn get_attribute_mod(current_skill: i32, is_bow: bool) -> f32 {
    let factor = if is_bow { BOW_MOD } else { DEFAULT_MOD };

    math::max_f32(1.0 + current_skill.wrapping_sub(55) as f32 * factor, 1.0)
}

// ACE: SkillFormula.CalcArmorMod
/// Converts AL from an additive linear value to a scaled damage multiplier.
#[must_use]
pub fn calc_armor_mod(armor_level: f32) -> f32 {
    if armor_level > 0.0 {
        ARMOR_MOD / (armor_level + ARMOR_MOD)
    } else if armor_level < 0.0 {
        1.0 - armor_level / ARMOR_MOD
    } else {
        1.0
    }
}
