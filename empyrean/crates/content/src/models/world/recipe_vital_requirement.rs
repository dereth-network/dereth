//! Optional crafting preconditions, separate from ACE's recipe representation.

/// Current player vitals required before attempting a recipe. Costs remain in its modifications.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RecipeVitalRequirement {
    /// Recipe id.
    pub id: u32,
    pub health: u32,
    pub stamina: u32,
    pub mana: u32,
}

crate::impl_codec!(RecipeVitalRequirement {
    id,
    health,
    stamina,
    mana
});
crate::impl_from_row!(RecipeVitalRequirement, "recipe_requirements_vital" {
    id: "id", health: "health", stamina: "stamina", mana: "mana"
});
