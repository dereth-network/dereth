// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/World/RecipeModsBool.cs
//! `RecipeModsBool`: one row of the world-database table `recipe_mods_bool`.

/// Recipe Bool Mods
// ACE: RecipeModsBool
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RecipeModsBool {
    /// Unique Id of this Recipe Mod instance
    pub id: u32,
    /// Unique Id of Recipe Mod
    pub recipe_mod_id: u32,
    pub index: i8,
    pub stat: i32,
    pub value: bool,
    pub r#enum: i32,
    pub source: i32,
    // ACE: RecipeModsBool.RecipeMod navigates back to the parent row, not carried.
}
