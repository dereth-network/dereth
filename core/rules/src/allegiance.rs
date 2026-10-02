//! Allegiance rules: the experience an oath costs on a world that charges for one.
//!
//! Before Throne of Destiny an oath cost unassigned experience, and the clients of that time drew
//! the cost on the allegiance panel. A character who has never broken from a patron swears free.

/// The experience an oath costs a character whose next level is `level_span` experience away from
/// its current one (the curve's step from its level to the next), after `breaks` earlier breaks.
///
/// Five percent of the step, held between 100 and 5,000, then a quarter more for each break,
/// rounded half up.
#[must_use]
pub fn swear_xp_cost(level_span: u64, breaks: u32) -> u32 {
    #[allow(clippy::cast_precision_loss)] // a step of a few million is exact in an f64
    let base = (level_span as f64 * f64::from(0.05_f32)).clamp(100.0, 5000.0);
    u32::try_from(dereth_primitives::num::to_i64_f64(
        base * (1.0 + 0.25 * f64::from(breaks)) + 0.5,
    ))
    .unwrap_or(u32::MAX)
}

/// The cost of an oath on a world that charges for one: nothing for a character with no break,
/// otherwise [`swear_xp_cost`].
#[must_use]
pub fn swear_xp_cost_after_breaks(level_span: u64, breaks: u32) -> u32 {
    if breaks == 0 {
        0
    } else {
        swear_xp_cost(level_span, breaks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The step is held between 100 and 5,000 before the quarter per break is added, so a
    /// high-level character with breaks pays more than 5,000.
    #[test]
    fn the_cost_is_five_percent_of_the_level_step_held_to_100_and_5000_then_a_quarter_per_break() {
        assert_eq!(swear_xp_cost(0, 0), 100);
        assert_eq!(swear_xp_cost(20_000, 0), 1_000);
        assert_eq!(swear_xp_cost(20_000, 1), 1_250);
        assert_eq!(swear_xp_cost(2_010, 0), 101, "100.5 rounds up");
        assert_eq!(swear_xp_cost(1_000_000, 4), 10_000);
        // Level 126 on the February 2005 curve: the step past the curve's end is huge.
        assert_eq!(swear_xp_cost(8_358_197, 1), 6_250);
    }

    #[test]
    fn a_character_with_no_break_swears_free() {
        assert_eq!(swear_xp_cost_after_breaks(20_000, 0), 0);
        assert_eq!(swear_xp_cost_after_breaks(20_000, 2), 1_500);
    }
}
