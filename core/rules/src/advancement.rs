//! XP curves, raise costs, level, and the item-levelling curves.
//!
//! **Never mutate a skill or attribute locally in response to a raise click.** The client sends
//! `Train*`, sets its awaiting-raise latch and does not touch its local copy until the server sends the
//! changed quality back; the latch exists precisely to stop double-spending.

use crate::skills::Sac;
use dereth_assets::tables::XpTable;
use dereth_primitives::num::math;
use dereth_primitives::num::{to_i32_f64, to_i64_f64};

/// The level a non-Throne-of-Destiny account is capped at.
pub const NON_TOD_LEVEL_CAP: i32 = 126;

/// Total experience an attribute level costs, from the attribute column; `None` past the end.
#[must_use]
pub fn experience_to_attribute_level(t: &XpTable, level: usize) -> Option<u32> {
    t.attribute_xp.get(level).copied()
}

/// Total experience a secondary-attribute level costs, from the vital column; `None` past the end.
#[must_use]
pub fn experience_to_attribute_2nd_level(t: &XpTable, level: usize) -> Option<u32> {
    t.vital_xp.get(level).copied()
}

/// Total experience a skill level costs, from the trained or the specialised column.
///
/// **Returns `0xFFFFFFFF`, not 0, when `sac` is neither trained nor specialised** — a caller that
/// subtracts `_pp` from it produces a nonsense cost, which is why the client checks
/// `_sac < TRAINED` first.
#[must_use]
pub fn experience_to_skill_level(t: &XpTable, sac: Sac, level: usize) -> u32 {
    let table = match sac {
        Sac::Trained => &t.trained_xp,
        Sac::Specialized => &t.specialized_xp,
        _ => return u32::MAX,
    };
    table.get(level).copied().unwrap_or(u32::MAX)
}

/// The inverse of [`experience_to_skill_level`]: the highest level whose cost is at or below
/// `xp`. 0 when `sac` is neither trained nor specialised.
#[must_use]
pub fn skill_level_from_experience(t: &XpTable, sac: Sac, xp: u32) -> u32 {
    let table = match sac {
        Sac::Trained => &t.trained_xp,
        Sac::Specialized => &t.specialized_xp,
        _ => return 0,
    };
    highest_index_at_or_below(table, xp)
}

/// The highest attribute level whose cumulative cost is at or below `xp`.
#[must_use]
pub fn attribute_level_from_experience(t: &XpTable, xp: u32) -> u32 {
    highest_index_at_or_below(&t.attribute_xp, xp)
}

/// The highest secondary-attribute level whose cumulative cost is at or below `xp`.
#[must_use]
pub fn attribute_2nd_level_from_experience(t: &XpTable, xp: u32) -> u32 {
    highest_index_at_or_below(&t.vital_xp, xp)
}

fn highest_index_at_or_below(table: &[u32], xp: u32) -> u32 {
    let mut level = 0u32;
    for (i, threshold) in table.iter().enumerate() {
        if *threshold <= xp {
            level = u32::try_from(i).unwrap_or(u32::MAX);
        } else {
            break;
        }
    }
    level
}

/// Total experience a character level costs; `None` past the end of the table.
#[must_use]
pub fn experience_to_level(t: &XpTable, level: usize) -> Option<u64> {
    t.level_xp.get(level).copied()
}

/// The experience needed to go from character level `from` to level `to`; 0 when `to <= from`.
#[must_use]
pub fn experience_to_raise_level(t: &XpTable, from: usize, to: usize) -> u64 {
    if to <= from {
        return 0;
    }
    let a = experience_to_level(t, to).unwrap_or(0);
    let b = experience_to_level(t, from).unwrap_or(0);
    a.saturating_sub(b)
}

/// The four `_max_*` accessors. Every list is `max + 1` long, so the maximum is `len - 1`.
#[must_use]
pub fn max_attribute_level(t: &XpTable) -> u32 {
    u32::try_from(t.attribute_xp.len().saturating_sub(1)).unwrap_or(0)
}

