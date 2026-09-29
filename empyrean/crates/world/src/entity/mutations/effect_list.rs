// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/Mutations/EffectList.cs
//! Port of `Source/ACE.Server/Entity/Mutations/EffectList.cs`.

use super::effect::Effect;
use crate::world_objects::world_object::WorldObject;

/// ACE `EffectList`: the effects of one `- Chance: n%:` block. `chance` is the running total of
/// the block's outcome, so a list is picked by the first `rng < chance`.
// ACE: EffectList
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EffectList {
    pub chance: f32,
    pub effects: Vec<Effect>,
}

impl EffectList {
    /// Applies every effect; true if any applied.
    // ACE: EffectList.TryMutate
    pub fn try_mutate(&self, wo: &mut WorldObject) -> bool {
        let mut mutated = false;

        for effect in &self.effects {
            mutated |= effect.try_mutate(wo); // stop completely on failure?
        }

        mutated
    }
}
