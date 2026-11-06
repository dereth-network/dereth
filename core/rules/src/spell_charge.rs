//! A character's charge on a spell: what casting it spends and time gives back.
//!
//! The server decides what spends charge; the client follows with the same numbers.

/// The least a charge can be.
pub const MIN: f32 = 0.5;
/// The most a charge can be, and what a newly learned spell has.
pub const MAX: f32 = 1.0;
/// What a charge regains each second.
pub const REGEN_PER_SECOND: f32 = 0.008_333;
/// What a successful cast multiplies a charge by.
pub const DECAY: f32 = 0.92;

/// A charge after `seconds` have passed.
#[must_use]
pub fn regenerated(charge: f32, seconds: f64) -> f32 {
    #[allow(clippy::cast_possible_truncation)]
    let gained = (f64::from(REGEN_PER_SECOND) * seconds.max(0.0)) as f32;
    (charge + gained).clamp(MIN, MAX)
}

/// A charge after a successful cast.
#[must_use]
pub fn decayed(charge: f32) -> f32 {
    (charge * DECAY).clamp(MIN, MAX)
}

/// A charge as a whole percent, as the panels show it.
#[must_use]
pub fn percent(charge: f32) -> u32 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let percent = (charge * 100.0) as u32;
    percent
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the arithmetic)
    use super::*;

    #[test]
    fn casting_spends_a_charge_and_time_gives_it_back_within_its_limits() {
        assert!((decayed(1.0) - 0.92).abs() < 1e-6);
        assert!((decayed(0.5) - 0.5).abs() < 1e-6, "the least it can be");
        assert!((regenerated(0.5, 60.0) - 0.999_98).abs() < 1e-3);
        assert!((regenerated(0.9, 1000.0) - 1.0).abs() < 1e-6, "the most");
        assert!((regenerated(0.9, -5.0) - 0.9).abs() < 1e-6, "no time");
        assert_eq!(percent(0.94), 94);
    }
}