#[must_use]
pub fn max_attribute_2nd_level(t: &XpTable) -> u32 {
    u32::try_from(t.vital_xp.len().saturating_sub(1)).unwrap_or(0)
}

#[must_use]
pub fn max_trained_skill_level(t: &XpTable) -> u32 {
    u32::try_from(t.trained_xp.len().saturating_sub(1)).unwrap_or(0)
}

#[must_use]
pub fn max_specialized_skill_level(t: &XpTable) -> u32 {
    u32::try_from(t.specialized_xp.len().saturating_sub(1)).unwrap_or(0)
}

/// The `n` both raise-by-ten cost computations share: `min(10, remaining)` but never below 1.
///
/// **Do not rewrite this as `if rem >= 10 { 10 } else { rem.max(1) }`.** **rustc 1.98.1 /
/// LLVM 22.1.8 miscompiles it under `--release`** in exactly the callers' structure -- a table
/// and its `max` chosen
/// by a bool, `if level >= max { return 0 }`, `rem = max - level`, then `table.get(level + n)`.
/// The optimiser folded `level + n` to `max` outright (the emitted IR indexes the table with
/// the zero-extended `max` and the `10` is gone), so every ten-point cost became the cost to the
/// **cap**: Strength at rank 30 asked for 4,019,405,968 XP instead of 31,202, and the `+10`
/// button was disabled at every rank in the release build while the debug build (no
/// optimisation) was right. `clamp` lowers to `umin`/`umax` directly and does not trip the
/// fold; `rem.min(10).max(1)` and a nested `if` were also clean, `black_box` around the `if`
/// was not. Guarded by the client's raise-by-ten release test, which must be run under
/// `--release` as well as debug.
#[inline]
#[must_use]
pub fn ten_point_span(rem: u32) -> u32 {
    rem.clamp(1, 10)
}

/// What the attribute panel's "raise 1" button charges: the same shape as the skill cost, but
/// measured against the credits already spent rather than against experience already sunk.
///
/// Live traffic confirms the inferred `+1` operand: consecutive raise-10 and raise-1 requests on
/// the same attribute cost 3,746 experience for level 0 -> 10 and 713 for level 10 -> 11, while a
/// vital raise costs 73. All three match this arithmetic exactly.
#[must_use]
pub fn attribute_cost_to_raise(t: &XpTable, level_from_cp: u32, cp_spent: u32, vital: bool) -> u32 {
    let (table, max): (&Vec<u32>, u32) = if vital {
        (&t.vital_xp, max_attribute_2nd_level(t))
    } else {
        (&t.attribute_xp, max_attribute_level(t))
    };
    if level_from_cp >= max {
        return 0;
    }
    table
        .get((level_from_cp + 1) as usize)
        .copied()
        .unwrap_or(u32::MAX)
        .saturating_sub(cp_spent)
}

/// What the attribute panel's "raise 10" button charges.
///
/// Live traffic confirms that raising an attribute from level 0 to 10 costs 3,746 experience,
/// exactly this function's answer.
#[must_use]
pub fn attribute_cost_to_raise_10(
    t: &XpTable,
    level_from_cp: u32,
    cp_spent: u32,
    vital: bool,
) -> u32 {
    let (table, max): (&Vec<u32>, u32) = if vital {
        (&t.vital_xp, max_attribute_2nd_level(t))
    } else {
        (&t.attribute_xp, max_attribute_level(t))
    };
    if level_from_cp >= max {
        return 0;
    }
    let rem = max - level_from_cp;
    let n = ten_point_span(rem);
    table
        .get((level_from_cp + n) as usize)
        .copied()
        .unwrap_or(u32::MAX)
        .saturating_sub(cp_spent)
}

