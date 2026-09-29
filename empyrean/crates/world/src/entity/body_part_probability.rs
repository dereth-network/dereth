// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/BodyPartProbability.cs
//! Port of `Source/ACE.Server/Entity/BodyPartProbability.cs`.

use empyrean_entity::enums::CombatBodyPart;

/// One body part and its hit weight in one quadrant.
// ACE: BodyPartProbability
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BodyPartProbability {
    // ACE: BodyPartProbability.BodyPart
    pub body_part: CombatBodyPart,
    // ACE: BodyPartProbability.Probability
    pub probability: f32,
}

impl BodyPartProbability {
    // ACE: BodyPartProbability.BodyPartProbability
    #[must_use]
    pub fn new(body_part: CombatBodyPart, probability: f32) -> Self {
        Self {
            body_part,
            probability,
        }
    }
}
