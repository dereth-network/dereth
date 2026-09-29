// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/SkillCheck.cs
//! Port of `Source/ACE.Server/WorldObjects/SkillCheck.cs`.
//!
//! A static class in ACE: free functions here. The `int` and `uint` overloads of `GetSkillChance`
//! are [`get_skill_chance`] and [`get_skill_chance_uint`]; ACE's default `factor` is
//! [`DEFAULT_FACTOR`].

use empyrean_common::dotnet::{math, CsCast};

/// The default `factor` of both `GetSkillChance` overloads.
pub const DEFAULT_FACTOR: f32 = 0.03;

// ACE: SkillCheck.GetSkillChance
/// `GetSkillChance(int skill, int difficulty, float factor = 0.03f)`: the logistic chance of
/// `skill` beating `difficulty`, clamped to `0..=1`.
#[must_use]
pub fn get_skill_chance(skill: i32, difficulty: i32, factor: f32) -> f64 {
    // `skill - difficulty` is an unchecked int subtraction; `factor * int` is a float product.
    let chance = 1.0
        - (1.0
            / (1.0
                + empyrean_common::math::exp(f64::from(
                    factor * skill.wrapping_sub(difficulty) as f32,
                ))));

    math::min(1.0, math::max(0.0, chance))
}

// ACE: SkillCheck.GetSkillChance
/// `GetSkillChance(uint skill, uint difficulty, float factor = 0.03f)`: casts both to `int`
/// (wrapping) and calls the `int` overload.
#[must_use]
pub fn get_skill_chance_uint(skill: u32, difficulty: u32, factor: f32) -> f64 {
    get_skill_chance(skill.cs_cast(), difficulty.cs_cast(), factor)
}

// ACE: SkillCheck.GetMagicSkillChance
/// `GetMagicSkillChance(int skill, int difficulty)`: [`get_skill_chance`] with factor `0.07f`.
#[must_use]
pub fn get_magic_skill_chance(skill: i32, difficulty: i32) -> f64 {
    get_skill_chance(skill, difficulty, 0.07)
}