/// The item-levelling curves — the cumulative experience an item level costs.
///
/// The only closed-form curves in the client: 1 = linear, 2 = geometric, 3 = triangular. All three
/// return a **cumulative total**, which is what makes [`item_total_xp_to_level`] their inverse.
///
/// Curve 2 is `base * (2^level - 1)`, **not** `base * 2^(level-1)`. With the second form the two
/// functions contradict each other: base 10 at level 3 gives 40 going forward, and 40 comes back
/// as level 2. The client's own loop settles it: the
/// curve-type-2 body multiplies by two **and** accumulates (`term *= 2; total += term`). A
/// self-doubling `x += x` would need no accumulator at all. Note the resulting shape is exactly
/// curve 3's with `*= 2` in place of `+= base`.
///
/// The order of operations and the widths are the client's, and they decide the edge cases:
///
/// - `level < 1` is tested **before** the clamp, so a `max_level` below 1 is not caught by it. The
///   level is then clamped down to `max_level` (a signed comparison).
/// - Curve 1 multiplies the clamped level, **sign-extended** to 64 bits, by `base_xp`, wrapping. With
///   `max_level < 1` that is `max_level * base_xp`: 0 for a `max_level` of 0 and a wrapped negative
///   product below that.
/// - Curves 2 and 3 start from `base_xp` and run their loop `level - 1` times only when the clamped
///   level is above 1, so a clamped level of 1 **or below** returns `base_xp`, not 0.
/// - Every 64-bit multiply and add wraps; nothing saturates.
/// - Any other curve number is 0.
#[must_use]
pub fn item_level_to_total_xp(level: i32, base_xp: u64, max_level: i32, curve_type: i32) -> u64 {
    if level < 1 {
        return 0;
    }
    let level = level.min(max_level);
    match curve_type {
        1 => i64::from(level).cast_unsigned().wrapping_mul(base_xp),
        2 => {
            let mut term = base_xp;
            let mut total = base_xp;
            for _ in 1..level {
                term = term.wrapping_mul(2);
                total = total.wrapping_add(term);
            }
            total
        }
        3 => {
            let mut step = base_xp;
            let mut total = base_xp;
            for _ in 1..level {
                step = step.wrapping_add(base_xp);
                total = total.wrapping_add(step);
            }
            total
        }
        _ => 0,
    }
}

/// The inverse of the item-levelling curves: the item level a cumulative total has reached, clamped
/// down to `max_level` with a signed comparison.
///
/// Curve 1 is `floor(total / base)` in double precision (both operands converted as unsigned), then
/// the runtime's float-to-integer conversion, which stores a **64-bit** integer, of which the low 32
/// bits are kept. So the level is the quotient **wrapped modulo 2^32**, not clamped: a quotient of
/// exactly 2^31 is `i32::MIN` (which the clamp keeps, since it is below any `max_level`), 2^32 is 0,
/// 2^32 + 5 is 5. A quotient at or past 2^63 (and `base_xp == 0`, whose quotient is infinite or NaN)
/// converts to the 64-bit integer-indefinite value, whose low half is 0.
///
/// Curves 2 and 3 subtract a growing step while the remainder covers it; the step's multiply and add
/// wrap. Any other curve number is level 0.
///
/// One deliberate departure: retail's count never finishes (the client hangs) with a zero base on
/// curves 2 and 3, or on curve 2 when the doubling step wraps to exactly 2^64. Here the count stops
/// and reports the levels reached: 0 for a zero base, and 64 for the whole of `u64::MAX` counted
/// from a base of 1. The examination panel shows no item level for a zero base, so it never asks.
#[must_use]
pub fn item_total_xp_to_level(total_xp: u64, base_xp: u64, max_level: i32, curve_type: i32) -> i32 {
    let level: i32 = match curve_type {
        1 => {
            #[allow(clippy::cast_precision_loss)]
            let q = ((total_xp as f64) / (base_xp as f64)).floor();
            // The low 32 bits of the 64-bit conversion.
            #[allow(clippy::cast_possible_truncation)]
            let low = to_i64_f64(q) as i32; // LINT-OK: i64 to i32, keeping the low 32 bits on purpose
            low
        }
        2 | 3 => {
            let mut level = 0i32;
            let mut left = total_xp;
            let mut step = base_xp;
            if base_xp != 0 {
                while left >= step {
                    level = level.wrapping_add(1);
                    left -= step;
                    step = if curve_type == 2 {
                        step.wrapping_mul(2)
                    } else {
                        step.wrapping_add(base_xp)
                    };
                    if step == 0 && curve_type == 2 {
                        break;
                    }
                }
            }
            level
        }
        _ => 0,
    };
    level.min(max_level)
}

