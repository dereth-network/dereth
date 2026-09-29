// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/Probability.cs
//! Port of `Source/ACE.Server/Entity/Probability.cs`.

// ACE: Probability.GetProbabilityNone
/// The probability of none of the events occurring, from a list of chances.
#[must_use]
pub fn get_probability_none(chances: &[f32]) -> f32 {
    let mut probability = 1.0f32;

    for &chance in chances {
        probability *= 1.0 - chance;
    }

    probability
}

// ACE: Probability.GetProbabilityAny
/// The probability of any of the events occurring, from a list of chances.
#[must_use]
pub fn get_probability_any(chances: &[f32]) -> f32 {
    1.0 - get_probability_none(chances)
}
