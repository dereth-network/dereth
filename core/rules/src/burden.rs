//! `EncumbranceSystem` — burden, capacity and the load thresholds.
//!
//! The client shows burden and uses load for local movement/jump permission and scaling.
//! Server authority is separate; these are the client's own inquiry kernels.

/// The encumbrance capacity.
///
/// `\[verified\]` identical to ACE `ACE.Server/Physics/Common/EncumbranceSystem.cs`.
#[must_use]
pub fn encumbrance_capacity(strength: i32, num_augs: i32) -> i32 {
    if strength < 1 {
        return 0;
    }
    let bonus = num_augs.saturating_mul(30);
    if bonus < 0 {
        // A negative aug count means no bonus, not a penalty.
        return strength.saturating_mul(150);
    }
    let bonus = bonus.min(150); // capped at +100%
    bonus
        .saturating_mul(strength)
        .saturating_add(strength.saturating_mul(150))
}

/// The load fraction.
#[must_use]
pub fn load(capacity: i32, burden: i32) -> f32 {
    if capacity < 1 {
        // No capacity: maximally overloaded.
        return 3.0;
    }
    if burden < 0 {
        return 0.0;
    }
    #[allow(clippy::cast_precision_loss)] // the client's own int-to-float conversion
    {
        burden as f32 / capacity as f32
    }
}

/// Behavior: the movement multiplier physics applies.
#[must_use]
pub fn load_mod(load: f32) -> f32 {
    if load < 1.0 {
        1.0
    } else if load < 2.0 {
        2.0 - load // linear falloff
    } else {
        0.0 // cannot move
    }
}

/// Which encumbrance threshold a load sits in. emits a
/// message when the boundary is crossed in either direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadBand {
    Normal,
    Encumbered,
    OverBurdened,
}

#[must_use]
pub fn load_band(load: f32) -> LoadBand {
    if load < 1.0 {
        LoadBand::Normal
    } else if load < 2.0 {
        LoadBand::Encumbered
    } else {
        LoadBand::OverBurdened
    }
}

/// Behavior: the burden ratio the bar draws.
///
/// Strength defaults to 10 when absent, as the client does; `EncumbranceVal` is int property 5 and
/// `AugmentationIncreasedCarryingCapacity` is int property 230.
#[cfg(feature = "proto")]
#[must_use]
pub fn inq_load<Q: crate::quality::QualityRead + ?Sized>(q: &Q) -> f32 {
    // The capacity inquiry reads Strength enchanted (raw = false): a live Strength enchantment
    // changes capacity.
    let strength = crate::attributes::inq_attribute(q, 1, false)
        .map_or(10, |strength| i32::try_from(strength).unwrap_or(i32::MAX));
    let augs = q.inq_int(230);
    let capacity = encumbrance_capacity(strength, augs);
    let burden = q.inq_int(5);
    load(capacity, burden)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the recovered qualities behavior §4, which prints all three
    /// functions verbatim from the recovered encumbrance implementation.
    #[test]
    fn capacity_is_150_per_strength_plus_30_per_aug_capped_at_150() {
        assert_eq!(
            encumbrance_capacity(0, 0),
            0,
            "strength < 1 has no capacity at all"
        );
        assert_eq!(encumbrance_capacity(100, 0), 15_000);
        assert_eq!(encumbrance_capacity(100, 1), 100 * 30 + 15_000);
        assert_eq!(
            encumbrance_capacity(100, 5),
            100 * 150 + 15_000,
            "capped at +100%"
        );
        assert_eq!(encumbrance_capacity(100, 50), 100 * 150 + 15_000);
        assert_eq!(
            encumbrance_capacity(100, -1),
            15_000,
            "a negative aug count gives no bonus"
        );
    }

    #[test]
    fn load_saturates_at_three_with_no_capacity_and_at_zero_with_negative_burden() {
        assert_eq!(load(0, 500), 3.0);
        assert_eq!(load(15_000, -1), 0.0);
        assert_eq!(load(15_000, 7_500), 0.5);
        assert_eq!(load(15_000, 30_000), 2.0);
    }

    #[test]
    fn load_mod_falls_linearly_between_one_and_two() {
        assert_eq!(load_mod(0.0), 1.0);
        assert_eq!(load_mod(0.999), 1.0);
        assert_eq!(load_mod(1.0), 1.0);
        assert_eq!(load_mod(1.5), 0.5);
        assert_eq!(load_mod(2.0), 0.0);
        assert_eq!(load_mod(10.0), 0.0);
    }

    /// Oracle: the burden threshold table — normal below a load of 1.0, a penalty band up to 2.0,
    /// and no movement at or beyond it.
    #[test]
    fn the_three_bands_are_bounded_at_one_and_two() {
        assert_eq!(load_band(0.9), LoadBand::Normal);
        assert_eq!(load_band(1.0), LoadBand::Encumbered);
        assert_eq!(load_band(1.99), LoadBand::Encumbered);
        assert_eq!(load_band(2.0), LoadBand::OverBurdened);
    }
}