/// The vitae CP pool threshold: the experience that must be earned before one step of vitae is
/// burned off.
///
/// The curve is `(level^2.5 * 2.5 + 20.0) * vitae^5.0 + 0.5`, truncated toward zero. Two details
/// the formula alone does not give, and both are modelled here: `level` is taken as an unsigned
/// whole number, and `vitae` arrives as a single-precision value widened to double **before** the
/// power, so there is no second rounding between the two powers. A reimplementation that rounds
/// the vitae fraction first, or that uses ACE's shape rather than these constants, disagrees with
/// the client at every level above 1.
#[must_use]
pub fn vitae_cp_pool_threshold(vitae: f64, level: f64) -> i32 {
    to_i32_f64((math::pow(level, 2.5) * 2.5 + 20.0) * math::pow(vitae, 5.0) + 0.5)
}

/// The experience header the character panel shows: total, experience into the current level,
/// experience still owed for it, and whether the cap has replaced the number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExperienceHeader {
    pub total: u64,
    pub xp_into_level: u64,
    pub xp_to_level: u64,
    /// True when the "infinity" string replaces the number.
    pub at_cap: bool,
}

/// The header, including the two Throne-of-Destiny behaviours: a non-ToD account's total-XP display
/// **saturates at 2³²−1**, and its level cap is 126, at which the XP-to-next-level becomes 0 →
/// "infinity".
#[must_use]
pub fn experience_header(
    t: &XpTable,
    total_experience: u64,
    level: i32,
    has_throne_of_destiny: bool,
) -> ExperienceHeader {
    let mut total = total_experience;
    if !has_throne_of_destiny && (total >> 32) != 0 {
        total = u64::from(u32::MAX);
    }
    let this_level =
        experience_to_level(t, usize::try_from(level.max(0)).unwrap_or(0)).unwrap_or(0);
    let next_level =
        experience_to_level(t, usize::try_from(level.max(0)).unwrap_or(0) + 1).unwrap_or(0);
    let mut xp_to_level = next_level.saturating_sub(total);
    let at_cap = !has_throne_of_destiny && level == NON_TOD_LEVEL_CAP;
    if at_cap {
        xp_to_level = 0;
    }
    ExperienceHeader {
        total,
        xp_into_level: total.saturating_sub(this_level),
        xp_to_level,
        at_cap,
    }
}

// ---------------------------------------------------------------------------------------------
// The raise costs over an object's qualities (the records need the `proto` feature).
// ---------------------------------------------------------------------------------------------

#[cfg(feature = "proto")]
pub use inquiries::*;

#[cfg(feature = "proto")]
mod inquiries {
    use super::{
        experience_to_skill_level, max_specialized_skill_level, max_trained_skill_level,
        ten_point_span,
    };
    use crate::quality::QualityRead;
    use crate::skills::Sac;
    use dereth_assets::tables::{SkillTable, XpTable};

