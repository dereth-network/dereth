// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/World/RecipeRequirementsBool.cs
//! `RecipeRequirementsBool`: one row of the world-database table `recipe_requirements_bool`.

/// Recipe Bool Requirments
// ACE: RecipeRequirementsBool
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RecipeRequirementsBool {
    /// Unique Id of this Recipe Requirement instance
    pub id: u32,
    /// Unique Id of Recipe
    pub recipe_id: u32,
    pub index: i8,
    pub stat: i32,
    pub value: bool,
    pub r#enum: i32,
    pub message: Option<String>,
    // ACE: RecipeRequirementsBool.Recipe navigates back to the parent row, not carried.
}
