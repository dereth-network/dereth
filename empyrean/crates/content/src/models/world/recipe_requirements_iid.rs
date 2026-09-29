// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/World/RecipeRequirementsIID.cs
//! `RecipeRequirementsIID`: one row of the world-database table `recipe_requirements_i_i_d`.

/// Recipe IID Requirments
// ACE: RecipeRequirementsIID
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RecipeRequirementsIID {
    /// Unique Id of this Recipe Requirement instance
    pub id: u32,
    /// Unique Id of Recipe
    pub recipe_id: u32,
    pub index: i8,
    pub stat: i32,
    pub value: u32,
    pub r#enum: i32,
    pub message: Option<String>,
    // ACE: RecipeRequirementsIID.Recipe navigates back to the parent row, not carried.
}
