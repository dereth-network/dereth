// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/World/RecipeRequirementsString.cs
//! `RecipeRequirementsString`: one row of the world-database table `recipe_requirements_string`.

/// Recipe String Requirments
// ACE: RecipeRequirementsString
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RecipeRequirementsString {
    /// Unique Id of this Recipe Requirement instance
    pub id: u32,
    /// Unique Id of Recipe
    pub recipe_id: u32,
    pub index: i8,
    pub stat: i32,
    pub value: Option<String>,
    pub r#enum: i32,
    pub message: Option<String>,
    // ACE: RecipeRequirementsString.Recipe navigates back to the parent row, not carried.
}
