//! The spell-range calculation's two halves.
//!
//! Cut out of `dereth_ui_screens::panels::spell_examine`, which keeps the panel. These three are
//! the arithmetic `dereth_client_runtime::hud` has to do for itself, because the range depends on the
//! player's skills and only the world half has them: the school's skill, the five skills the
//! fallback takes the maximum of, and the clamped affine range.

/// The skill the spell-range calculation asks the player for, chosen by school. `0` for a school
/// outside `1..=5`, and that zero is what makes the range fall back to *the best magic skill*.
#[must_use]
pub const fn skill_for_spell(school: u32) -> u32 {
    match school {
        1 => 0x22,
        2 => 0x21,
        3 => 0x20,
        4 => 0x1F,
        5 => 0x2B,
        _ => 0,
    }
}

/// The five skills the spell-range calculation takes the maximum of when [`skill_for_spell`]
/// answers `0` — `0x1F`, `0x20`, `0x21`, `0x22`, `0x2B`, read in that order.
pub const MAGIC_SKILLS: [u32; 5] = [0x1F, 0x20, 0x21, 0x22, 0x2B];

/// The range arithmetic, once the skill is chosen:
/// `min(75, _base_range_constant + _base_range_mod * skill)`.
///
/// The clamp is one-sided: a negative constant is **not** raised to zero, and the caller's
/// `range == 0.0` test is an exact float compare, not a threshold.
#[must_use]
pub fn spell_range(base_range_constant: f32, base_range_mod: f32, skill: i32) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    let r = base_range_mod * (skill as f32) + base_range_constant;
    if r > 75.0 {
        75.0
    } else {
        r
    }
}
