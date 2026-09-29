// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/Mutations/MutationFilter.cs
//! Port of `Source/ACE.Server/Entity/Mutations/MutationFilter.cs`.

use empyrean_common::thread_safe_random::ThreadSafeRandom;

use super::mutation::Mutation;
use crate::world_objects::world_object::WorldObject;

/// ACE `MutationFilter`: one compiled mutation script.
///
/// MutationFilter -> Mutation -> MutationOutcome -> EffectList -> Effect
// ACE: MutationFilter
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MutationFilter {
    pub mutations: Vec<Mutation>,
}

impl MutationFilter {
    /// Draws once, then offers that draw to every mutation in order. ACE's `tier` defaults to 1.
    // ACE: MutationFilter.TryMutate
    pub fn try_mutate(&self, wo: &mut WorldObject, tier: i32) -> bool {
        let rng = ThreadSafeRandom::next_float(0.0, 1.0);

        let mut mutated = false;

        for mutation in &self.mutations {
            mutated |= mutation.try_mutate(wo, tier, rng);
        }

        mutated
    }
}