    /// What the skill panel's "raise 1" button charges.
    ///
    /// The cost is always measured against `_pp` (experience already sunk), not against the previous
    /// level's threshold, so a partially-filled level is handled for free. An **untrained** skill's
    /// cost is `SkillBase::_trained_cost` in *credits*, not experience.
    #[must_use]
    pub fn skill_cost_to_raise<Q: QualityRead + ?Sized>(
        q: &Q,
        skills: &SkillTable,
        xp: &XpTable,
        id: u32,
    ) -> u32 {
        let Some(s) = q.skill(id) else {
            return skills
                .skills
                .get(&id)
                .map_or(0, |b| u32::try_from(b.trained_cost).unwrap_or(0));
        };
        let sac = Sac::from_raw(s.sac);
        if sac < Sac::Trained {
            return skills
                .skills
                .get(&id)
                .map_or(0, |b| u32::try_from(b.trained_cost).unwrap_or(0));
        }
        let max = if sac == Sac::Trained {
            max_trained_skill_level(xp)
        } else {
            max_specialized_skill_level(xp)
        };
        if u32::from(s.level_from_pp) >= max {
            return 0;
        }
        experience_to_skill_level(xp, sac, usize::from(s.level_from_pp) + 1).saturating_sub(s.pp)
    }

    /// What the skill panel's "raise 10" button charges.
    ///
    /// `n` is `min(10, remaining)` but never below 1 -- [`ten_point_span`], and read its note.
    #[must_use]
    pub fn skill_cost_to_raise_10<Q: QualityRead + ?Sized>(q: &Q, xp: &XpTable, id: u32) -> u32 {
        let Some(s) = q.skill(id) else { return 0 };
        let sac = Sac::from_raw(s.sac);
        if sac < Sac::Trained {
            return 0;
        }
        let max = if sac == Sac::Trained {
            max_trained_skill_level(xp)
        } else {
            max_specialized_skill_level(xp)
        };
        let level = u32::from(s.level_from_pp);
        if level >= max {
            return 0;
        }
        let rem = max - level;
        let n = ten_point_span(rem);
        experience_to_skill_level(xp, sac, (level + n) as usize).saturating_sub(s.pp)
    }
}

#[cfg(test)]
mod tests {
    /// Oracle: the client's six resolved constants —
    /// the player-and-character-state table of them.
    ///
    /// The `0.95` is widened from `f32` because a single-precision load is what reads it, which is
    /// the one place a naive `f64` literal would disagree: `0.95f32 as f64` is
    /// `0.949999988079071`, not `0.95`.
    #[test]
    fn the_vitae_threshold_is_the_binary_s_own_two_powers_and_its_half() {
        let v = f64::from(0.95_f32);
        assert_eq!(vitae_cp_pool_threshold(v, 20.0), 3_476);
        assert_eq!(vitae_cp_pool_threshold(v, 30.0), 9_551);
        // A level of zero leaves only the `20.0` term, scaled: `(0 + 20) * 0.7737… + 0.5`.
        assert_eq!(vitae_cp_pool_threshold(v, 0.0), 15);
        // No penalty is a multiplier of exactly 1.0, and then the second `pow` is the identity.
        assert_eq!(vitae_cp_pool_threshold(1.0, 20.0), 4_492);
        // Monotone in both arguments, which is the property the panel's "earn this much" line
        // depends on.
        assert!(vitae_cp_pool_threshold(v, 40.0) > vitae_cp_pool_threshold(v, 20.0));
        assert!(vitae_cp_pool_threshold(0.60, 20.0) < vitae_cp_pool_threshold(v, 20.0));
    }

    /// The two item-XP functions must be **inverses**, for every curve and every level.
    ///
    /// Oracle: [`item_level_to_total_xp`] and [`item_total_xp_to_level`] are the forward and
    /// reverse of one curve, so round-tripping is a property of the pair rather than a value
    /// anyone had to choose — which is what makes it the right test here.
    ///
    /// It failed before 2026-09-03: curve 2's forward returned `baseXp * 2^(level-1)`, the final
    /// *term*, while the inverse consumed a cumulative series. Base 10 at level 3 produced 40, and
    /// 40 came back as level 2. Each function looked reasonable alone; only together were they
    /// wrong, and nothing compared them.
    #[test]
    fn the_item_xp_curves_and_their_inverse_round_trip() {
        for curve in 1..=3i32 {
            for base in [1u64, 10, 137, 5_000] {
                for level in 1..=20i32 {
                    let total = item_level_to_total_xp(level, base, 100, curve);
                    assert_eq!(
                        item_total_xp_to_level(total, base, 100, curve),
                        level,
                        "curve {curve}, base {base}, level {level} -> {total} did not come back"
                    );
                }
            }
        }
    }

