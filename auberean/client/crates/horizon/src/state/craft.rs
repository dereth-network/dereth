//! Crafting as Horizon draws it: the player's craft skills and their recipes, the recipe opened
//! for making as the game last read it, and what the player's scripts said of making this frame,
//! followed from the view's craft and the craft notices.

use std::collections::BTreeMap;

use auberean_client_sdk::contract::notice::{CraftNotice, Notice, Said};
use auberean_client_sdk::contract::text::TextRef;
use auberean_client_sdk::contract::view::{self as v, View};

/// A craft skill as the crafting window lists it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CraftSkill {
    pub id: u32,
    pub name: String,
    /// Its icon, a picture of the game's records (0 for none).
    pub icon: u32,
    /// Its level as it stands, and its own (without what raises or lowers it for now).
    pub level: u32,
    pub own_level: u32,
    /// What raising it costs, in experience.
    pub cost_to_raise: u64,
    /// Whether it may ever be raised, and whether it may be raised now.
    pub raiseable: bool,
    pub can_raise: bool,
    /// Its use earns at least the usual experience.
    pub earns: bool,
    /// Its recipes, in the game's order.
    pub recipes: Vec<Recipe>,
}

/// A recipe as the crafting window lists it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Recipe {
    pub id: u32,
    pub name: String,
    /// Its icon (0: its craft skill's is shown).
    pub icon: u32,
    /// It is listed.
    pub shown: bool,
    pub category: u32,
    /// The game time from which it may be made again.
    pub use_again: f64,
}

/// The recipe opened for making, as the game read it at a spinner value: the game's own record,
/// its name and its ingredients' names written.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Opened {
    pub at: v::RecipeAt,
    pub name: String,
    /// Each ingredient the recipe takes at this value: its slot's number and what it is.
    pub ingredients: Vec<(u32, String)>,
}

/// An ingredient put in a slot, or not, as the scripts said.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Added {
    pub recipe: u32,
    pub slot: u32,
    pub item: u64,
    /// The spinner's value the scripts had.
    pub spinner: i32,
    /// Why it was not put in (0: it was).
    pub error: u32,
}

/// Crafting as Horizon draws it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Crafting {
    /// The craft skills, in the game's order.
    pub skills: Vec<CraftSkill>,
    /// The experience the player may spend on craft skills.
    pub experience: u64,
    /// The recipe categories' names.
    pub categories: BTreeMap<u32, String>,
    /// The recipe opened for making, as the game last read it.
    pub opened: Option<Opened>,
    /// This frame: the recipes the scripts asked to open for making, the ingredients they said
    /// were put in (or not), and the makings they said were done (the recipe, and why it failed:
    /// 0 when it went).
    pub open_asked: Vec<u32>,
    pub added: Vec<Added>,
    pub made: Vec<(u32, u32)>,
    /// The view's craft as last written, and whether a name in it was not yet to be had.
    version: Option<u64>,
    missing: bool,
}

impl Crafting {
    /// A craft skill by its id.
    #[must_use]
    pub fn skill(&self, id: u32) -> Option<&CraftSkill> {
        self.skills.iter().find(|s| s.id == id)
    }

