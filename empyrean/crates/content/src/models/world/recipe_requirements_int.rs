// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/World/RecipeRequirementsInt.cs
//! `RecipeRequirementsInt`: one row of the world-database table `recipe_requirements_int`.

/// Recipe Int Requirments
// ACE: RecipeRequirementsInt
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RecipeRequirementsInt {
    /// Unique Id of this Recipe Requirement instance
    pub id: u32,
    /// Unique Id of Recipe
    pub recipe_id: u32,
    pub index: i8,
    pub stat: i32,
    pub value: i32,
    pub r#enum: i32,
    pub message: Option<String>,
    // ACE: RecipeRequirementsInt.Recipe navigates back to the parent row, not carried.
}
