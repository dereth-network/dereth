// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/World/CookBook.cs
//! `CookBook`: one row of the world-database table `cook_book`.

use std::sync::Arc;

use empyrean_common::dotnet::DotNetDateTime;

use super::Recipe;

/// Cook Book for Recipes
// ACE: CookBook
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CookBook {
    /// Unique Id of this cook book instance
    pub id: u32,
    /// Unique Id of Recipe
    pub recipe_id: u32,
    /// Weenie Class Id of the source object for this recipe
    pub source_wcid: u32,
    /// Weenie Class Id of the target object for this recipe
    pub target_wcid: u32,
    pub last_modified: DotNetDateTime,
    pub recipe: Option<Arc<Recipe>>,
}
