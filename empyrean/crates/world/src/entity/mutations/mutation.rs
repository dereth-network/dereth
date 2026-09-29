// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/Mutations/Mutation.cs
//! Port of `Source/ACE.Server/Entity/Mutations/Mutation.cs`.

use empyrean_common::thread_safe_random::ThreadSafeRandom;

use super::mutation_outcome::MutationOutcome;
use crate::world_objects::world_object::WorldObject;

/// ACE `Mutation`: one `Mutation #n` block: a chance per tier and its outcomes.
// ACE: Mutation
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mutation {
    pub chances: Vec<f32>,

    pub outcomes: Vec<MutationOutcome>,
}

impl Mutation {
    /// Passes when `rng` (the filter's one draw) is below the tier's chance; then draws once more
    /// and applies every outcome with that second draw.
    // ACE: Mutation.TryMutate
    pub fn try_mutate(&self, wo: &mut WorldObject, mut tier: i32, rng: f64) -> bool {
        let count = i32::try_from(self.chances.len()).expect("a C# list count fits an int");

        // if at least 6 tiers are defined,
        // if we are rolling for a higher tier,
        // fall back on highest tier?
        if count >= 6 && tier > count {
            tier = count;
        }

        if tier < 1 || tier > count {
            return false;
        }

        // does it pass the roll to mutate for the tier?
        #[allow(clippy::cast_sign_loss)]
        if rng >= f64::from(self.chances[(tier - 1) as usize]) {
            return false;
        }

        // roll again to select the mutations
        let rng = ThreadSafeRandom::next_float(0.0, 1.0);

        let mut mutated = false;
        for outcome in &self.outcomes {
            mutated |= outcome.try_mutate(wo, rng);
        }

        mutated
    }
}
