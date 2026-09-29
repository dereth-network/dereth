// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/World/Recipe.cs
//! `Recipe`: one row of the world-database table `recipe`.

use empyrean_common::dotnet::DotNetDateTime;

use super::{
    RecipeMod, RecipeRequirementsBool, RecipeRequirementsDID, RecipeRequirementsFloat,
    RecipeRequirementsIID, RecipeRequirementsInt, RecipeRequirementsString,
};

/// Recipes
// ACE: Recipe
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Recipe {
    /// Unique Id of this Recipe
    pub id: u32,
    pub unknown_1: u32,
    pub skill: u32,
    pub difficulty: u32,
    pub salvage_type: u32,
    /// Weenie Class Id of object to create upon successful application of this recipe
    pub success_wcid: u32,
    /// Amount of objects to create upon successful application of this recipe
    pub success_amount: u32,
    pub success_message: Option<String>,
    /// Weenie Class Id of object to create upon failing application of this recipe
    pub fail_wcid: u32,
    /// Amount of objects to create upon failing application of this recipe
    pub fail_amount: u32,
    pub fail_message: Option<String>,
    pub success_destroy_source_chance: f64,
    pub success_destroy_source_amount: u32,
    pub success_destroy_source_message: Option<String>,
    pub success_destroy_target_chance: f64,
    pub success_destroy_target_amount: u32,
    pub success_destroy_target_message: Option<String>,
    pub fail_destroy_source_chance: f64,
    pub fail_destroy_source_amount: u32,
    pub fail_destroy_source_message: Option<String>,
    pub fail_destroy_target_chance: f64,
    pub fail_destroy_target_amount: u32,
    pub fail_destroy_target_message: Option<String>,
    pub data_id: u32,
    pub last_modified: DotNetDateTime,
    // ACE: Recipe.CookBook is an inverse navigation, not carried: nothing reads it.
    pub recipe_mod: Vec<RecipeMod>,
    pub recipe_requirements_bool: Vec<RecipeRequirementsBool>,
    pub recipe_requirements_did: Vec<RecipeRequirementsDID>,
    pub recipe_requirements_float: Vec<RecipeRequirementsFloat>,
    pub recipe_requirements_iid: Vec<RecipeRequirementsIID>,
    pub recipe_requirements_int: Vec<RecipeRequirementsInt>,
    pub recipe_requirements_string: Vec<RecipeRequirementsString>,
}
