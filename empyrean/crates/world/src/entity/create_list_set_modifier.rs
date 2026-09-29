// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/CreateListSetModifier.cs
//! Port of `Source/ACE.Server/Entity/CreateListSetModifier.cs`: a treasure set's trophy chance
//! scaled by a drop-rate modifier, capped at certainty.

use crate::entity::create_list_set::CreateListSet;

// ACE: CreateListSetModifier
#[derive(Debug, Clone, PartialEq)]
pub struct CreateListSetModifier {
    // ACE: CreateListSetModifier.Set
    pub set: Option<CreateListSet>,
    // ACE: CreateListSetModifier.Modifier
    pub modifier: f32,
    /// Usually Modifier, unless Set.TrophyProbability * Modifier > 1.0, in which case TrophyMod
    /// is capped so Set.TrophyProbability * TrophyMod = 1.0.
    // ACE: CreateListSetModifier.TrophyMod
    pub trophy_mod: f32,
}

impl Default for CreateListSetModifier {
    // ACE: CreateListSetModifier.CreateListSetModifier
    /// `new CreateListSetModifier()`: a modifier of 1 and no set.
    fn default() -> Self {
        Self {
            set: None,
            modifier: 1.0,
            trophy_mod: 0.0,
        }
    }
}

impl CreateListSetModifier {
    // ACE: CreateListSetModifier.CreateListSetModifier
    #[must_use]
    pub fn new(set: CreateListSet, modifier: f32) -> Self {
        // calculate TrophyMod
        let trophy_probability = set.trophy_probability();

        let trophy_mod = if trophy_probability * modifier > 1.0 {
            1.0 / trophy_probability
        } else {
            modifier
        };

        Self {
            set: Some(set),
            modifier,
            trophy_mod,
        }
    }

    fn set(&self) -> &CreateListSet {
        self.set
            .as_ref()
            .expect("ACE: Set is null (NullReferenceException)")
    }

    // ACE: CreateListSetModifier.NoneMod
    #[must_use]
    pub fn none_mod(&self) -> f32 {
        self.none_probability() / self.set().none_probability()
    }

    // ACE: CreateListSetModifier.TrophyProbability
    #[must_use]
    pub fn trophy_probability(&self) -> f32 {
        self.set().trophy_probability() * self.trophy_mod
    }

    // ACE: CreateListSetModifier.NoneProbability
    #[must_use]
    pub fn none_probability(&self) -> f32 {
        1.0 - self.trophy_probability()
    }
}
