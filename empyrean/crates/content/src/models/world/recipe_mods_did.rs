// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/World/RecipeModsDID.cs
//! `RecipeModsDID`: one row of the world-database table `recipe_mods_d_i_d`.

/// Recipe DID Mods
// ACE: RecipeModsDID
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RecipeModsDID {
    /// Unique Id of this Recipe Mod instance
    pub id: u32,
    /// Unique Id of Recipe Mod
    pub recipe_mod_id: u32,
    pub index: i8,
    pub stat: i32,
    pub value: u32,
    pub r#enum: i32,
    pub source: i32,
    // ACE: RecipeModsDID.RecipeMod navigates back to the parent row, not carried.
}
