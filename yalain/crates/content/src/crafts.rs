//! What the world database says of crafting's own work: the surveyor's map recipes each region's
//! drawing sets waiting, the totem spellbinding makes of each sort of item, and what each refining
//! recipe yields.

use std::collections::BTreeMap;

use crate::treasure_tables::Sort;

/// A region's surveyor's map recipes: drawing one has them all wait, a find of one of the region's
/// maps lets them be drawn again.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MapRecipes {
    /// The region, a key.
    pub region: String,
    /// The effect a map drawn puts on its drawer, which sets the wait (its saved state).
    pub drawn: u32,
    /// The name of the effect a find of one of the region's maps puts on, which ends it.
    pub found: String,
    /// The recipes, in the order their call lists them.
    pub recipes: Vec<u32>,
    /// How long they wait after a drawing, in seconds of game time (none: the server's rule).
    pub wait: Option<f64>,
}

/// The totem spellbinding makes of a sort of item, and the word it says it goes on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Totem {
    pub sort: Sort,
    pub template: u32,
    pub word: u32,
}

/// What a refining recipe yields: by its script package, the template of its product, and the craft
/// skill level at which it yields all it takes (none: the server's rule).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Refining {
    pub recipe: String,
    pub product: u32,
    pub full_at: Option<u32>,
}

/// The world's crafting work: map recipes by region, totems by sort, refinings by recipe.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Crafts {
    pub map_recipes: BTreeMap<String, MapRecipes>,
    pub totems: Vec<Totem>,
    pub refining: BTreeMap<String, Refining>,
}

impl Crafts {
    /// The map recipes a drawing's effect sets waiting, by the effect's saved state.
    #[must_use]
    pub fn map_recipes_drawn(&self, effect: u32) -> Option<&MapRecipes> {
        self.map_recipes.values().find(|m| m.drawn == effect)
    }

    /// The region whose map recipes a map's find lets be drawn again, by its effect's name.
    #[must_use]
    pub fn map_recipes_found(&self, effect: &str) -> Option<&MapRecipes> {
        self.map_recipes.values().find(|m| m.found == effect)
    }

    /// The totem spellbinding makes of a sort of item.
    #[must_use]
    pub fn totem(&self, sort: Sort) -> Option<&Totem> {
        self.totems.iter().find(|t| t.sort == sort)
    }

    /// What a totem of a template says it goes on: the word for it. None for any other template.
    #[must_use]
    pub fn totem_word(&self, template: u32) -> Option<u32> {
        self.totems
            .iter()
            .find(|t| t.template == template)
            .map(|t| t.word)
    }

    /// Every template crafting's work makes: the totems and the refined products.
    pub fn templates(&self) -> impl Iterator<Item = u32> + '_ {
        self.totems
            .iter()
            .map(|t| t.template)
            .chain(self.refining.values().map(|r| r.product))
    }
}
