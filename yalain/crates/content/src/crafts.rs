//! What the world database says of crafting's own work: the surveyor's map recipes each region's
//! drawing sets waiting, the totem spellbinding makes of each sort of item, what each refining
//! recipe yields, the recipes the world defines itself, and what a recipe that works out its own
//! product makes of a set of ingredients; and the steps and gives that wait on a player's craft
//! skill.

use std::collections::BTreeMap;

use crate::quests::{Give, Step};
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

/// What a refining recipe yields: by its script package, the template of its product, the craft
/// skill level at which it yields all it takes, and the percent of it it yields before any skill
/// (none: the server's rule).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Refining {
    pub recipe: String,
    pub product: u32,
    pub full_at: Option<u32>,
    pub start: Option<u32>,
}

/// A recipe the world defines, where the install holds none: an install recipe's saved state (its
/// craft skill, category, animation, checks and the like) with its name, description, ingredients
/// and product replaced, under an id of its own.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Recipe {
    /// Its data id, which no record of the install holds.
    pub recipe: u32,
    /// The install recipe it is made from.
    pub like: u32,
    /// Its name and description, as string references `(string, table)` (no description: none).
    pub name: (u32, u32),
    pub description: Option<(u32, u32)>,
    /// What it takes, slot by slot.
    pub ingredients: Vec<RecipeIngredient>,
    /// The template of what it makes, and how many.
    pub product: u32,
    pub count: u32,
    /// Its craft skill (a saved state) and its category (none: the install recipe's).
    pub skill: Option<u32>,
    pub category: Option<u32>,
}

/// One of a world recipe's slots: the templates any of which goes in it, how many it takes, and
/// what the slot says it wants (none: nothing).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RecipeIngredient {
    pub items: Vec<u32>,
    pub count: u32,
    pub name: Option<(u32, u32)>,
}

/// What a recipe that works out its own product makes of one set of ingredients: the recipe (an
/// install recipe's saved state), the templates its slots hold (one a slot, in any order; a
/// template twice where it takes two), and the template of what it makes, how many.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Combination {
    pub key: String,
    pub recipe: u32,
    pub items: Vec<u32>,
    pub product: u32,
    pub count: u32,
}

impl Combination {
    /// Do these templates, one a slot in any order, make this combination?
    #[must_use]
    pub fn made_of(&self, templates: &[u32]) -> bool {
        let mut want = self.items.clone();
        let mut have = templates.to_vec();
        want.sort_unstable();
        have.sort_unstable();
        want == have
    }
}

/// The world's crafting work: map recipes by region, totems by sort, refinings by recipe, the
/// world's own recipes by id, the combinations recipes that work out their own product make, in
/// the rows' order.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Crafts {
    pub map_recipes: BTreeMap<String, MapRecipes>,
    pub totems: Vec<Totem>,
    pub refining: BTreeMap<String, Refining>,
    pub recipes: BTreeMap<u32, Recipe>,
    pub combinations: Vec<Combination>,
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

    /// What a recipe that works out its own product makes of the templates its slots hold: the
    /// first combination of the recipe they make.
    #[must_use]
    pub fn combination(&self, recipe: u32, templates: &[u32]) -> Option<&Combination> {
        self.combinations
            .iter()
            .find(|c| c.recipe == recipe && c.made_of(templates))
    }

    /// Every template crafting's work makes: the totems, the refined products, the world's
    /// recipes' products and the combinations'.
    pub fn templates(&self) -> impl Iterator<Item = u32> + '_ {
        self.totems
            .iter()
            .map(|t| t.template)
            .chain(self.refining.values().map(|r| r.product))
            .chain(self.recipes.values().map(|r| r.product))
            .chain(self.combinations.iter().map(|c| c.product))
    }
}

/// What a step or a give waits on: the player's level in a craft skill, or in any one of several,
/// at least so much (the level a making reads: its own, with what its effects add, never past
/// the skill's cap).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CraftGate {
    /// The craft skills, by their saved states; any one of them at the level will do.
    pub skills: Vec<u32>,
    pub at_least: u32,
}

/// The steps and gives that wait on a player's craft skills.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CraftGates {
    /// Quest steps, each with what it waits on.
    pub steps: Vec<(Step, CraftGate)>,
    /// Gives that move no quest, each with what it waits on.
    pub gives: Vec<(Give, CraftGate)>,
}

impl CraftGates {
    /// Does no step or give wait on a craft skill?
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty() && self.gives.is_empty()
    }

    /// What a step waits on: the gates of the step of its quest and key.
    pub fn of_step<'a>(&'a self, step: &'a Step) -> impl Iterator<Item = &'a CraftGate> + 'a {
        self.steps
            .iter()
            .filter(move |(s, _)| s.quest == step.quest && s.key == step.key)
            .map(|(_, g)| g)
    }

    /// What a give waits on.
    pub fn of_give<'a>(&'a self, give: &'a Give) -> impl Iterator<Item = &'a CraftGate> + 'a {
        self.gives
            .iter()
            .filter(move |(g, _)| g == give)
            .map(|(_, g)| g)
    }
}
