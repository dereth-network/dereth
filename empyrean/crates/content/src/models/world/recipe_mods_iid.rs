// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/World/RecipeModsIID.cs
//! `RecipeModsIID`: one row of the world-database table `recipe_mods_i_i_d`.

/// Recipe IID Mods
// ACE: RecipeModsIID
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RecipeModsIID {
    /// Unique Id of this Recipe Mod instance
    pub id: u32,
    /// Unique Id of Recipe Mod
    pub recipe_mod_id: u32,
    pub index: i8,
    pub stat: i32,
    pub value: u32,
    pub r#enum: i32,
    pub source: i32,
    // ACE: RecipeModsIID.RecipeMod navigates back to the parent row, not carried.
}
