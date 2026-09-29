// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/Mutations/MutationOutcome.cs
//! Port of `Source/ACE.Server/Entity/Mutations/MutationOutcome.cs`.

use super::effect_list::EffectList;
use crate::world_objects::world_object::WorldObject;

/// ACE `MutationOutcome`: a group of effect lists whose chances add up to 1.
// ACE: MutationOutcome
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MutationOutcome {
    pub effect_lists: Vec<EffectList>,
}

impl MutationOutcome {
    /// Applies the first effect list whose cumulative chance exceeds `rng` (a `double` compared
    /// with the `float` chance, as in ACE).
    // ACE: MutationOutcome.TryMutate
    pub fn try_mutate(&self, wo: &mut WorldObject, rng: f64) -> bool {
        for effect_list in &self.effect_lists {
            if rng < f64::from(effect_list.chance) {
                return effect_list.try_mutate(wo);
            }
        }
        false
    }
}