    /// The one worked example, spelled out: curve 2 is the cumulative `base * (2^level - 1)`.
    #[test]
    fn the_geometric_curve_is_cumulative_not_the_final_term() {
        // 10 + 20 + 40 = 70, not 40.
        assert_eq!(item_level_to_total_xp(3, 10, 100, 2), 70);
        assert_eq!(item_total_xp_to_level(70, 10, 100, 2), 3);
        assert_eq!(item_total_xp_to_level(40, 10, 100, 2), 2);
    }

    /// Where retail's count would never finish, this one stops with the levels it reached.
    #[test]
    fn a_zero_base_or_a_doubling_step_that_wraps_to_zero_ends_the_level_count() {
        assert_eq!(item_total_xp_to_level(1000, 0, 200, 2), 0);
        assert_eq!(item_total_xp_to_level(1000, 0, 200, 3), 0);
        // 1 + 2 + ... + 2^63 is exactly u64::MAX, and the next doubling wraps to 0.
        assert_eq!(item_total_xp_to_level(u64::MAX, 1, 200, 2), 64);
    }

    use super::*;
    use dereth_primitives::DataId;

    fn xp_table() -> XpTable {
        XpTable {
            id: DataId(0x0E00_0018),
            attribute_xp: vec![0, 10, 30, 60, 100],
            vital_xp: vec![0, 5, 15, 30, 50],
            trained_xp: vec![0, 100, 300, 700, 1500],
            specialized_xp: vec![0, 50, 150, 350, 750],
            level_xp: vec![0, 1000, 3000, 7000],
            level_credits: vec![0, 0, 1, 1],
        }
    }

    /// Oracle: `12-skills-and-advancement.md` §5 and §11 — the experience-to-skill-level lookup returns
    /// `0xFFFFFFFF`, **not 0**, for a skill that is neither trained nor specialised. This is the
    /// exact trap the rebuild notes call out.
    #[test]
    fn experience_to_skill_level_returns_all_ones_for_an_untrained_skill() {
        let t = xp_table();
        assert_eq!(experience_to_skill_level(&t, Sac::Untrained, 1), u32::MAX);
        assert_eq!(experience_to_skill_level(&t, Sac::Undef, 1), u32::MAX);
        assert_eq!(experience_to_skill_level(&t, Sac::Trained, 2), 300);
        assert_eq!(experience_to_skill_level(&t, Sac::Specialized, 2), 150);
        // The inverse returns 0 rather than all-ones for the same case.
        assert_eq!(skill_level_from_experience(&t, Sac::Untrained, 999), 0);
    }

    #[test]
    fn the_inverse_curves_find_the_highest_threshold_at_or_below() {
        let t = xp_table();
        assert_eq!(skill_level_from_experience(&t, Sac::Trained, 0), 0);
        assert_eq!(skill_level_from_experience(&t, Sac::Trained, 99), 0);
        assert_eq!(skill_level_from_experience(&t, Sac::Trained, 100), 1);
        assert_eq!(skill_level_from_experience(&t, Sac::Trained, 699), 2);
        assert_eq!(skill_level_from_experience(&t, Sac::Trained, 100_000), 4);
        assert_eq!(attribute_level_from_experience(&t, 59), 2);
        assert_eq!(attribute_2nd_level_from_experience(&t, 29), 2);
    }

