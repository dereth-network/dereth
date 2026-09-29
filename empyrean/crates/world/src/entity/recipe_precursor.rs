// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/RecipePrecursor.cs
//! Port of `Source/ACE.Server/Entity/RecipePrecursor.cs`.

/// One row of ACE's `json\recipeprecursors.json`: tool -> target -> recipe.
// ACE: RecipePrecursor
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RecipePrecursor {
    // ACE: RecipePrecursor.Tool
    pub tool: u32,
    // ACE: RecipePrecursor.Target
    pub target: u32,
    // ACE: RecipePrecursor.RecipeID
    pub recipe_id: u32,
}
