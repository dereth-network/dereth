// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/World/RecipeModsFloat.cs
//! `RecipeModsFloat`: one row of the world-database table `recipe_mods_float`.

/// Recipe Float Mods
// ACE: RecipeModsFloat
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RecipeModsFloat {
    /// Unique Id of this Recipe Mod instance
    pub id: u32,
    /// Unique Id of Recipe Mod
    pub recipe_mod_id: u32,
    pub index: i8,
    pub stat: i32,
    pub value: f64,
    pub r#enum: i32,
    pub source: i32,
    // ACE: RecipeModsFloat.RecipeMod navigates back to the parent row, not carried.
}