    /// Oracle: the geometric curve's multiply and accumulating add, as retail computes it.
    ///
    /// This test previously asserted that curve 2 does **not** round-trip, on the reasoning that the
    /// asymmetry was "the client's own inconsistency rather than a transcription error" and should
    /// be preserved. That reasoning was wrong, and it is worth leaving a note about, because the
    /// test was carefully written and confidently argued: the document it transcribed had the
    /// geometric forward as `x += x`, and rather than checking retail, the mismatch with the
    /// inverse was rationalised into a shipped bug and pinned.
    ///
    /// Retail disagrees. Curve type 2's loop has a 64-bit multiply **and** an accumulating add,
    /// i.e. `term *= 2; total += term` — the same shape as curve 3 with `*= 2` for `+= baseXp`. A
    /// self-doubling would have no accumulator. Curve 2 is cumulative, all three curves round-trip, and
    /// `the_item_xp_curves_and_their_inverse_round_trip` now asserts that as a property.
    #[test]
    fn the_item_curves_are_cumulative_and_all_three_round_trip() {
        // Level 4, base 100.
        assert_eq!(item_level_to_total_xp(4, 100, 99, 1), 400, "linear");
        assert_eq!(
            item_level_to_total_xp(4, 100, 99, 2),
            1500,
            "geometric: 100+200+400+800 = 100 * (2^4 - 1)"
        );
        assert_eq!(
            item_level_to_total_xp(4, 100, 99, 3),
            1000,
            "triangular: 100+200+300+400"
        );

        assert_eq!(item_total_xp_to_level(400, 100, 99, 1), 4);
        assert_eq!(item_total_xp_to_level(1500, 100, 99, 2), 4);
        assert_eq!(item_total_xp_to_level(1000, 100, 99, 3), 4);

        assert_eq!(item_level_to_total_xp(0, 100, 99, 1), 0);
        assert_eq!(
            item_level_to_total_xp(50, 100, 10, 1),
            1000,
            "clamped to maxLevel"
        );
        assert_eq!(
            item_total_xp_to_level(999_999, 100, 10, 1),
            10,
            "clamped to maxLevel"
        );
        assert_eq!(
            item_level_to_total_xp(4, 100, 99, 9),
            0,
            "an unknown curve type is zero"
        );
    }

    /// Oracle: the client's curve-1 inverse — an unsigned double quotient, floored, converted by the
    /// runtime helper that stores a 64-bit integer, of which only the low 32 bits are kept, then a
    /// signed `min` against `max_level`. Total XP is a 64-bit quantity, so a quotient past 2^31 is
    /// reachable from the wire.
    ///
    /// Two earlier copies disagreed here and both were wrong: one clamped the out-of-range
    /// conversion to level 0, the other saturated it to `max_level`. The client wraps.
    #[test]
    fn the_linear_inverse_wraps_a_quotient_past_i32_to_its_low_32_bits() {
        let two_31 = 1u64 << 31;
        let two_32 = 1u64 << 32;
        // Just below the edge: an ordinary level, clamped.
        assert_eq!(item_total_xp_to_level(two_31 - 1, 1, 200, 1), 200);
        assert_eq!(item_total_xp_to_level(two_31 - 1, 1, i32::MAX, 1), i32::MAX);
        // 2^31 is bit 31 alone: i32::MIN, which a signed min against any max keeps.
        assert_eq!(item_total_xp_to_level(two_31, 1, 200, 1), i32::MIN);
        assert_eq!(item_total_xp_to_level(two_31 * 10, 10, 200, 1), i32::MIN);
        // 2^32 wraps to 0; 2^32 + 5 to 5.
        assert_eq!(item_total_xp_to_level(two_32, 1, 200, 1), 0);
        assert_eq!(item_total_xp_to_level(two_32 + 5, 1, 200, 1), 5);
        assert_eq!(item_total_xp_to_level((two_32 + 500) * 3, 3, 200, 1), 200);
        // At or past 2^63 the 64-bit conversion is integer-indefinite, whose low half is 0.
        assert_eq!(item_total_xp_to_level(u64::MAX, 1, 200, 1), 0);
        assert_eq!(item_total_xp_to_level(1 << 63, 1, 200, 1), 0);
        // A zero base divides to infinity (or NaN for 0/0): also indefinite, level 0.
        assert_eq!(item_total_xp_to_level(1000, 0, 200, 1), 0);
        assert_eq!(item_total_xp_to_level(0, 0, 200, 1), 0);
        // The clamp is signed, so a negative max_level is returned as is.
        assert_eq!(item_total_xp_to_level(1000, 10, -3, 1), -3);
    }

