// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/World/RecipeRequirementsFloat.cs
//! `RecipeRequirementsFloat`: one row of the world-database table `recipe_requirements_float`.

/// Recipe Float Requirments
// ACE: RecipeRequirementsFloat
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RecipeRequirementsFloat {
    /// Unique Id of this Recipe Requirement instance
    pub id: u32,
    /// Unique Id of Recipe
    pub recipe_id: u32,
    pub index: i8,
    pub stat: i32,
    pub value: f64,
    pub r#enum: i32,
    pub message: Option<String>,
    // ACE: RecipeRequirementsFloat.Recipe navigates back to the parent row, not carried.
}
