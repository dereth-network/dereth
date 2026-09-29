// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/World/RecipeMod.cs
//! `RecipeMod`: one row of the world-database table `recipe_mod`.

use super::{
    RecipeModsBool, RecipeModsDID, RecipeModsFloat, RecipeModsIID, RecipeModsInt, RecipeModsString,
};

/// Recipe Mods
// ACE: RecipeMod
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RecipeMod {
    /// Unique Id of this Recipe Mod instance
    pub id: u32,
    /// Unique Id of Recipe
    pub recipe_id: u32,
    pub executes_on_success: bool,
    pub health: i32,
    pub stamina: i32,
    pub mana: i32,
    pub unknown_7: bool,
    pub data_id: i32,
    pub unknown_9: i32,
    pub instance_id: i32,
    // ACE: RecipeMod.Recipe navigates back to the parent row, not carried.
    pub recipe_mods_bool: Vec<RecipeModsBool>,
    pub recipe_mods_did: Vec<RecipeModsDID>,
    pub recipe_mods_float: Vec<RecipeModsFloat>,
    pub recipe_mods_iid: Vec<RecipeModsIID>,
    pub recipe_mods_int: Vec<RecipeModsInt>,
    pub recipe_mods_string: Vec<RecipeModsString>,
}