    /// Oracle: the client's forward curves test `level < 1` **before** clamping to `max_level`, and
    /// curves 2 and 3 return their seed `base_xp` whenever the clamped level is not above 1. So with
    /// `max_level < 1` a positive level gives `base_xp` for curves 2 and 3, and for curve 1 the
    /// sign-extended clamped level times `base_xp`, wrapping.
    #[test]
    fn a_max_level_below_one_clamps_after_the_level_test() {
        for level in [1, 5, i32::MAX] {
            assert_eq!(
                item_level_to_total_xp(level, 100, 0, 1),
                0,
                "curve 1, max 0"
            );
            assert_eq!(
                item_level_to_total_xp(level, 100, 0, 2),
                100,
                "curve 2, max 0"
            );
            assert_eq!(
                item_level_to_total_xp(level, 100, 0, 3),
                100,
                "curve 3, max 0"
            );
            assert_eq!(
                item_level_to_total_xp(level, 100, -3, 1),
                (-300i64).cast_unsigned(),
                "curve 1, max -3: a wrapped negative product"
            );
            assert_eq!(
                item_level_to_total_xp(level, 100, -3, 2),
                100,
                "curve 2, max -3"
            );
            assert_eq!(
                item_level_to_total_xp(level, 100, -3, 3),
                100,
                "curve 3, max -3"
            );
            assert_eq!(
                item_level_to_total_xp(level, 100, 0, 7),
                0,
                "an unknown curve"
            );
        }
        // The `level < 1` test still comes first.
        assert_eq!(item_level_to_total_xp(0, 100, 0, 2), 0);
        assert_eq!(item_level_to_total_xp(-1, 100, 10, 3), 0);
    }

    /// Oracle: the client's 64-bit multiplies and adds are plain, wrapping ones.
    #[test]
    fn the_item_curves_wrap_rather_than_saturate() {
        let base = 1u64 << 62;
        // Curve 2 at level 3: base + 2*base + 4*base = 7 * 2^62, wrapped.
        assert_eq!(item_level_to_total_xp(3, base, 10, 2), base.wrapping_mul(7));
        // Curve 1: level 5 times 2^62, wrapped.
        assert_eq!(item_level_to_total_xp(5, base, 10, 1), base.wrapping_mul(5));
        // Curve 3 at level 2: base + 2*base.
        assert_eq!(item_level_to_total_xp(2, base, 10, 3), base.wrapping_mul(3));
        // The inverse's wrapped step: base 2^63 and a full remainder give a step of 0 after one
        // round of curve 3, which then consumes nothing and restores the step to base.
        assert_eq!(item_total_xp_to_level(u64::MAX, 1 << 63, 10, 3), 2);
    }

    /// Oracle: §8's — the saturation and the level-126 cap.
    #[test]
    fn a_non_throne_of_destiny_account_saturates_and_caps() {
        let t = xp_table();
        let h = experience_header(&t, 0x1_0000_0000, 1, false);
        assert_eq!(
            h.total,
            u64::from(u32::MAX),
            "the total-XP display saturates at 2^32 - 1"
        );
        let h = experience_header(&t, 0x1_0000_0000, 1, true);
        assert_eq!(
            h.total, 0x1_0000_0000,
            "a Throne-of-Destiny account does not saturate"
        );

        let h = experience_header(&t, 500, NON_TOD_LEVEL_CAP, false);
        assert_eq!(h.xp_to_level, 0);
        assert!(h.at_cap, "level 126 renders the infinity string");
        let h = experience_header(&t, 500, NON_TOD_LEVEL_CAP, true);
        assert!(!h.at_cap);

        let h = experience_header(&t, 1500, 1, true);
        assert_eq!(h.xp_into_level, 500);
        assert_eq!(h.xp_to_level, 1500);
    }
}