    /// Follow one frame: the view's craft (written again when it changed, or while one of its
    /// names was not to be had), and the scripts' craft notices.
    pub fn follow(
        &mut self,
        view: Option<&View>,
        notices: &[Notice],
        write: &dyn Fn(&TextRef) -> Option<String>,
    ) {
        self.open_asked.clear();
        self.added.clear();
        self.made.clear();
        let Some(craft) = view.and_then(|v| v.craft.as_ref()) else {
            *self = Self::default();
            return;
        };
        if self.version != Some(craft.version) || self.missing {
            self.version = Some(craft.version);
            self.missing = false;
            let mut name = |t: &TextRef| match write(t) {
                Some(n) => n,
                None => {
                    self.missing = true;
                    String::new()
                }
            };
            self.experience = craft.experience;
            self.categories = craft
                .categories
                .iter()
                .map(|(c, t)| (*c, name(t)))
                .collect();
            self.skills = craft
                .skills
                .iter()
                .map(|s| {
                    let own = s.own.as_ref().unwrap_or(&s.listed);
                    CraftSkill {
                        id: s.listed.did,
                        name: name(&s.listed.name),
                        icon: own.icon,
                        level: s.level,
                        own_level: s.raw_level,
                        cost_to_raise: own.cost_to_raise,
                        raiseable: s.raiseable,
                        can_raise: s.can_raise,
                        earns: s.listed.usage_xp_mod >= 1.0,
                        recipes: s
                            .recipes
                            .iter()
                            .map(|r| Recipe {
                                id: r.did,
                                name: name(&r.name),
                                icon: r.icon,
                                shown: r.shown,
                                category: r.category,
                                use_again: r.time_use_again,
                            })
                            .collect(),
                    }
                })
                .collect();
            self.opened = craft.opened.as_ref().map(|at| Opened {
                at: at.clone(),
                name: name(&at.name),
                ingredients: at
                    .ingredients
                    .iter()
                    .map(|i| (i.id, name(&i.description)))
                    .collect(),
            });
        }
        for n in notices {
            match n.said {
                Said::Craft(CraftNotice::Open { recipe }) => self.open_asked.push(recipe),
                Said::Craft(CraftNotice::IngredientAdded {
                    recipe,
                    slot,
                    item,
                    amount,
                    error,
                }) => self.added.push(Added {
                    recipe,
                    slot,
                    item,
                    spinner: amount.cast_signed(),
                    error,
                }),
                Said::Craft(CraftNotice::Made { recipe, error }) => {
                    self.made.push((recipe, error));
                }
                _ => {}
            }
        }
    }
}

/// How hard a recipe is to make, as its name's color shows it: 0 for no difficulty, 1 to 5 from
/// far below the craft skill to far above it, 6 past what the skill may make at all.
#[must_use]
pub fn difficulty_colour(at: &v::RecipeAt) -> u32 {
    let (difficulty, colour) = difficulty_at(at);
    if past_cutoff(at) {
        6
    } else if at.name_coloring {
        colour
    } else if difficulty == 0 {
        0
    } else {
        #[allow(clippy::cast_precision_loss)]
        let over = difficulty as f32 - at.skill_level as f32;
        let c = scale(over + 42.0, -50.0, 50.0, 0.0, 5.0);
        dereth_primitives::num::to_i32(c).cast_unsigned().min(5)
    }
}

/// The recipe's difficulty and its name's color at its spinner's value: those the value gives
/// when they move with it, else the recipe's own.
#[must_use]
pub fn difficulty_at(at: &v::RecipeAt) -> (u32, u32) {
    match at.spinner_difficulty {
        Some(d) if at.dynamic_difficulty => d,
        _ => (at.difficulty, at.name_color),
    }
}

/// What making the recipe costs at its spinner's value.
#[must_use]
pub fn cost_at(at: &v::RecipeAt) -> i32 {
    match at.spinner_cost {
        Some(c) if at.dynamic_cost => c,
        _ => at.cost,
    }
}

/// Whether the recipe is past what the craft skill may make: its difficulty over the skill by
/// more than the skill's cutoff.
#[must_use]
pub fn past_cutoff(at: &v::RecipeAt) -> bool {
    let (difficulty, _) = difficulty_at(at);
    let over = difficulty.wrapping_sub(at.skill_level).cast_signed();
    at.cutoff.is_some_and(|c| c.cast_signed() < over)
}

/// A value kept between two bounds, mapped onto another range.
fn scale(v: f32, from_lo: f32, from_hi: f32, to_lo: f32, to_hi: f32) -> f32 {
    let v = v.clamp(from_lo, from_hi);
    (to_hi - to_lo) * ((v - from_lo) / (from_hi - from_lo)) + to_lo
}
