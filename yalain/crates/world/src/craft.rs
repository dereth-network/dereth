//! Crafting: a player's craft skills and recipes, putting ingredients into a recipe, and making
//! it.
//!
//! **What the client knows.** A player's whole craft registry (its craft skills with their levels
//! and experience, its recipes with when each may be used again and its charges) comes in one
//! call after the session start; the client's crafting page reads nothing else. A new character
//! holds every craft skill at level 1 and the world's starting recipes, its craft skills' daily
//! experience starting over 22 hours on. Learning or forgetting a recipe later is a call of its
//! own.
//!
//! **An ingredient** the client puts into a recipe's slot is answered with the request's values
//! and why it does not fit (0 when it does): the recipe unknown or the slot not one of its, the
//! item not the player's, not one of the slot's items or of their classes, or too few of a stack
//! for the spinner the player chose. The client writes the reason.
//!
//! **The spinner** (a recipe's item level) decides what a recipe takes: of a recipe with a spinner
//! only the ingredients whose own range holds the value chosen are asked for, each in the amount
//! its own table gives at that value, and the outcome uses up what its actions say at that value.
//! A recipe without one takes all its ingredients, in their fixed amounts.
//!
//! **Making a recipe**: refused at once (only the answer, with why) when the player is not at
//! peace, is dead or in portal space, the recipe is unknown or still waiting, its workshop is not at hand (the record a
//! workshop puts on a player nearby, [`crate::workshops`]), the spinner is outside the recipe's
//! range, the slots filled are not the ones the spinner asks for, an item does not fit, or its money
//! is short of the recipe's cost. Else the player
//! plays the recipe's animation for its cycles, lets it go, and some seconds after the request the
//! result: the check rolled against the recipe's difficulty and the player's craft skill, and the
//! outcome's actions in order (items made in the pack, ingredients used up, an effect on the
//! player such as a quest moved on, a text), the cost paid, the recipe's wait and charges told, the
//! answer; ingredients used up after the answer. What it asked is asked again as the result comes:
//! an ingredient gone or the money spent since, and it makes nothing, says why and takes nothing.
//!
//! **Refining** turns two materials into a third, in a workshop. The recipe holds what it takes
//! and uses up; what it yields it does not hold: a third of the amount taken of each material,
//! growing with the player's craft skill and what its effects add to that skill, such as a
//! crucible's (2017: 100 of each gave 34, 2 of 2, both at level 2; how it grows is this server's
//! own), added to the player's stack with a line saying how many. It pays what its craft skill's
//! table pays for a use at its level (2 of 2).
//!
//! **Spellbinding** takes an item apart into a totem: a success makes a totem of the item's kind
//! (2017: 13 of 13), the failures of its check nothing, and both use up what the outcome says. The
//! totem holds the item's first enchantment, and asks of what it goes on the item's own least level
//! and Arcane Lore (2017: 41 totems, one enchantment each). An item with none is refused. Which
//! enchantment of several is our choice.
//!
//! **Dyeing** gives a piece of armor the color of a dye plant: three of its appearance modifiers
//! take the plant's value, in the pack and on whoever wears it (2017: 32 of 32 by the armor's next
//! making; the plant is used up, the armor stays).
//!
//! **Other recipes that work out their own product** (card melds and the like) name none the data
//! holds: such a making answers that it failed, with a line saying so, uses up nothing and pays
//! nothing (2017: 9 of 9 card melds).
//!
//! **Craft experience.** A recipe made pays experience in its craft skill: the recipe's table by
//! the spinner chosen (else its own fixed amount), times the outcome's share. A part goes to the
//! player's own pool of craft experience: a fifth for a recipe skill, a hundredth for a gathering
//! skill (never less than one point), and the rest to the craft skill, which rises a level
//! whenever what it holds reaches its table's next step, the step taken from what it holds. At its
//! cap a craft skill takes nothing: the award is only its pool share, all of it the player's. The
//! pool's total and what is left of it, then the craft skill's record, are told the client.
//!
//! **Raising** a craft skill by asking spends from the pool what the skill still lacks for its
//! next level: its record holds nothing after, and "Your rating in %1 is now %2." is shown.
//!
//! **The surveyor's map recipes** of a region share one wait: drawing any of them (a recipe whose
//! outcome sets the region's wait) has all five wait five minutes, and a map of the region finding
//! its place lets all five be drawn again; either way the five records come in one call.
//!
//! **A day's craft experience** starts over 22 hours after it last did (at entry, when that came
//! while the player was away): every craft skill's record is told with nothing earned today.

use std::collections::HashMap;

use auberean_protocol::physics::Release;
use auberean_protocol::quality::VisualQuality;
use auberean_protocol::types::BehaviorParams;
use auberean_protocol::ServerMessage;
use auberean_script::args::Arg;
use auberean_script::graph::{Graph, PackedObject};
use auberean_script::library::{Elem, Library};
use auberean_script::native_object::{Item, NativeObject};
use yalain_net::{ConnId, ServerNet};
use yalain_store::Store;

use crate::calls::{Node, Value};
use crate::graph::{hash, tag, Builder, Slot, NULL};
use crate::start::Nav;
use crate::state::{CraftSkillState, RecipeState};
use crate::{presence, progress, quals, session, Clock, Server, Timer};

/// The calls of crafting.
pub mod function {
    /// The player's whole craft registry.
    pub const REFRESH: u32 = 0x8206_0019;
    /// Every craft skill's record; one craft skill's.
    pub const SKILLS: u32 = 0x8206_0033;
    pub const SKILL: u32 = 0x8206_0034;
    /// A recipe learned, forgotten, or its record changed.
    pub const ADD_RECIPE: u32 = 0x8206_001B;
    pub const REMOVE_RECIPE: u32 = 0x8206_0018;
    pub const UPDATE_RECIPE: u32 = 0x8206_001A;
    /// Several recipes' records changed at once.
    pub const UPDATE_RECIPES: u32 = 0x8206_0031;
    /// The client asks to put an ingredient in; the answer.
    pub const ADD_INGREDIENT: u32 = 0x8206_0006;
    pub const INGREDIENT_ADDED: u32 = 0x8206_0016;
    /// The client asks to make a recipe; the answer.
    pub const EXECUTE: u32 = 0x8206_0012;
    pub const EXECUTED: u32 = 0x8206_001C;
    /// The client asks to raise a craft skill.
    pub const RAISE: u32 = 0x8206_0022;
}

/// The player's own pool of craft experience: all it was ever given, and what is left to spend.
pub mod pool {
    pub const TOTAL: u32 = 1000;
    pub const AVAILABLE: u32 = 1001;
}

/// The effects a craft's result plays on the player.
pub mod fx {
    pub const FAILED: u32 = 0x4100_051C;
    pub const MADE: u32 = 0x4100_051E;
    /// A gathering use done expertly.
    pub const MADE_WELL: u32 = 0x4100_051F;
}

/// "Your rating in %1 is now %2.": its string, and its variables (the craft skill's name, the new
/// level).
pub mod raised_text {
    pub const TEXT: (u32, u32) = (0x903D_FFF9, 0x2500_000C);
    pub const SKILL: u32 = 0x23AF_C5F3;
    pub const RATING: u32 = 0x1916_B459;
}

/// Why an ingredient or a craft is refused (the client writes the reason).
pub mod error {
    pub const NOT_OWNED: u32 = 1048;
    pub const MISSING_ITEM: u32 = 40_001;
    pub const TOO_FEW: u32 = 40_002;
    pub const BAD_RECIPE: u32 = 40_016;
    pub const INVALID_INGREDIENTS: u32 = 40_018;
    /// A recipe with no charges left: the recipe's own message for it, if it has one, is the client's.
    pub const NO_CHARGES: u32 = 40_021;
    /// A recipe that asks for a target, not given one of its own (2017: 8 of 8, none of them at a mine).
    pub const WRONG_TARGET: u32 = 40_022;
    pub const NOT_RESET: u32 = 40_024;
    pub const FAILED: u32 = 40_025;
    pub const PEACE_REQUIRED: u32 = 40_026;
    /// Dead or in portal space: the client's word for being too busy to craft.
    pub const TOO_BUSY: u32 = 40_028;
    /// No place in the pack for the product (2017: 1 of 1, at the result, with a full pack).
    pub const NO_ROOM: u32 = 40_029;
    /// The maker's money is short of the recipe's cost.
    pub const NO_MONEY: u32 = 40_038;
    /// The recipe's workshop is not at hand (its record is not on the player).
    pub const MISSING_EFFECT: u32 = 40_041;
    /// An item whose least level is above what its slot takes at the spinner.
    pub const ITEM_LEVEL_TOO_HIGH: u32 = 2007;
    /// An item whose Arcane Lore is above what its slot takes at the spinner, or a slot that asks
    /// only a level, of an item that takes Arcane Lore.
    pub const ITEM_LORE_TOO_HIGH: u32 = 2008;
    /// An item its craft flags bar from a slot, none of the named ones.
    pub const BARRED_BY_FLAGS: u32 = 40_009;
    /// A craft flag a slot requires is missing: by the flag 1, 2 and 4.
    pub const LACKS_FLAG: [(u32, u32); 3] = [(1, 40_030), (2, 40_034), (4, 40_032)];
    /// A craft flag a slot refuses is there: by the flag 1, 2 and 4.
    pub const HAS_FLAG: [(u32, u32); 3] = [(1, 40_031), (2, 40_035), (4, 40_033)];
}

/// A recipe's and an ingredient's flags, as their definitions hold them.
mod flag {
    /// A recipe: it has a spinner.
    pub const RECIPE_SPINNER: u32 = 0x80;
    /// A recipe: it needs an effect on its maker.
    pub const RECIPE_EFFECT: u32 = 0x2000;
    /// An ingredient: its amount comes from a table by the spinner.
    pub const INGREDIENT_STACK_TABLE: u32 = 0x2;
    /// An ingredient: it is asked for at a range of the spinner.
    pub const INGREDIENT_RANGE: u32 = 0x4;
    /// An ingredient: the highest item level it takes, fixed or by the spinner.
    pub const INGREDIENT_LEVEL: u32 = 0x10;
    pub const INGREDIENT_LEVEL_TABLE: u32 = 0x20;
    /// An ingredient: the highest Arcane Lore it takes, fixed or by the spinner.
    pub const INGREDIENT_LORE: u32 = 0x40;
    pub const INGREDIENT_LORE_TABLE: u32 = 0x80;
    /// A recipe: it can't be tried when its difficulty is too far above the craft skill.
    pub const RECIPE_CUTOFF: u32 = 0x400;
    /// An action: its amount comes from a table by the spinner.
    pub const ACTION_TABLE: u32 = 0x2;
}

/// The qualities effects add to a craft skill by: these bits with the craft skill's number.
const BONUS_STATS: u32 = 129 << 16;

/// The script packages of the registry and its records, with their sizes in slots.
pub mod package {
    pub const REGISTRY: (u16, usize) = (0x1D2, 7);
    pub const SKILL_RECORD: (u16, usize) = (0x1B4F, 6);
    pub const RECIPE_RECORD: (u16, usize) = (0x1D9, 12);
}

/// A new character's registry: its score and title, and how long until its craft skills' daily
/// experience starts over (2017: 79,210-79,267 s after the first session started; each later day
/// 79,232 s after the last began, 16 times).
pub const NEW_SCORE: f32 = 0.002;
pub const NEW_TITLE: u32 = 23;
pub const DAY_RESET_SECONDS: f64 = 79_240.0;

/// An item's quantity (a visual quality).
pub const QUANTITY: u32 = 262;

/// The size places of the registry's tables (47 and 383 buckets, as 2017's).
const SKILL_TABLE_SIZE: u8 = 2;
const RECIPE_TABLE_SIZE: u8 = 5;

/// A word slot tagged as an enumeration (a data id), as the registry carries them.
const DATUM: u8 = 0x04;

/// An ingredient slot of a recipe: its number, how many it takes, the templates and classes it
/// takes.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Ingredient {
    pub ordinal: u32,
    pub quantity: u32,
    pub items: Vec<u32>,
    pub classes: Vec<u32>,
    /// The spinner values it is asked for at, when it has a range of its own.
    pub spinners: Option<(u32, u32)>,
    /// The table giving its amount by the spinner (a level table's saved state), in place of
    /// `quantity`.
    pub stack_table: Option<u32>,
    /// The highest item level it takes: a number, or a table by the spinner (a saved state).
    pub level: Limit,
    /// The highest Arcane Lore it takes, likewise.
    pub lore: Limit,
    /// The craft flags an item must have, and those it must not.
    pub required_flags: u32,
    pub restricted_flags: u32,
}

/// What a slot asks of an item's level or Arcane Lore.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Limit {
    /// Nothing.
    #[default]
    None,
    Fixed(i32),
    ByTable(u32),
}

impl Ingredient {
    /// Is it asked for at a spinner value (a recipe with a spinner asks only for those whose own
    /// range holds the value)?
    #[must_use]
    pub fn asked_at(&self, spinner: u32) -> bool {
        self.spinners
            .is_some_and(|(least, most)| (least..=most).contains(&spinner))
    }
}

/// What an outcome does.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// Make so many of a template in the pack (the product's ordinal names it to later actions).
    Produce {
        template: u32,
        quantity: u32,
        ordinal: u32,
    },
    /// Use up so many of a slot's ingredient: by the spinner when it has a table, and only at the
    /// spinner values it names (none: at any).
    Destroy {
        ordinal: u32,
        quantity: u32,
        by_spinner: Option<u32>,
        spinners: Option<(u32, u32)>,
    },
    /// Put an effect on the player.
    Effect { state: u32, spellcraft: f32 },
    /// Show a text of a kind.
    Text { text: (u32, u32), kind: u32 },
    /// Give an item of the making (an ingredient by its slot's ordinal, a product by its own) an
    /// effect of its own, for when it applies ([`item_effect`]): at a fixed spellcraft, or at the
    /// spinner the player chose through a level table (its saved state).
    ItemEffect {
        ordinal: u32,
        state: u32,
        kind: u32,
        spellcraft: f32,
        by_spinner: Option<u32>,
    },
    /// Make an item of the making (a product by its ordinal) one that binds to its holder when used
    /// in combat, when the spinner the player chose is within the action's.
    Bind { ordinal: u32, spinners: (u32, u32) },
    /// Attune an item of the making (by its ordinal) to whoever holds it, when the spinner the
    /// player chose is within the action's.
    Attune { ordinal: u32, spinners: (u32, u32) },
    /// Change an item of the making (a product by its ordinal) by the spinner chosen, when it is
    /// within the action's: through a table (its saved state), or by no number.
    Mutate {
        ordinal: u32,
        table: Option<u32>,
        spinners: Option<(u32, u32)>,
    },
    /// Make an item of the making one that can become harmonic.
    Harmonize { ordinal: u32, spinners: (u32, u32) },
    /// The recipe's own work on success, which its kind decides ([`Custom`]).
    Custom,
    /// Add craft flags to an item of the making (a product or a slot's, by its ordinal), when the
    /// spinner the player chose is within the action's.
    Flag {
        ordinal: u32,
        flags: u32,
        spinners: Option<(u32, u32)>,
    },
    /// Marks on the product the client does not show.
    Other,
}

/// What a recipe's own work on success does, by its kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Custom {
    /// It has none.
    #[default]
    None,
    /// Refining: it yields units of this template, by how much it took and the craft skill.
    Refine { product: u32 },
    /// Spellbinding: it makes a totem of the kind of the item in its first slot.
    Spellbind,
    /// Dyeing: the item in its first slot takes the color of the dye in its second.
    Dye,
    /// A product it works out for itself, which nothing here can: it makes nothing.
    Unknown,
}

/// The totems spellbinding makes, by what the item taken apart is: a weapon, a shield, armor (2017:
/// 11 weapons, 2 armor; the shield's by the totems seen in play, none made).
pub mod totem {
    pub const WEAPON: u32 = 0x4700_0CCB;
    pub const SHIELD: u32 = 0x4700_0CC9;
    pub const ARMOR: u32 = 0x4700_0CC7;

    /// What a totem of a template says it goes on: the word for it. None for any other template.
    #[must_use]
    pub const fn noun(template: u32) -> Option<u32> {
        match template {
            WEAPON => Some(0xEE40_CD5E),
            SHIELD => Some(0x3B28_4FFE),
            ARMOR => Some(0xCED7_169A),
            _ => None,
        }
    }
}

/// The script packages whose recipes bind an item's enchantments into a totem, and whose recipes
/// work out a product for themselves.
const SPELLBINDING: [&str; 2] = ["MinorSpellbindingRecipe", "MajorSpellbindingRecipe"];
const OWN_PRODUCT: &str = "SecretRecipe";
const DYEING: &str = "DyeingRecipe";

/// "You fail to produce anything."
pub const NOTHING_PRODUCED: (u32, u32) = (0x9CD4_94DC, 0x2500_000C);

/// The refining recipes' script packages and the template each yields.
const REFINED: [(&str, u32); 10] = [
    ("SteelRefiningRecipe", 0x4700_0B40),
    ("DramasticRefiningRecipe", 0x4700_0B46),
    ("MoonglassRefiningRecipe", 0x4700_08FC),
    ("RunewoodRefiningRecipe", 0x4700_08DA),
    ("TukaliteRefiningRecipe", 0x4700_08D3),
    ("TileRefiningRecipe", 0x4700_0657),
    ("BlockRefiningRecipe", 0x4700_0658),
    ("BoardRefiningRecipe", 0x4700_0659),
    ("BrickRefiningRecipe", 0x4700_0655),
    ("PlasterRefiningRecipe", 0x4700_0656),
];

/// A refining's greatest craft skill level, where it yields all it takes.
const REFINING_CAP: u32 = 150;

/// What a refining of this amount of each material yields at a craft skill level: a third of it at
/// the first level (2017: 34 of 100 at level 2, 2 of 2), all of it at the skill's cap (this
/// server's own: the recipes say only that it grows with the skill).
#[must_use]
pub fn refining_yield(amount: u32, level: u32) -> u32 {
    let above = f64::from(level.clamp(1, REFINING_CAP) - 1) / f64::from(REFINING_CAP - 1);
    let share = 1.0 / 3.0 + 2.0 / 3.0 * above;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let got = (f64::from(amount) * share).round() as u32;
    got
}

/// "You successfully produce %1 units of %2.": its string, and its variables (how many, the
/// product's name).
pub mod refined_text {
    pub const TEXT: (u32, u32) = (0xC13B_3213, 0x2500_000C);
    pub const COUNT: u32 = 0x313C_55EA;
    pub const PRODUCT: u32 = 0x9D06_6C50;
}

/// When an effect a making gives an item applies: always, for a while, on its use, on whoever
/// wears or wields it, on what it hits.
pub mod item_effect {
    pub const STATIC: u32 = 0;
    pub const TIMED: u32 = 1;
    pub const USED: u32 = 2;
    pub const WORN: u32 = 3;
    pub const ON_HIT: u32 = 4;
    /// The action's flags: its spellcraft is the spinner's through a level table.
    pub const BY_SPINNER: u32 = 0x2;
}

/// One outcome of a check: the roll it covers up to, its share of the craft experience, its
/// actions.
#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    pub upto: f32,
    pub xp_mod: f32,
    pub actions: Vec<Action>,
}

/// A recipe's check: the value it covers up to, its outcomes by roll.
#[derive(Debug, Clone, PartialEq)]
pub struct Check {
    pub upto: f32,
    pub outcomes: Vec<Outcome>,
}

/// Values by level (or by a recipe's spinner): the first level the table covers, and its value at
/// each level from there; a level outside it takes the nearest end's.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LevelMap {
    pub first: u32,
    pub values: Vec<u32>,
}

impl LevelMap {
    /// The value at a level.
    #[must_use]
    pub fn at(&self, level: u32) -> u32 {
        let i = level.saturating_sub(self.first) as usize;
        self.values
            .get(i.min(self.values.len().saturating_sub(1)))
            .copied()
            .unwrap_or(0)
    }

    /// Read a level mapping table's graph.
    ///
    /// # Errors
    /// The table's list is missing.
    pub fn from_state(lib: &Library, g: &Graph) -> Result<Self, String> {
        let nav = Nav::new(lib, g);
        let root = nav.object(g.root().ok_or("an empty graph")?)?;
        let values = nav
            .items(nav.field(root, "m_map")?)?
            .into_iter()
            .map(|i| match i {
                Item::Long(v) => u32::try_from(v).map_err(|e| e.to_string()),
                Item::Word(v) => Ok(v),
                other => Err(format!("{other:?} in a level table")),
            })
            .collect::<Result<Vec<u32>, String>>()?;
        Ok(Self {
            first: nav.field(root, "m_minLevel").unwrap_or(1),
            values,
        })
    }
}

/// A recipe's definition.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RecipeDef {
    pub did: u32,
    pub skill: u32,
    pub category: u32,
    pub difficulty: u32,
    /// Its cost in money, its charges (-1 none), how long until it may be made again (s).
    pub cost: u32,
    pub charges: i32,
    pub recovery: f32,
    pub craft_xp: u32,
    /// The animation the player plays, and for how many cycles.
    pub animation: u32,
    pub cycles: u32,
    pub ingredients: Vec<Ingredient>,
    /// The templates it must be made at (none: it asks for no target).
    pub targets: Vec<u32>,
    pub checks: Vec<Check>,
    /// The craft experience by the spinner chosen (none: the fixed `craft_xp`).
    pub xp_map: Option<LevelMap>,
    /// The difficulty by the spinner chosen (none: the fixed `difficulty`).
    pub difficulty_map: Option<LevelMap>,
    /// How far above the craft skill its difficulty may be for it to be tried at all (none: any).
    pub cutoff: Option<i32>,
    /// The spinner's least and greatest values, when the recipe has one.
    pub spinner: Option<(u32, u32)>,
    /// The effect a maker must hold (a workshop's), when it needs one.
    pub required_effect: Option<u32>,
    /// What its own work on success does.
    pub custom: Custom,
}

impl RecipeDef {
    /// The ingredients a making asks for at a spinner value: of a recipe with a spinner those
    /// whose range holds it, of any other all of them.
    pub fn ingredients_at(&self, spinner: u32) -> impl Iterator<Item = &Ingredient> {
        self.ingredients
            .iter()
            .filter(move |i| self.spinner.is_none() || i.asked_at(spinner))
    }

    /// The craft experience a making pays before its outcome's share: by the spinner chosen when
    /// the recipe has a table for it, else its fixed amount.
    #[must_use]
    pub fn base_xp(&self, spinner: u32) -> u64 {
        u64::from(
            self.xp_map
                .as_ref()
                .map_or(self.craft_xp, |m| m.at(spinner)),
        )
    }

    /// The difficulty a making is checked against: by the spinner chosen when the recipe has a
    /// table for it, else its fixed difficulty.
    #[must_use]
    pub fn difficulty_at(&self, spinner: u32) -> u32 {
        self.difficulty_map
            .as_ref()
            .map_or(self.difficulty, |m| m.at(spinner))
    }
}

/// A craft skill's definition: its cap, its name, its advancement table (the experience each level
/// needs, from level 0), the day's experience past which a use pays less, by level; what a use pays
/// by level (gathering skills only); the share of an award that goes to the player's own pool.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CraftSkillDef {
    pub did: u32,
    pub cap: u32,
    pub name: (u32, u32),
    pub advancement: Vec<u64>,
    pub daily: LevelMap,
    pub usage_xp: Option<LevelMap>,
    pub pool_share: f32,
    pub cannot_raise: bool,
    /// The quality effects change to add to it (a crucible's, a tool's): none when it has none.
    pub bonus_stat: u32,
    /// What a tool's requirement calls it.
    pub common_name: (u32, u32),
}

impl CraftSkillDef {
    /// The experience the next level takes from a level (none past the table's end).
    #[must_use]
    pub fn step(&self, level: u32) -> Option<u64> {
        let l = level as usize;
        let (a, b) = (self.advancement.get(l)?, self.advancement.get(l + 1)?);
        Some(b.saturating_sub(*a))
    }
}

/// What the world knows of crafting: the recipes and craft skills, and what a new character
/// starts with.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Crafting {
    pub recipes: HashMap<u32, RecipeDef>,
    pub skills: Vec<CraftSkillDef>,
    pub starting_recipes: Vec<u32>,
    /// What each butchery profile yields.
    pub butchery: HashMap<u32, Vec<crate::butchery::Yield>>,
    /// The level tables recipes' actions read by the spinner, by saved state.
    pub spinner_tables: HashMap<u32, LevelMap>,
}

impl Crafting {
    /// The recipes and craft skills of these saved states (a state that does not read is left
    /// out; a recipe's craft skill is read with it).
    #[must_use]
    pub fn read(
        lib: &Library,
        skills: &[u32],
        recipes: &[u32],
        starting_recipes: &[u32],
        graph: &dyn Fn(u32) -> Option<Graph>,
    ) -> Self {
        let mut t = Self {
            starting_recipes: starting_recipes.to_vec(),
            ..Self::default()
        };
        for did in skills {
            let Some(g) = graph(*did) else { continue };
            let nav = Nav::new(lib, &g);
            let Some(root) = g.root().and_then(|r| nav.object(r).ok()) else {
                continue;
            };
            let table = |name: &str| {
                nav.field(root, name)
                    .ok()
                    .filter(|d| *d != 0)
                    .and_then(graph)
            };
            t.skills.push(CraftSkillDef {
                did: *did,
                cap: nav.field(root, "m_uiAdvancementCap").unwrap_or(1),
                name: text_of(&nav, root, "m_siName"),
                advancement: table("m_didAdvancementTable")
                    .and_then(|g| crate::skills::table_from_state(lib, &g).ok())
                    .unwrap_or_default(),
                daily: table("m_didDailyXPTable")
                    .and_then(|g| LevelMap::from_state(lib, &g).ok())
                    .unwrap_or_default(),
                usage_xp: table("m_didDefaultXPTable")
                    .and_then(|g| LevelMap::from_state(lib, &g).ok()),
                pool_share: nav.number(root, "m_fGenericXPMod").unwrap_or(0.0),
                cannot_raise: nav.field(root, "m_bCannotRaise").unwrap_or(0) != 0,
                bonus_stat: nav.field(root, "mEnum").map_or(0, |e| BONUS_STATS | e),
                common_name: text_of(&nav, root, "m_siCommonName"),
            });
        }
        for did in recipes.iter().chain(starting_recipes) {
            if t.recipes.contains_key(did) {
                continue;
            }
            let Some(g) = graph(*did) else { continue };
            if let Ok(mut r) = RecipeDef::from_state(*did, lib, &g) {
                let nav = Nav::new(lib, &g);
                let map = |name: &str| {
                    g.root()
                        .and_then(|root| nav.object(root).ok())
                        .and_then(|root| nav.field(root, name).ok())
                        .filter(|x| *x != 0 && *x != NULL)
                        .and_then(|x| nav.states_of_ref(x))
                        .and_then(graph)
                        .and_then(|m| LevelMap::from_state(lib, &m).ok())
                };
                r.xp_map = map("m_craftXPMappingTable");
                r.difficulty_map = map("m_difficultyMappingTable");
                let tables: Vec<u32> = r
                    .checks
                    .iter()
                    .flat_map(|c| &c.outcomes)
                    .flat_map(|o| &o.actions)
                    .filter_map(|a| match a {
                        Action::ItemEffect { by_spinner, .. }
                        | Action::Destroy { by_spinner, .. } => *by_spinner,
                        Action::Mutate { table, .. } => *table,
                        _ => None,
                    })
                    .chain(r.ingredients.iter().filter_map(|i| i.stack_table))
                    .chain(r.ingredients.iter().flat_map(|i| {
                        [i.level, i.lore].into_iter().filter_map(|l| match l {
                            Limit::ByTable(table) => Some(table),
                            _ => None,
                        })
                    }))
                    .collect();
                for table in tables {
                    if let Some(m) = graph(table).and_then(|m| LevelMap::from_state(lib, &m).ok()) {
                        t.spinner_tables.insert(table, m);
                    }
                }
                t.recipes.insert(*did, r);
            }
        }
        t
    }

    /// The templates crafting needs the world to make things of: what the recipes make and ask for,
    /// and the nodes surveying conjures, whose materials recipes ask for in turn.
    #[must_use]
    pub fn templates(&self) -> Vec<u32> {
        let made = self
            .recipes
            .values()
            .flat_map(|r| &r.checks)
            .flat_map(|c| &c.outcomes)
            .flat_map(|o| &o.actions)
            .filter_map(|a| match a {
                Action::Produce { template, .. } => Some(*template),
                _ => None,
            });
        let asked = self
            .recipes
            .values()
            .flat_map(|r| &r.ingredients)
            .flat_map(|i| i.items.iter().copied());
        let refined = self.recipes.values().filter_map(|r| match r.custom {
            Custom::Refine { product } => Some(product),
            _ => None,
        });
        made.chain(asked)
            .chain(refined)
            .chain([totem::WEAPON, totem::SHIELD, totem::ARMOR])
            .chain(crate::mining::SUMMONED_NODES)
            .collect()
    }

    /// The spellcraft an item effect is at: fixed, or the spinner's through a level table.
    #[must_use]
    pub fn spellcraft_at(&self, spellcraft: f32, by_spinner: Option<u32>, spinner: u32) -> f32 {
        #[allow(clippy::cast_precision_loss)]
        by_spinner.map_or(spellcraft, |table| {
            self.spinner_tables
                .get(&table)
                .map_or(spinner, |t| t.at(spinner)) as f32
        })
    }

    /// How many of an ingredient a making takes at a spinner value.
    #[must_use]
    pub fn required(&self, ingredient: &Ingredient, spinner: u32) -> u32 {
        ingredient
            .stack_table
            .and_then(|table| self.spinner_tables.get(&table))
            .map_or(ingredient.quantity, |t| t.at(spinner))
    }

    /// What a slot's limit comes to at a spinner value: none where its table holds nothing there.
    fn limit_at(&self, limit: Limit, spinner: u32) -> Option<i32> {
        let n = match limit {
            Limit::None => return None,
            Limit::Fixed(n) => n,
            Limit::ByTable(table) => {
                let at = self.spinner_tables.get(&table)?.at(spinner);
                i32::try_from(at).unwrap_or(i32::MAX)
            }
        };
        (n != 0 && n != -1).then_some(n)
    }

    /// Why an item of a least level and Arcane Lore does not do for a slot at a spinner value (0:
    /// it does). A slot asking Arcane Lore there asks no level. An item that takes Arcane Lore is
    /// judged by that alone, so a slot asking only a level refuses it; any other by its level.
    #[must_use]
    pub fn level_or_lore_refused(
        &self,
        ingredient: &Ingredient,
        spinner: u32,
        item_level: i64,
        item_lore: i64,
    ) -> u32 {
        let lore = self.limit_at(ingredient.lore, spinner);
        let level = if lore.is_some_and(|n| n > 0) {
            None
        } else {
            self.limit_at(ingredient.level, spinner)
        };
        if lore.is_none() && level.is_none() {
            return 0;
        }
        if item_lore > 0 {
            if lore.is_some_and(|n| i64::from(n) >= item_lore) {
                0
            } else {
                error::ITEM_LORE_TOO_HIGH
            }
        } else if level.is_none_or(|n| i64::from(n) >= item_level) {
            0
        } else {
            error::ITEM_LEVEL_TOO_HIGH
        }
    }

    /// How many of a slot's ingredient an action uses up at a spinner value (none: it does nothing
    /// at that value).
    #[must_use]
    pub fn destroyed(&self, action: &Action, spinner: u32) -> Option<(u32, u32)> {
        let Action::Destroy {
            ordinal,
            quantity,
            by_spinner,
            spinners,
        } = action
        else {
            return None;
        };
        if spinners.is_some_and(|(least, most)| !(least..=most).contains(&spinner)) {
            return None;
        }
        let n = by_spinner
            .and_then(|table| self.spinner_tables.get(&table))
            .map_or(*quantity, |t| t.at(spinner));
        Some((*ordinal, n))
    }

    /// An ingredient's classes made its templates: every template the world makes (with the script
    /// package it is made of) that derives from one is one of the slot's items.
    pub fn resolve_classes(
        &mut self,
        templates: &[(u32, u32)],
        derives: &dyn Fn(u32, u32) -> bool,
    ) {
        for ing in self
            .recipes
            .values_mut()
            .flat_map(|r| r.ingredients.iter_mut())
            .filter(|i| !i.classes.is_empty())
        {
            let classes = std::mem::take(&mut ing.classes);
            for &(template, package) in templates {
                if !ing.items.contains(&template)
                    && classes.iter().any(|class| derives(package, *class))
                {
                    ing.items.push(template);
                }
            }
            ing.classes = classes;
        }
    }
}

impl RecipeDef {
    /// Read a recipe from its saved state's graph.
    ///
    /// # Errors
    /// A field every recipe holds is missing.
    pub fn from_state(did: u32, lib: &Library, g: &Graph) -> Result<Self, String> {
        let nav = Nav::new(lib, g);
        let root = nav.object(g.root().ok_or("an empty graph")?)?;
        let word = |name: &str| nav.field(root, name);
        let keys = |o: &PackedObject, name: &str| -> Vec<u32> {
            nav.field(o, name)
                .ok()
                .filter(|r| *r != 0 && *r != NULL)
                .and_then(|r| nav.entries(r).ok())
                .unwrap_or_default()
                .into_iter()
                .filter_map(|(k, _)| match k {
                    Item::Word(w) => Some(w),
                    _ => None,
                })
                .collect()
        };
        let limit = |o: &PackedObject,
                     flags: u32,
                     fixed: u32,
                     by_table: u32,
                     number: &str,
                     table: &str|
         -> Limit {
            if flags & by_table != 0 {
                nav.field(o, table)
                    .ok()
                    .and_then(|r| nav.states_of_ref(r))
                    .map_or(Limit::None, Limit::ByTable)
            } else if flags & fixed != 0 {
                nav.field(o, number)
                    .map_or(Limit::None, |n: u32| Limit::Fixed(n.cast_signed()))
            } else {
                Limit::None
            }
        };
        let mut ingredients: Vec<Ingredient> = nav
            .list(root, "m_ingredients")?
            .into_iter()
            .map(|o| {
                let flags: u32 = nav.field(o, "m_flags").unwrap_or(0);
                Ingredient {
                    ordinal: nav.field(o, "m_uiOrdinal").unwrap_or(0),
                    quantity: nav.field(o, "m_uiQuantity").unwrap_or(1),
                    items: keys(o, "m_hashItems"),
                    classes: keys(o, "m_hashItemClasses"),
                    spinners: (flags & flag::INGREDIENT_RANGE != 0).then(|| {
                        (
                            nav.field(o, "m_minSpinnerVal").unwrap_or(0),
                            nav.field(o, "m_maxSpinnerVal").unwrap_or(0),
                        )
                    }),
                    level: limit(
                        o,
                        flags,
                        flag::INGREDIENT_LEVEL,
                        flag::INGREDIENT_LEVEL_TABLE,
                        "m_iLevel",
                        "m_levelMappingTable",
                    ),
                    lore: limit(
                        o,
                        flags,
                        flag::INGREDIENT_LORE,
                        flag::INGREDIENT_LORE_TABLE,
                        "m_iLore",
                        "m_loreMappingTable",
                    ),
                    required_flags: nav.field(o, "m_requiredFlags").unwrap_or(0),
                    restricted_flags: nav.field(o, "m_restrictedFlags").unwrap_or(0),
                    stack_table: (flags & flag::INGREDIENT_STACK_TABLE != 0)
                        .then(|| {
                            nav.field(o, "m_stackMappingTable")
                                .ok()
                                .and_then(|r| nav.states_of_ref(r))
                        })
                        .flatten(),
                }
            })
            .collect();
        ingredients.sort_by_key(|i| i.ordinal);
        let mut checks = Vec::new();
        let entries = nav
            .field(root, "m_craftCheckEntries")
            .ok()
            .filter(|r| *r != 0 && *r != NULL)
            .and_then(|r| nav.entries(r).ok())
            .unwrap_or_default();
        for (k, v) in entries {
            let Some(o) = v.reference().and_then(|r| nav.object(r).ok()) else {
                continue;
            };
            // The value it covers up to is its key; its own field reads nothing.
            let upto = match k {
                Item::Word(w) => f32::from_bits(w),
                _ => 2.0,
            };
            let mut outcomes = Vec::new();
            let rand = nav
                .field(o, "m_hashRandEntries")
                .ok()
                .filter(|r| *r != 0 && *r != NULL)
                .and_then(|r| nav.entries(r).ok())
                .unwrap_or_default();
            for (k, v) in rand {
                let Some(e) = v.reference().and_then(|r| nav.object(r).ok()) else {
                    continue;
                };
                let upto = match k {
                    Item::Word(w) => f32::from_bits(w),
                    _ => 1000.0,
                };
                let actions = nav
                    .list(e, "m_outcomeActions")
                    .unwrap_or_default()
                    .into_iter()
                    .map(|a| action(lib, &nav, a))
                    .collect();
                outcomes.push(Outcome {
                    upto,
                    xp_mod: nav.number(e, "m_fCraftXPMod").unwrap_or(1.0),
                    actions,
                });
            }
            outcomes.sort_by(|a, b| a.upto.total_cmp(&b.upto));
            checks.push(Check { upto, outcomes });
        }
        checks.sort_by(|a, b| a.upto.total_cmp(&b.upto));
        let flags: u32 = word("m_flags").unwrap_or(0);
        let package = lib.package_name(root.package_id());
        let custom = REFINED
            .iter()
            .find(|(name, _)| *name == package)
            .map_or_else(
                || {
                    if SPELLBINDING.contains(&package.as_str()) {
                        Custom::Spellbind
                    } else if package == OWN_PRODUCT {
                        Custom::Unknown
                    } else if package == DYEING {
                        Custom::Dye
                    } else {
                        Custom::None
                    }
                },
                |(_, product)| Custom::Refine { product: *product },
            );
        Ok(Self {
            did,
            cutoff: (flags & flag::RECIPE_CUTOFF != 0)
                .then(|| word("m_difficultyMinusSkillCutoff").ok())
                .flatten()
                .map(u32::cast_signed),
            skill: word("m_didCraftSkill")?,
            category: word("m_craftSkillCategory")?,
            difficulty: word("m_uiDifficulty")?,
            cost: word("m_cost")?,
            charges: word("m_iCharges")?.cast_signed(),
            recovery: nav.number(root, "m_recoveryTime")?,
            craft_xp: word("m_uiCraftXP")?,
            animation: word("m_animation")?,
            cycles: word("m_uiNumAnimCycles")?,
            ingredients,
            targets: keys(root, "m_hashTargets"),
            checks,
            xp_map: None,
            difficulty_map: None,
            spinner: (flags & flag::RECIPE_SPINNER != 0).then(|| {
                (
                    word("m_minSpinnerVal").unwrap_or(0),
                    word("m_maxSpinnerVal").unwrap_or(0),
                )
            }),
            required_effect: (flags & flag::RECIPE_EFFECT != 0)
                .then(|| {
                    nav.field(root, "m_requiredEffect")
                        .ok()
                        .and_then(|r| nav.states_of_ref(r))
                })
                .flatten(),
            custom,
        })
    }

    /// The outcome of a check value and a roll in [0, 1000]: the first check covering the value
    /// (else the last), its first outcome covering the roll (else its last).
    #[must_use]
    pub fn outcome(&self, value: f32, roll: f32) -> Option<&Outcome> {
        let check = self
            .checks
            .iter()
            .find(|c| c.upto >= value)
            .or_else(|| self.checks.last())?;
        check
            .outcomes
            .iter()
            .find(|o| o.upto >= roll)
            .or_else(|| check.outcomes.last())
    }

    /// Whether the recipe can fail: more than one check.
    #[must_use]
    pub fn checked(&self) -> bool {
        self.checks.len() > 1
    }
}

/// A text field's string (its id and table; none when it is not one).
fn text_of(nav: &Nav<'_>, o: &PackedObject, name: &str) -> (u32, u32) {
    nav.field(o, name)
        .ok()
        .and_then(|r| nav.native(r).ok())
        .and_then(|n| match n {
            NativeObject::TextInfo(t) => Some((t.string_id, t.table)),
            _ => None,
        })
        .unwrap_or((0, 0))
}

fn action(lib: &Library, nav: &Nav<'_>, a: &PackedObject) -> Action {
    let pkg = lib.package_name(a.package_id());
    let w = |n: &str| nav.field(a, n).unwrap_or(0);
    match pkg.as_str() {
        "ProduceRecipeAction" => Action::Produce {
            template: w("m_entityDID"),
            quantity: w("m_uiQuantity").max(1),
            ordinal: w("m_uiOrdinal"),
        },
        "ItemEffectRecipeAction" => Action::ItemEffect {
            ordinal: w("m_uiOrdinal"),
            state: nav
                .field(a, "m_effect")
                .ok()
                .and_then(|r| nav.states_of_ref(r))
                .unwrap_or(0),
            kind: w("m_iEffectKind"),
            spellcraft: nav.number(a, "m_fSpellcraft").unwrap_or(1.0),
            by_spinner: (w("m_flags") & item_effect::BY_SPINNER != 0)
                .then(|| w("m_didMappingTable"))
                .filter(|d| *d != 0 && *d != NULL),
        },
        "DestroyRecipeAction" => Action::Destroy {
            ordinal: w("m_uiOrdinal"),
            quantity: w("m_uiQuantity"),
            by_spinner: (w("m_flags") & flag::ACTION_TABLE != 0)
                .then(|| w("m_didMappingTable"))
                .filter(|d| *d != 0 && *d != NULL),
            spinners: Some((w("m_uiMinSpinnerVal"), w("m_uiMaxSpinnerVal")))
                .filter(|(_, most)| *most != 0),
        },
        "PlayerEffectRecipeAction" => Action::Effect {
            state: nav
                .field(a, "m_effect")
                .ok()
                .and_then(|r| nav.states_of_ref(r))
                .unwrap_or(0),
            spellcraft: nav.number(a, "m_fSpellcraft").unwrap_or(1.0),
        },
        "TextRecipeAction" => {
            let text = nav
                .field(a, "m_siText")
                .ok()
                .and_then(|r| nav.native(r).ok())
                .and_then(|n| match n {
                    NativeObject::TextInfo(t) => Some((t.string_id, t.table)),
                    _ => None,
                })
                .unwrap_or((0, 0));
            Action::Text {
                text,
                kind: w("m_textType"),
            }
        }
        "BindRecipeAction" => Action::Bind {
            ordinal: w("m_uiOrdinal"),
            spinners: (w("m_uiMinSpinnerVal"), w("m_uiMaxSpinnerVal")),
        },
        "AttuneRecipeAction" => Action::Attune {
            ordinal: w("m_uiOrdinal"),
            spinners: (w("m_uiMinSpinnerVal"), w("m_uiMaxSpinnerVal")),
        },
        "MutateRecipeAction" => Action::Mutate {
            ordinal: w("m_uiOrdinal"),
            table: (w("m_flags") & flag::ACTION_TABLE != 0)
                .then(|| w("m_didMappingTable"))
                .filter(|d| *d != 0 && *d != NULL),
            spinners: Some((w("m_uiMinSpinnerVal"), w("m_uiMaxSpinnerVal")))
                .filter(|(_, most)| *most != 0),
        },
        "HarmonizeRecipeAction" => Action::Harmonize {
            ordinal: w("m_uiOrdinal"),
            spinners: (w("m_uiMinSpinnerVal"), w("m_uiMaxSpinnerVal")),
        },
        "CustomSuccessRecipeAction" => Action::Custom,
        // Type 1 adds; the data has no other.
        "FlagRecipeAction" if w("m_type") == 1 => Action::Flag {
            ordinal: w("m_uiOrdinal"),
            flags: w("m_flags"),
            spinners: Some((w("m_uiMinSpinnerVal"), w("m_uiMaxSpinnerVal")))
                .filter(|(_, most)| *most != 0),
        },
        _ => Action::Other,
    }
}

/// A use's share of its pay by the day's experience so far against the day's mark for the level:
/// all of it up to the mark, three quarters past it, half past twice it, a quarter past three
/// times it.
#[must_use]
pub fn day_share(today: u64, mark: u32) -> f64 {
    #[allow(clippy::cast_precision_loss)]
    let r = today as f64 / f64::from(mark.max(1));
    if r > 3.0 {
        0.25
    } else if r > 2.0 {
        0.5
    } else if r > 1.0 {
        0.75
    } else {
        1.0
    }
}

/// The experience a gathering use (butchery, mining, surveying) pays at the craft skill's level,
/// less as the day's experience grows.
#[must_use]
pub fn usage_xp(def: &CraftSkillDef, c: &CraftSkillState) -> u64 {
    let Some(table) = &def.usage_xp else {
        return 0;
    };
    let base = f64::from(table.at(c.level)) * day_share(c.today, def.daily.at(c.level));
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let xp = base.floor() as u64;
    xp
}

/// An award of craft experience shared out: what the player's pool gains (the craft skill's
/// record takes the rest, raising itself as far as it reaches).
#[must_use]
pub fn share_out(def: &CraftSkillDef, c: &mut CraftSkillState, xp: u64) -> u64 {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    let part = |x: u64| (x as f64 * f64::from(def.pool_share)).floor() as u64;
    if c.level >= def.cap {
        let gained = part(xp);
        c.today += gained;
        return gained;
    }
    let pool = part(xp).max(1).min(xp);
    c.today += xp;
    c.available += xp - pool;
    while c.level < def.cap {
        match def.step(c.level) {
            Some(step) if c.available >= step => {
                c.available -= step;
                c.level += 1;
            }
            _ => break,
        }
    }
    pool
}

/// The call carrying one craft skill's record.
#[must_use]
pub fn skill_call(c: &CraftSkillState) -> ServerMessage {
    let mut b = Builder::new();
    b.object(package::SKILL_RECORD.0, &skill_slots(c));
    session::call(function::SKILL, &[Arg::Object(b.graph())]).expect("a record packs")
}

/// Tell a player its pool of craft experience: what is left, then the total.
fn tell_pool(net: &mut ServerNet, world: ConnId, p: &crate::world::Player, total: bool) {
    let q = |k| quals::long(&p.qualities, k).unwrap_or(0);
    progress::send_long(net, world, pool::AVAILABLE, q(pool::AVAILABLE));
    if total {
        progress::send_long(net, world, pool::TOTAL, q(pool::TOTAL));
    }
}

/// A player is awarded craft experience in a craft skill: its pool and the skill's record change,
/// and the client is told both (nothing when the award is nothing).
pub fn award<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, skill: u32, xp: u64) {
    let Some(def) = s.crafting.skills.iter().find(|d| d.did == skill).cloned() else {
        return;
    };
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let Some(c) = p.play.craft_skills.iter_mut().find(|c| c.skill == skill) else {
        return;
    };
    let before = *c;
    let pool_gain = share_out(&def, c, xp);
    let record = *c;
    let gained = record.today - before.today;
    if gained == 0 {
        return;
    }
    for (k, add) in [(pool::AVAILABLE, pool_gain), (pool::TOTAL, gained)] {
        let v = quals::long(&p.qualities, k).unwrap_or(0) + add;
        quals::set_long(&mut p.qualities, k, v);
    }
    tell_pool(net, world, p, true);
    progress::private(net, world, &skill_call(&record));
}

/// The client asks to raise a craft skill: the pool pays what the skill still lacks for its next
/// level. One that cannot be raised, is at its cap, or that the pool cannot pay for is left as it
/// is (the client asks only when it can).
pub fn raise<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, v: &[Value]) {
    let Some(skill) = v.iter().find_map(|x| match x {
        Value::Int(i) => Some(*i),
        _ => None,
    }) else {
        s.unhandled.note("a craft skill raise that does not read");
        return;
    };
    let Some(def) = s.crafting.skills.iter().find(|d| d.did == skill).cloned() else {
        s.unhandled
            .note("raising a craft skill the world does not have");
        return;
    };
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let pooled = quals::long(&p.qualities, pool::AVAILABLE).unwrap_or(0);
    let Some(c) = p.play.craft_skills.iter_mut().find(|c| c.skill == skill) else {
        return;
    };
    let Some(step) = def.step(c.level) else {
        return;
    };
    let cost = step.saturating_sub(c.available);
    if def.cannot_raise || c.level >= def.cap || pooled < cost {
        return;
    }
    c.level += 1;
    c.available = 0;
    let record = *c;
    quals::set_long(&mut p.qualities, pool::AVAILABLE, pooled - cost);
    tell_pool(net, world, p, false);
    progress::private(net, world, &skill_call(&record));
    let t = progress::text_with(
        raised_text::TEXT,
        vec![
            progress::nested(raised_text::SKILL, progress::text(def.name)),
            progress::word(raised_text::RATING, 2, record.level),
        ],
    );
    progress::private(net, world, &crate::chat::show_text(progress::TEXT_KIND, t));
    crate::world::keep_player(s, world);
}

/// The day's craft experience starts over, if it is time: every craft skill's record earns
/// nothing today, the client is told them all, and the next day starts so long after now.
/// Whether it was time.
pub fn day_over<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, now: f64) -> bool {
    let Some(p) = s.players.get_mut(&world) else {
        return false;
    };
    if p.play.craft_skills.is_empty() || now < p.play.craft_reset {
        return false;
    }
    for c in &mut p.play.craft_skills {
        c.today = 0;
    }
    p.play.craft_reset = now + DAY_RESET_SECONDS;
    progress::private(net, world, &skills_call(&p.play));
    crate::world::keep_player(s, world);
    true
}

/// Play one of a craft's effects on the player, for everyone who sees it.
pub fn play_fx<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, effect: u32) {
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let me = p.character;
    let m = ServerMessage::DoFx(crate::combat::player_event(
        p,
        auberean_protocol::physics::Fx {
            fx: effect,
            scalar: 1.0,
        },
    ));
    crate::combat::to_all_seeing(s, net, me, &m);
}

/// The chance in a hundred a craft succeeds, at the player's craft level less the recipe's
/// difficulty: none below -50, then 10 rising a point a step to 30 at -30, two points a step to
/// 70 at -10, a point a step to 90 at +10, half a point a step to 100 at +30.
#[must_use]
pub fn success_chance(margin: i32) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    let m = margin as f32;
    match margin {
        i32::MIN..=-51 => 0.0,
        -50..=-30 => 10.0 + (m + 50.0),
        -29..=-10 => 30.0 + 2.0 * (m + 30.0),
        -9..=10 => 70.0 + (m + 10.0),
        11..=30 => 90.0 + 0.5 * (m - 10.0),
        _ => 100.0,
    }
}

/// A player's level in a craft skill as a making reads it: its own, with what the effects on it
/// add (a hammer's, a crucible's), never past the skill's cap.
#[must_use]
pub fn craft_level<S: Store>(s: &Server<S>, world: ConnId, skill: u32) -> u32 {
    let Some(p) = s.players.get(&world) else {
        return 1;
    };
    let raw = p
        .play
        .craft_skills
        .iter()
        .find(|c| c.skill == skill)
        .map_or(1, |c| c.level);
    let Some(d) = s.crafting.skills.iter().find(|d| d.did == skill) else {
        return raw;
    };
    if d.bonus_stat == 0 {
        return raw;
    }
    let (add, mult) = crate::effects::modifier(&s.effect_table, &p.play.effects, d.bonus_stat);
    let lifted = (i64::from(raw) + i64::from(dereth_primitives::num::to_i32(add))) as f32 * mult;
    u32::try_from(dereth_primitives::num::to_i32(lifted).max(0))
        .unwrap_or(raw)
        .min(d.cap.max(raw))
}

/// The record of a craft skill held, as the registry packs it.
fn skill_slots(c: &CraftSkillState) -> [Slot; 4] {
    [
        Slot::Long(c.today),
        Slot::Tagged(DATUM, c.skill),
        Slot::Long(c.available),
        Slot::Int(c.level),
    ]
}

/// The record of a recipe known, as the registry packs it, with when the client was last told of
/// it (-1 in the registry; the time of the telling in an update).
fn recipe_slots(r: &RecipeState, told: f64) -> [Slot; 9] {
    [
        Slot::Int(0),
        Slot::Time(r.use_again),
        Slot::Int(r.successes),
        Slot::Time(told),
        Slot::Int(r.charges.cast_unsigned()),
        Slot::Tagged(DATUM, r.recipe),
        Slot::Time(-1.0),
        Slot::Int(r.last_spinner),
        Slot::Int(u32::MAX),
    ]
}

/// The call carrying a player's whole craft registry.
#[must_use]
pub fn refresh_call(p: &crate::state::PlayState, score: f32, title: u32) -> ServerMessage {
    let mut b = Builder::new();
    let reg = b.reserve(package::REGISTRY.0);
    let skills_at = b.next_id();
    let first_skill = skills_at + 1;
    let skill_entries: Vec<(Item, Item)> = p
        .craft_skills
        .iter()
        .zip(first_skill..)
        .map(|(c, id)| (Item::Word(c.skill), Item::Ref(id)))
        .collect();
    b.native(
        session::native::WORD_REF_HASH,
        hash(Elem::Word, Elem::Ref, SKILL_TABLE_SIZE, &skill_entries),
    );
    for c in &p.craft_skills {
        b.object(package::SKILL_RECORD.0, &skill_slots(c));
    }
    let recipes_at = b.next_id();
    let first_recipe = recipes_at + 1;
    let recipe_entries: Vec<(Item, Item)> = p
        .recipes
        .iter()
        .zip(first_recipe..)
        .map(|(r, id)| (Item::Word(r.recipe), Item::Ref(id)))
        .collect();
    b.native(
        session::native::WORD_REF_HASH,
        hash(Elem::Word, Elem::Ref, RECIPE_TABLE_SIZE, &recipe_entries),
    );
    for r in &p.recipes {
        b.object(package::RECIPE_RECORD.0, &recipe_slots(r, -1.0));
    }
    b.fill(
        reg,
        &[
            Slot::Ref(None),
            Slot::Tagged(tag::FLOAT, score.to_bits()),
            Slot::Tagged(DATUM, title),
            Slot::Double(p.craft_reset),
            Slot::Ref(Some(skills_at)),
            Slot::Ref(Some(recipes_at)),
        ],
    );
    session::call(function::REFRESH, &[Arg::Object(b.graph())]).expect("a registry packs")
}

/// The call carrying every craft skill's record.
#[must_use]
pub fn skills_call(p: &crate::state::PlayState) -> ServerMessage {
    let mut b = Builder::new();
    let first = b.next_id() + 1;
    let refs = (first..)
        .take(p.craft_skills.len())
        .map(Item::Ref)
        .collect();
    b.native(
        session::native::REF_LIST,
        crate::graph::list(Elem::Ref, refs),
    );
    for c in &p.craft_skills {
        b.object(package::SKILL_RECORD.0, &skill_slots(c));
    }
    session::call(function::SKILLS, &[Arg::Object(b.graph())]).expect("records pack")
}

/// The call carrying one recipe's record.
#[must_use]
pub fn recipe_call(r: &RecipeState, now: f64) -> ServerMessage {
    let mut b = Builder::new();
    b.object(package::RECIPE_RECORD.0, &recipe_slots(r, now));
    session::call(function::UPDATE_RECIPE, &[Arg::Object(b.graph())]).expect("a record packs")
}

/// The call carrying several recipes' records at once, told at `now`.
#[must_use]
pub fn recipes_call(rs: &[RecipeState], now: f64) -> ServerMessage {
    let mut b = Builder::new();
    let first = b.next_id() + 1;
    let refs = (first..).take(rs.len()).map(Item::Ref).collect();
    b.native(
        session::native::REF_LIST,
        crate::graph::list(Elem::Ref, refs),
    );
    for r in rs {
        b.object(package::RECIPE_RECORD.0, &recipe_slots(r, now));
    }
    session::call(function::UPDATE_RECIPES, &[Arg::Object(b.graph())]).expect("records pack")
}

/// The surveyor's map recipes of each region, in the order their call lists them (crystal, iron,
/// stone, wood, silver), with the effect a map drawn puts on the drawer to set their wait and the
/// usage effect of the region's maps that ends it.
pub const MAP_RECIPES: [(u32, &str, [u32; 5]); 3] = [
    (
        0x6F00_170C,
        "Eff_ResetLinvakResetTimers",
        [
            0x6F00_109B,
            0x6F00_1099,
            0x6F00_109C,
            0x6F00_109D,
            0x6F00_109A,
        ],
    ),
    (
        0x6F00_170D,
        "Eff_ResetOmishanResetTimers",
        [
            0x6F00_1096,
            0x6F00_1094,
            0x6F00_1097,
            0x6F00_1098,
            0x6F00_1095,
        ],
    ),
    (
        0x6F00_170E,
        "Eff_ResetOstethResetTimers",
        [
            0x6F00_1091,
            0x6F00_108F,
            0x6F00_1092,
            0x6F00_1093,
            0x6F00_1090,
        ],
    ),
];

/// How long a region's map recipes wait after one of them is drawn, in seconds of game time.
pub const MAP_RECIPES_WAIT: f64 = 300.0;

/// The region whose map recipes an effect sets waiting, by the effect's id.
#[must_use]
pub fn map_region_set_by(effect: u32) -> Option<usize> {
    MAP_RECIPES.iter().position(|(e, _, _)| *e == effect)
}

/// The region whose map recipes a map's usage effect lets be drawn again, by the effect's name.
#[must_use]
pub fn map_region_reset_by(effect: &str) -> Option<usize> {
    MAP_RECIPES.iter().position(|(_, e, _)| *e == effect)
}

/// A region's five map recipes may next be drawn at `until`: each record set so (a recipe the
/// player lacks is held from now on), and the five told in one call.
pub fn map_recipes_wait<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    region: usize,
    until: f64,
    now: f64,
) {
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let mut told = Vec::new();
    for recipe in MAP_RECIPES[region].2 {
        let i = match p.play.recipes.iter().position(|r| r.recipe == recipe) {
            Some(i) => i,
            None => {
                p.play.recipes.push(new_recipe(recipe));
                p.play.recipes.len() - 1
            }
        };
        p.play.recipes[i].use_again = until;
        told.push(p.play.recipes[i]);
    }
    progress::private(net, world, &recipes_call(&told, now));
}

/// A recipe's record for a new learner.
#[must_use]
pub const fn new_recipe(recipe: u32) -> RecipeState {
    RecipeState {
        recipe,
        use_again: -1.0,
        successes: 0,
        charges: -1,
        last_spinner: 0,
    }
}

/// A character's crafting as it enters: a new one gets every craft skill at level 1 and the
/// starting recipes (its craft skills told first); then its whole registry.
pub fn enter<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, clock: Clock) {
    let now = net.game_time(clock.local);
    let skills: Vec<u32> = s.crafting.skills.iter().map(|c| c.did).collect();
    let starting = s.crafting.starting_recipes.clone();
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let new = p.play.craft_skills.is_empty() && !skills.is_empty();
    if new {
        p.play.craft_skills = skills
            .iter()
            .map(|skill| CraftSkillState {
                skill: *skill,
                level: 1,
                today: 0,
                available: 0,
            })
            .collect();
        for r in starting {
            if !p.play.recipes.iter().any(|x| x.recipe == r) {
                p.play.recipes.push(new_recipe(r));
            }
        }
        p.play.craft_reset = now + DAY_RESET_SECONDS;
        progress::private(net, world, &skills_call(&p.play));
    }
    if p.play.craft_skills.is_empty() {
        return;
    }
    // A day that ended while the player was away starts over before the registry comes.
    let over = !new && day_over(s, net, world, now);
    let Some(p) = s.players.get(&world) else {
        return;
    };
    let m = refresh_call(&p.play, NEW_SCORE, NEW_TITLE);
    let next = p.play.craft_reset - now;
    progress::private(net, world, &m);
    if new && !over {
        crate::world::keep_player(s, world);
    }
    s.after(clock, next.max(0.0), Timer::CraftDay { world });
}

/// A player's craft day may be over: if it is, it starts over, and the next is waited for.
pub fn day_due<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, clock: Clock) {
    let now = net.game_time(clock.local);
    if day_over(s, net, world, now) {
        s.after(clock, DAY_RESET_SECONDS, Timer::CraftDay { world });
    }
}

/// A player learns a recipe (unless it knows it).
pub fn learn<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    recipe: u32,
    clock: Clock,
) {
    let charges = s.crafting.recipes.get(&recipe).map_or(-1, |d| d.charges);
    let now = net.game_time(clock.local);
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    if let Some(known) = p.play.recipes.iter_mut().find(|r| r.recipe == recipe) {
        // Granted again, its charges are whole again. Nothing recorded grants one twice; a second
        // toll for a mine's gems has to mean this, or the mine stays spent.
        if known.charges != charges {
            known.charges = charges;
            let rec = *known;
            progress::private(net, world, &recipe_call(&rec, now));
            crate::world::keep_player(s, world);
        }
        return;
    }
    p.play.recipes.push(RecipeState {
        charges,
        ..new_recipe(recipe)
    });
    progress::private(
        net,
        world,
        &session::call(function::ADD_RECIPE, &[Arg::Int(recipe)]).expect("packs"),
    );
    crate::world::keep_player(s, world);
}

/// A player forgets a recipe (if it knows it).
pub fn forget<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, recipe: u32) {
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let Some(i) = p.play.recipes.iter().position(|r| r.recipe == recipe) else {
        return;
    };
    p.play.recipes.remove(i);
    progress::private(
        net,
        world,
        &session::call(function::REMOVE_RECIPE, &[Arg::Int(recipe)]).expect("packs"),
    );
    crate::world::keep_player(s, world);
}

/// Why an item does not go into a recipe's slot at a spinner value (0: it does).
fn check_ingredient<S: Store>(
    s: &Server<S>,
    world: ConnId,
    recipe: u32,
    slot: u32,
    item: u64,
    spinner: u32,
) -> u32 {
    let Some(p) = s.players.get(&world) else {
        return error::NOT_OWNED;
    };
    let Some(def) = s
        .crafting
        .recipes
        .get(&recipe)
        .filter(|_| p.play.recipes.iter().any(|r| r.recipe == recipe))
    else {
        return error::BAD_RECIPE;
    };
    let Some(ing) = def.ingredients.iter().find(|i| i.ordinal == slot) else {
        return error::BAD_RECIPE;
    };
    let Some(held) = p.play.items.iter().find(|h| h.id == item) else {
        return error::NOT_OWNED;
    };
    if !ing.items.contains(&held.template) {
        return error::MISSING_ITEM;
    }
    let props = s.catalog.get(held.template).map(|t| &t.props);
    let own = s.things.get(item).map(|t| t.own).unwrap_or_default();
    let flags = own.craft_flags
        | props
            .and_then(|p| p.int("Item_CraftFlags"))
            .and_then(|v| u32::try_from(v).ok())
            .unwrap_or(0);
    let why = flags_refused(ing, flags);
    if why != 0 {
        return why;
    }
    if held.quantity.max(1) < s.crafting.required(ing, spinner) {
        return error::TOO_FEW;
    }
    let number = |mine: Option<i32>, name: &str| {
        mine.map(i64::from)
            .or_else(|| props.and_then(|p| p.int(name)))
            .unwrap_or(0)
    };
    s.crafting.level_or_lore_refused(
        ing,
        spinner,
        number(own.min_level, "Usage_MinLevel"),
        number(own.arcane_lore, "Usage_RequiredArcaneLore"),
    )
}

/// Why an item's craft flags bar it from a slot (0: they don't).
fn flags_refused(ing: &Ingredient, flags: u32) -> u32 {
    if ing.required_flags & flags == ing.required_flags && ing.restricted_flags & flags == 0 {
        return 0;
    }
    error::LACKS_FLAG
        .iter()
        .find(|(bit, _)| ing.required_flags & bit != 0 && flags & bit == 0)
        .or_else(|| {
            error::HAS_FLAG
                .iter()
                .find(|(bit, _)| ing.restricted_flags & bit != 0 && flags & bit != 0)
        })
        .map_or(error::BARRED_BY_FLAGS, |(_, code)| *code)
}

/// Why a making's slots do not do for a recipe at a spinner value (0: they do): a slot filled that
/// the spinner does not ask for, or one it asks for left empty, are invalid ingredients; else the
/// first item that does not fit says why.
fn check_slots<S: Store>(
    s: &Server<S>,
    world: ConnId,
    def: &RecipeDef,
    spinner: u32,
    slots: &[(u32, u64)],
) -> u32 {
    if slots.len() != def.ingredients_at(spinner).count() {
        return error::INVALID_INGREDIENTS;
    }
    // Spellbinding takes an enchantment: an item with none is refused.
    if def.custom == Custom::Spellbind {
        let holds = slots.first().and_then(|(_, item)| s.things.get(*item));
        if holds.is_none_or(|t| t.enchantments.is_empty() && t.bonuses.is_empty()) {
            return error::INVALID_INGREDIENTS;
        }
    }
    // One stack may fill two slots only with enough of it for both.
    let mut wanted: Vec<(u64, u64)> = Vec::new();
    for ing in def.ingredients_at(spinner) {
        let Some((slot, item)) = slots.iter().find(|(slot, _)| *slot == ing.ordinal) else {
            return error::INVALID_INGREDIENTS;
        };
        let why = check_ingredient(s, world, def.did, *slot, *item, spinner);
        if why != 0 {
            return why;
        }
        let need = u64::from(s.crafting.required(ing, spinner).max(1));
        match wanted.iter_mut().find(|(i, _)| i == item) {
            Some((_, n)) => *n += need,
            None => wanted.push((*item, need)),
        }
    }
    let held = |item: u64| {
        s.players.get(&world).map_or(0, |p| {
            p.play
                .items
                .iter()
                .find(|h| h.id == item)
                .map_or(0, |h| u64::from(h.quantity.max(1)))
        })
    };
    if wanted.iter().any(|(item, n)| held(*item) < *n) {
        return error::TOO_FEW;
    }
    0
}

/// The client puts an ingredient into a recipe's slot: answered with its values and why it does
/// not fit (0 when it does).
pub fn add_ingredient<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    v: &[Value],
) {
    let [Value::Int(spinner), Value::Long(item), Value::Int(slot), Value::Int(recipe)] = v else {
        s.unhandled.note("an ingredient that does not read");
        return;
    };
    let err = check_ingredient(s, world, *recipe, *slot, *item, *spinner);
    let m = session::call(
        function::INGREDIENT_ADDED,
        &[
            Arg::Int(err),
            Arg::Int(*spinner),
            Arg::LongInt(*item),
            Arg::Int(*slot),
            Arg::Int(*recipe),
        ],
    )
    .expect("packs");
    progress::private(net, world, &m);
}

/// The steps of a craft.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// The animation is let go.
    Release,
    /// The result.
    Result,
}

/// A craft under way: the recipe, its ingredients by slot, the context of its animation.
#[derive(Debug, Clone, PartialEq)]
pub struct Making {
    pub recipe: u32,
    pub slots: Vec<(u32, u64)>,
    pub context: u32,
    /// The spinner the player chose (the quality of what it makes).
    pub spinner: u32,
}

/// From the request to the animation, from it to its release, and from the request to the
/// result, by the animation's cycles (2017: two cycles 0.275, 2.57, 4.28 s; three 0.14, 6.1, 8.1;
/// four 0.48, 8.48, 10.39).
#[must_use]
pub fn timing(cycles: u32) -> (f64, f64) {
    match cycles {
        0..=2 => (2.30, 4.28),
        3 => (5.96, 8.10),
        _ => (8.00, 10.39),
    }
}

/// The answer to a craft request: always shown, why it failed (0: it went), the recipe.
#[must_use]
pub fn executed(err: u32, recipe: u32) -> ServerMessage {
    session::call(
        function::EXECUTED,
        &[Arg::Int(1), Arg::Int(err), Arg::Int(recipe)],
    )
    .expect("packs")
}

/// An ALHash's pairs (a word key, a 64-bit value), from its packed bytes: none when its count isn't
/// what they hold.
fn word_long_pairs(bytes: &[u8]) -> Vec<(u32, u64)> {
    let Some((header, body)) = bytes.split_first_chunk::<4>() else {
        return Vec::new();
    };
    let n = (u32::from_le_bytes(*header) & 0x00FF_FFFF) as usize;
    let (pairs, rest) = body.as_chunks::<12>();
    if pairs.len() != n || !rest.is_empty() {
        return Vec::new();
    }
    pairs
        .iter()
        .map(|p| {
            let word = |i: usize| u32::from_le_bytes([p[i], p[i + 1], p[i + 2], p[i + 3]]);
            (word(0), u64::from(word(8)) << 32 | u64::from(word(4)))
        })
        .collect()
}

/// The ingredients a craft's request names by slot, from its table.
#[must_use]
pub fn slots_in(g: &[(u32, Node)]) -> Vec<(u32, u64)> {
    g.iter()
        .find_map(|(_, n)| match n {
            Node::Native { bytes, .. } => Some(word_long_pairs(bytes)),
            _ => None,
        })
        .unwrap_or_default()
}

/// Has a player the money a recipe costs?
fn affords<S: Store>(s: &Server<S>, world: ConnId, def: &RecipeDef) -> bool {
    s.players
        .get(&world)
        .is_some_and(|p| crate::store::money(&p.qualities) >= def.cost)
}

/// The client asks to make a recipe with ingredients by slot.
pub fn execute<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    v: &[Value],
    clock: Clock,
) {
    let [Value::Long(target), Value::Graph(g), Value::Int(spinner), Value::Int(recipe)] = v else {
        s.unhandled.note("a craft that does not read");
        return;
    };
    let slots = slots_in(g);
    let recipe = *recipe;
    let now = net.game_time(clock.local);
    let refuse = |net: &mut ServerNet, err: u32| {
        progress::private(net, world, &executed(err, recipe));
    };
    let Some(p) = s.players.get(&world) else {
        return;
    };
    if p.attacking.is_some() || p.using.is_some() || p.making.is_some() {
        return refuse(net, error::PEACE_REQUIRED);
    }
    if p.dying.is_some() || crate::combat::is_dead(p) || p.trip.is_some() {
        return refuse(net, error::TOO_BUSY);
    }
    let Some(known) = p.play.recipes.iter().find(|r| r.recipe == recipe).copied() else {
        return refuse(net, error::BAD_RECIPE);
    };
    let Some(def) = s.crafting.recipes.get(&recipe).cloned() else {
        return refuse(net, error::BAD_RECIPE);
    };
    if known.charges == 0 {
        return refuse(net, error::NO_CHARGES);
    }
    if known.use_again > now {
        return refuse(net, error::NOT_RESET);
    }
    if !def.targets.is_empty()
        && !presence::content_by_id(&s.content.objects, *target)
            .is_some_and(|i| def.targets.contains(&s.content.objects[i].template))
    {
        return refuse(net, error::WRONG_TARGET);
    }
    if def
        .required_effect
        .is_some_and(|effect| !p.play.effects.iter().any(|e| e.state == effect))
    {
        return refuse(net, error::MISSING_EFFECT);
    }
    if def
        .spinner
        .is_some_and(|(least, most)| !(least..=most).contains(spinner))
    {
        return refuse(net, error::BAD_RECIPE);
    }
    // The client greys out a recipe this far above the player, so no recording shows an answer.
    if def.cutoff.is_some_and(|cutoff| {
        i64::from(def.difficulty_at(*spinner)) - i64::from(craft_level(s, world, def.skill))
            > i64::from(cutoff)
    }) {
        return refuse(net, error::BAD_RECIPE);
    }
    let why = check_slots(s, world, &def, *spinner, &slots);
    if why != 0 {
        return refuse(net, why);
    }
    if !affords(s, world, &def) {
        return refuse(net, error::NO_MONEY);
    }
    // Making a recipe ends a protection, before its behavior.
    crate::effects::unprotect(s, net, world, |_, _| {});
    s.next_context = s.next_context.wrapping_add(1);
    let context = s.next_context;
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    p.making = Some(Making {
        recipe,
        slots,
        context,
        spinner: *spinner,
    });
    let me = p.character;
    let mut b = BehaviorParams {
        bits: BehaviorParams::BEHAVIOR
            | 0x20
            | BehaviorParams::CONTEXT
            | BehaviorParams::HOLD_CYCLES,
        behavior: def.animation,
        hold_cycles: def.cycles,
        context,
        ..BehaviorParams::default()
    };
    b.words[1] = 1;
    let plays = ServerMessage::PlayBehavior(crate::combat::player_event(p, b));
    crate::combat::to_all_seeing(s, net, me, &plays);
    let (release, result) = timing(def.cycles);
    s.after(
        clock,
        release,
        Timer::Craft {
            world,
            step: Step::Release,
        },
    );
    s.after(
        clock,
        result,
        Timer::Craft {
            world,
            step: Step::Result,
        },
    );
}

/// A step of a craft, when it is due.
pub fn step<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    st: Step,
    clock: Clock,
) {
    let Some(m) = s.players.get(&world).and_then(|p| p.making.clone()) else {
        return;
    };
    match st {
        Step::Release => {
            let Some(animation) = s.crafting.recipes.get(&m.recipe).map(|d| d.animation) else {
                return;
            };
            let Some(p) = s.players.get_mut(&world) else {
                return;
            };
            let me = p.character;
            let release = Release {
                behavior: animation,
                params: BehaviorParams {
                    bits: BehaviorParams::CONTEXT,
                    context: m.context,
                    ..BehaviorParams::default()
                },
            };
            let lets_go = ServerMessage::LetGoBehavior(crate::combat::player_event(p, release));
            crate::combat::to_all_seeing(s, net, me, &lets_go);
        }
        Step::Result => result(s, net, world, &m, clock),
    }
}

/// The craft's result: the roll, the outcome's actions in order, the cost, the recipe's record
/// when it waits or has charges, the answer, then the ingredients used up.
fn result<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    m: &Making,
    clock: Clock,
) {
    if let Some(p) = s.players.get_mut(&world) {
        p.making = None;
    }
    let Some(def) = s.crafting.recipes.get(&m.recipe).cloned() else {
        return;
    };
    if def.custom == Custom::Unknown {
        let line = progress::text(NOTHING_PRODUCED);
        progress::private(
            net,
            world,
            &crate::chat::show_text(progress::TEXT_KIND, line),
        );
        progress::private(net, world, &executed(error::FAILED, m.recipe));
        return;
    }
    // An ingredient put down, traded or sold since the request: nothing is made of it.
    let why = check_slots(s, world, &def, m.spinner, &m.slots);
    if why != 0 {
        progress::private(net, world, &executed(why, m.recipe));
        return;
    }
    // So is money spent since the request: nothing is made that isn't paid for.
    if !affords(s, world, &def) {
        progress::private(net, world, &executed(error::NO_MONEY, m.recipe));
        return;
    }
    let level = craft_level(s, world, def.skill);
    let chance = success_chance(
        i32::try_from(level).unwrap_or(0)
            - i32::try_from(def.difficulty_at(m.spinner)).unwrap_or(0),
    );
    let value = if !def.checked() {
        2.0
    } else if s.rng.between(0.0, 100.0) < chance {
        1.0 + s.rng.between(0.000_1, 1.0)
    } else {
        1.0
    };
    let roll = s.rng.between(0.0, 1000.0);
    let Some(outcome) = def.outcome(value, roll).cloned() else {
        return;
    };
    let makes_item = outcome.actions.iter().any(|a| match a {
        Action::Produce { .. } => true,
        Action::Custom => matches!(def.custom, Custom::Refine { .. }),
        _ => false,
    });
    if makes_item
        && s.players
            .get(&world)
            .is_some_and(|p| crate::inventory::pack_full(&p.play.items))
    {
        progress::private(net, world, &executed(error::NO_ROOM, m.recipe));
        return;
    }
    let failed = value <= 1.0 && def.checked();
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    let mut xp = (def.base_xp(m.spinner) as f64 * f64::from(outcome.xp_mod)).floor() as u64;
    let mut used: Vec<(u64, u32)> = Vec::new();
    let mut effects: Vec<(u32, f32)> = Vec::new();
    let mut made: Vec<(u32, u64)> = Vec::new();
    let mut binds: Vec<u32> = Vec::new();
    let mut attunes: Vec<u32> = Vec::new();
    for a in &outcome.actions {
        match a {
            Action::Produce {
                template,
                quantity,
                ordinal,
            } => {
                if let Some(id) = crate::inventory::give_made(s, net, world, *template, *quantity) {
                    made.push((*ordinal, id));
                }
            }
            // An effect on whoever wears or wields the item becomes the item's own (2017: every
            // item made by such a recipe and later worn, 43 of 43, carried its outcome's effects
            // at the spinner chosen); one on the item itself stays with it always.
            Action::ItemEffect {
                ordinal,
                state,
                kind: kind @ (item_effect::WORN | item_effect::STATIC),
                spellcraft,
                by_spinner,
            } => {
                let item = made
                    .iter()
                    .chain(m.slots.iter())
                    .find(|(o, _)| o == ordinal)
                    .map(|(_, i)| *i);
                let at = s
                    .crafting
                    .spellcraft_at(*spellcraft, *by_spinner, m.spinner);
                if let Some(t) = item.and_then(|i| s.things.get_mut(i)) {
                    if *kind == item_effect::WORN {
                        t.bonuses.push((*state, at));
                    } else {
                        t.enchantments.push((*state, at));
                    }
                }
            }
            Action::ItemEffect { .. } | Action::Mutate { .. } | Action::Harmonize { .. } => {}
            Action::Flag {
                ordinal,
                flags,
                spinners,
            } if spinners.is_none_or(|(least, most)| (least..=most).contains(&m.spinner)) => {
                let item = made
                    .iter()
                    .find(|(o, _)| o == ordinal)
                    .map(|(_, i)| *i)
                    .or_else(|| m.slots.iter().find(|(o, _)| o == ordinal).map(|(_, i)| *i));
                if let Some(t) = item.and_then(|i| s.things.get_mut(i)) {
                    t.own.craft_flags |= flags;
                }
            }
            Action::Flag { .. } => {}
            Action::Destroy { .. } => {
                let Some((ordinal, quantity)) = s.crafting.destroyed(a, m.spinner) else {
                    continue;
                };
                if let Some((_, item)) = m.slots.iter().find(|(o, _)| *o == ordinal) {
                    used.push((*item, quantity));
                }
            }
            Action::Effect { state, spellcraft } => effects.push((*state, *spellcraft)),
            Action::Text { text, kind } => {
                progress::private(
                    net,
                    world,
                    &crate::chat::show_text(*kind, progress::text(*text)),
                );
            }
            Action::Bind { ordinal, spinners } => {
                if (spinners.0..=spinners.1).contains(&m.spinner) {
                    binds.push(*ordinal);
                }
            }
            Action::Attune { ordinal, spinners } => {
                if (spinners.0..=spinners.1).contains(&m.spinner) {
                    attunes.push(*ordinal);
                }
            }
            Action::Custom => match def.custom {
                Custom::Refine { product } => {
                    xp = xp.max(refine(s, net, world, &def, &outcome, m.spinner, product));
                }
                Custom::Spellbind => {
                    let source = m.slots.first().map(|(_, item)| *item);
                    if let Some(item) = source {
                        spellbind(s, net, world, item);
                    }
                }
                Custom::Dye => dye(s, net, world, &m.slots),
                Custom::None | Custom::Unknown => {}
            },
            Action::Other => {}
        }
    }
    // What the recipe's item level makes each product (its effects known by now).
    for &(ordinal, item) in &made {
        endow(s, &def, &outcome, ordinal, item, m.spinner);
    }
    // An item of the making that binds when used in combat (2017: the 56 items equipped with the
    // mark were all of templates made by recipes that bind their product).
    for ordinal in binds {
        let item = made
            .iter()
            .chain(m.slots.iter())
            .find(|(o, _)| *o == ordinal)
            .map(|(_, i)| *i);
        if let Some(t) = item.and_then(|i| s.things.get_mut(i)) {
            t.binds_on_use = true;
        }
    }
    // An item of the making it attunes stays with whoever holds it (its outcome's own word; the
    // library's action, and the recipes' text "The final result of this recipe will be attuned").
    for ordinal in attunes {
        let item = made
            .iter()
            .chain(m.slots.iter())
            .find(|(o, _)| *o == ordinal)
            .map(|(_, i)| *i);
        if let Some(t) = item.and_then(|i| s.things.get_mut(i)) {
            t.own.attuned = true;
        }
    }
    // The effects' updates (a quest moved on) follow the text.
    let me = s.players.get(&world).map_or(0, |p| p.character);
    let now = net.game_time(clock.local);
    for (state, spellcraft) in effects {
        if let Some(region) = map_region_set_by(state) {
            map_recipes_wait(s, net, world, region, now + MAP_RECIPES_WAIT, now);
            continue;
        }
        crate::effects::apply(
            s,
            net,
            world,
            state,
            crate::effects::Cast {
                caster: me,
                spellcraft,
                ..crate::effects::Cast::default()
            },
            clock,
        );
    }
    award(s, net, world, def.skill, xp);
    if def.checked() {
        play_fx(s, net, world, if failed { fx::FAILED } else { fx::MADE });
    }
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    if def.cost > 0 {
        let money = crate::store::money(&p.qualities);
        let left = money.saturating_sub(def.cost);
        quals::set_int(
            &mut p.qualities,
            crate::store::MONEY,
            i32::try_from(left).unwrap_or(i32::MAX),
        );
        progress::send_int(
            net,
            world,
            crate::store::MONEY,
            i32::try_from(left).unwrap_or(i32::MAX),
        );
    }
    if let Some(r) = p.play.recipes.iter_mut().find(|r| r.recipe == m.recipe) {
        if !failed {
            r.successes += 1;
        }
        let waits = def.recovery > 0.0;
        if waits {
            r.use_again = now + f64::from(def.recovery);
        } else if def.charges >= 0 && !failed {
            // A recipe with charges is marked used now (2017: 118 of 118 such crafts).
            r.use_again = now;
        }
        if r.charges > 0 {
            r.charges -= 1;
        }
        if waits || def.charges >= 0 {
            let rec = *r;
            progress::private(net, world, &recipe_call(&rec, now));
        }
    }
    progress::private(
        net,
        world,
        &executed(if failed { error::FAILED } else { 0 }, m.recipe),
    );
    for (item, quantity) in used {
        use_up(s, net, world, item, quantity);
    }
    crate::world::keep_player(s, world);
}

/// A product takes what its outcome's mutations say at the spinner chosen: see [`crate::made`].
fn endow<S: Store>(
    s: &mut Server<S>,
    def: &RecipeDef,
    outcome: &Outcome,
    ordinal: u32,
    item: u64,
    spinner: u32,
) {
    let Some(tp) = s.things.get(item).and_then(|t| s.catalog.get(t.template)) else {
        return;
    };
    let table = &s.effect_table;
    let Some(own) = made_of(
        &s.crafting,
        def,
        outcome,
        ordinal,
        spinner,
        &tp.props,
        &|state| table.get(state).cloned(),
        &mut s.rng,
    ) else {
        return;
    };
    if let Some(t) = s.things.get_mut(item) {
        if own.damage.is_some() || own.armor.is_some() {
            t.own.min_level = Some(i32::try_from(spinner).unwrap_or(i32::MAX));
        }
        t.value = own.value;
        t.own.made = Some(own);
    }
}

/// The checks' values an expert making's outcome covers up to, and a flawed one's at most.
const EXPERT_CHECK: f32 = 2.0;
const FLAWED_CHECK: f32 = 1.02;

/// What a product of a template carries at a spinner, drawing what differs between items of one
/// level; none when its outcome does not give it its level.
#[allow(clippy::too_many_arguments)]
pub fn made_of(
    crafting: &Crafting,
    def: &RecipeDef,
    outcome: &Outcome,
    ordinal: u32,
    spinner: u32,
    props: &yalain_content::Props,
    effect: &dyn Fn(u32) -> Option<crate::effect_defs::EffectDef>,
    rng: &mut crate::rng::Rng,
) -> Option<crate::made::Made> {
    use crate::made;
    let within = |spinners: &Option<(u32, u32)>| {
        spinners.is_none_or(|(least, most)| (least..=most).contains(&spinner))
    };
    let (mut named, mut quality, mut harmonic) = (false, None, false);
    let mut tool = None;
    let mut enchantments = Vec::new();
    for a in &outcome.actions {
        match a {
            Action::Mutate {
                ordinal: o,
                table: Some(table),
                spinners,
            } if *o == ordinal && within(spinners) => {
                if *table == made::ITEM_LEVEL_TABLE {
                    named = true;
                } else if *table == made::GATHERING_TOOL_TABLE || *table == made::WORKING_TOOL_TABLE
                {
                    tool = crafting
                        .spinner_tables
                        .get(table)
                        .map(|t| (*table, t.at(spinner)));
                } else if let Some(t) = crafting.spinner_tables.get(table) {
                    quality = Some(made::quality_level(t.at(spinner)));
                }
            }
            Action::Harmonize {
                ordinal: o,
                spinners,
            } if *o == ordinal && (spinners.0..=spinners.1).contains(&spinner) => harmonic = true,
            Action::ItemEffect {
                ordinal: o,
                state,
                kind: item_effect::STATIC,
                spellcraft,
                by_spinner,
            } if *o == ordinal => {
                enchantments.push((
                    *state,
                    crafting.spellcraft_at(*spellcraft, *by_spinner, spinner),
                ));
            }
            _ => {}
        }
    }
    if let Some((table, craft)) = tool {
        let check = def.checks.iter().find(|c| c.outcomes.contains(outcome));
        let (expert, flawed) = check.map_or((false, false), |c| {
            (c.upto >= EXPERT_CHECK, c.upto <= FLAWED_CHECK)
        });
        return Some(made::Made {
            durability: Some(made::tool_durability(expert, flawed)),
            tool_yield: (table == made::GATHERING_TOOL_TABLE)
                .then_some(spinner + made::TOOL_YIELD_ABOVE_LEVEL),
            craft_needed: Some(craft),
            ..made::Made::default()
        });
    }
    if !named {
        return None;
    }
    let is_weapon = props.int("Weapon_Damage").is_some();
    let is_armor = props.int("Item_ArmorLevel").is_some();
    let places = props.word("Item_ValidInventoryLocations").unwrap_or(0);
    let precluded = props.word("Item_PrecludedInventoryLocations").unwrap_or(0);
    let variance = props.float("Weapon_Variance").unwrap_or(0.0);
    let mut own = made::Made {
        prefix: made::prefix(spinner),
        ..made::Made::default()
    };
    let Some(q) = quality else {
        // Trade goods are worth what went into them.
        let units = |o: u32| {
            def.ingredients
                .iter()
                .filter(|i| i.ordinal == o && i.asked_at(spinner))
                .map(|i| crafting.required(i, spinner))
                .sum::<u32>()
        };
        own.value = made::trade_goods_value(units(1), units(3));
        return Some(own);
    };
    let band = |r: &mut crate::rng::Rng, (least, most): (i32, i32)| {
        least + i32::try_from(r.below(usize::try_from(most - least + 1).unwrap_or(1))).unwrap_or(0)
    };
    let (low, high) = made::equipment_value(spinner);
    own.value = low + u32::try_from(rng.below((high - low + 1) as usize)).unwrap_or(0);
    if is_weapon {
        let most = if is_armor {
            made::shield_damage(q)
        } else {
            made::damage(q)
        };
        own.damage = Some((most, variance));
        own.delay = Some(band(rng, made::WEAPON_DELAY));
        own.vigor = Some((made::vigor(spinner) - i32::from(rng.chance(0.5))).max(0));
        own.slots = if is_armor { 1 } else { 2 };
        if is_armor {
            own.armor = Some(made::shield_rating(q));
        }
        own.harmonic = harmonic && !is_armor;
    } else if is_armor {
        let piece = made::Piece::of(places, precluded);
        own.armor = Some(made::armor_rating(piece, q));
        own.delay = Some(band(rng, made::ARMOR_DELAY));
        if piece == made::Piece::Robe {
            own.slots = 3;
            own.covers = precluded;
        } else {
            own.slots = 1;
        }
    }
    // What its own permanent effects do to its numbers.
    for (state, sc) in enchantments {
        let Some(d) = effect(state) else {
            continue;
        };
        for (stat, multiply, by) in d.changes(sc) {
            if !multiply {
                continue;
            }
            if let Some((most, variance)) = own.damage.as_mut() {
                match stat {
                    made::stat::DAMAGE => {
                        #[allow(clippy::cast_precision_loss)]
                        let more = *most as f32 * by;
                        *most = dereth_primitives::num::to_i32(more.round());
                        own.damage_changed = true;
                    }
                    made::stat::SPAN => {
                        *variance *= by;
                        own.damage_changed = true;
                    }
                    made::stat::VIGOR => {
                        if let Some(v) = own.vigor.as_mut() {
                            #[allow(clippy::cast_precision_loss)]
                            let less = *v as f32 * by;
                            *v = dereth_primitives::num::to_i32(less.round());
                        }
                        own.vigor_changed = true;
                    }
                    _ => {}
                }
            }
        }
    }
    Some(own)
}

/// Dyeing: the armor in the first slot takes the value of the plant in the second; on a wearer it
/// shows at once.
fn dye<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, slots: &[(u32, u64)]) {
    let of = |o: u32| slots.iter().find(|(x, _)| *x == o).map(|(_, i)| *i);
    let (Some(armor), Some(plant)) = (of(1), of(2)) else {
        return;
    };
    let props = |item: u64| {
        s.things
            .get(item)
            .and_then(|t| s.catalog.get(t.template))
            .map(|t| &t.props)
    };
    let Some(value) = props(plant).and_then(|p| p.float("Craft_DyePlant_Mod")) else {
        return;
    };
    let Some(table) = props(armor).and_then(crate::equip::dye_table) else {
        return;
    };
    let look = s.players.get(&world).and_then(|p| {
        p.play
            .items
            .iter()
            .find(|h| h.id == armor && h.location & crate::equip::BODY_PLACES != 0)?;
        let t = s.things.get(armor)?;
        let tp = s.catalog.get(t.template)?;
        let own = crate::treasure::Own {
            dye: Some((table, value)),
            ..t.own
        };
        crate::equip::worn_look(&tp.props, p.race, p.sex, &own)
    });
    if let Some(t) = s.things.get_mut(armor) {
        t.own.dye = Some((table, value));
    }
    if let Some((file, values)) = look {
        if let Some(p) = s.players.get_mut(&world) {
            crate::equip::put_on(&mut p.look, file, values);
        }
        crate::equip::show_look(s, net, world);
    }
}

/// Spellbinding: a totem of the kind of the item taken apart, in the pack, made by the player.
fn spellbind<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, item: u64) {
    let Some(kind) = totem_of(s, item) else {
        return;
    };
    let Some(source) = s.things.get(item) else {
        return;
    };
    let on_itself = source.enchantments.first().copied();
    let on_wearer = source.bonuses.first().copied();
    let (least_level, lore) = (source.own.min_level, source.own.arcane_lore);
    let Some(id) = crate::inventory::give_made(s, net, world, kind, 1) else {
        return;
    };
    if let Some(t) = s.things.get_mut(id) {
        t.own.min_level = least_level;
        t.own.arcane_lore = lore;
        t.own.made = Some(crate::made::Made::default());
        t.enchantments.extend(on_itself);
        if on_itself.is_none() {
            t.bonuses.extend(on_wearer);
        }
    }
    crate::world::keep_player(s, world);
}

/// The totem an item taken apart makes, by what it is.
fn totem_of<S: Store>(s: &Server<S>, item: u64) -> Option<u32> {
    let template = s.things.get(item).map(|t| t.template);
    let props = template.and_then(|t| s.catalog.get(t)).map(|t| &t.props);
    let is_weapon = props.is_some_and(|p| p.int("Weapon_Damage").is_some());
    let is_armor = props.is_some_and(|p| p.int("Item_ArmorLevel").is_some());
    Some(match (is_weapon, is_armor) {
        (true, true) => totem::SHIELD,
        (true, false) => totem::WEAPON,
        (false, _) => totem::ARMOR,
    })
}

/// A totem used on an item: refused if the item isn't one it goes on, or lacks what it asks; else
/// the item holds its enchantment, in a slot it had free, in place of a weaker of the same class,
/// and the totem is gone (2017: 2 uses, each answered as spent; asked with no target, 4 of 4 went
/// unanswered). What it refuses, and how it answers, are our choice.
pub fn use_totem<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    u: &crate::places::Use,
) {
    use crate::item_use::flags;
    let held = |id: u64| {
        s.players
            .get(&world)
            .is_some_and(|p| p.play.items.iter().any(|h| h.id == id))
    };
    if !held(u.item) || u.target == 0 {
        return;
    }
    let answer = if held(u.target) && enchant_from_totem(s, u.item, u.target) {
        use_up(s, net, world, u.item, 1);
        crate::places::use_answer_on(u, flags::PLAYED | flags::SPENT, 0.0, 0)
    } else {
        crate::places::use_answer_on(u, flags::PLAYED, 0.0, error::INVALID_INGREDIENTS)
    };
    progress::private(net, world, &answer);
    crate::world::keep_player(s, world);
}

/// Put a totem's enchantment on an item, if the item may take it. Returns whether it did.
fn enchant_from_totem<S: Store>(s: &mut Server<S>, from: u64, on: u64) -> bool {
    let Some(t) = s.things.get(from) else {
        return false;
    };
    let (ask_level, ask_lore) = (t.own.min_level.unwrap_or(0), t.own.arcane_lore.unwrap_or(0));
    let totem_template = t.template;
    let on_wearer = t.bonuses.first().copied();
    let Some((state, spellcraft)) = t.enchantments.first().copied().or(on_wearer) else {
        return false;
    };
    let Some(target) = s.things.get(on) else {
        return false;
    };
    if totem_of(s, on) != Some(totem_template) || totem::noun(target.template).is_some() {
        return false;
    }
    let of_template = |name: &str| {
        s.catalog
            .get(target.template)
            .and_then(|t| t.props.int(name))
            .and_then(|v| i32::try_from(v).ok())
    };
    let level = target
        .own
        .min_level
        .or_else(|| of_template("Usage_MinLevel"))
        .unwrap_or(0);
    let lore = target.own.arcane_lore.unwrap_or(0);
    let Some(slots) = target.own.made.map(|m| m.slots) else {
        return false;
    };
    if level < ask_level || lore < ask_lore || slots == 0 {
        return false;
    }
    // One of the same class: the stronger stays.
    let def = s.effect_table.get(state).cloned();
    let strength = |d: &crate::effect_defs::EffectDef, at: f32| d.priority.at(at).unwrap_or(0.0);
    let holds: Vec<(u32, f32)> = target
        .enchantments
        .iter()
        .chain(&target.bonuses)
        .copied()
        .collect();
    let mut replaced = None;
    if let Some(new) = &def {
        for (old_state, at) in holds {
            let Some(old) = s.effect_table.get(old_state) else {
                continue;
            };
            if old.eq_class != 0 && old.eq_class == new.eq_class {
                if strength(old, at) >= strength(new, spellcraft) {
                    return false;
                }
                replaced = Some(old_state);
            }
        }
    }
    let on_itself = on_wearer.is_none_or(|(w, _)| w != state);
    let Some(item) = s.things.get_mut(on) else {
        return false;
    };
    if let Some(old) = replaced {
        item.enchantments.retain(|(x, _)| *x != old);
        item.bonuses.retain(|(x, _)| *x != old);
    } else if let Some(m) = item.own.made.as_mut() {
        m.slots -= 1;
    }
    if on_itself {
        item.enchantments.push((state, spellcraft));
    } else {
        item.bonuses.push((state, spellcraft));
    }
    let permanent = !matches!(
        yalain_content::treasure_effects::tier_of(state),
        Some((yalain_content::treasure_effects::When::Hit, _))
    );
    if let (true, Some(d)) = (permanent && on_itself, def) {
        crate::treasure::apply_numbers(&mut item.own, &d, spellcraft);
    }
    true
}

/// A refining: the units it yields go to the player's stack, a line says how many. What it pays:
/// what the craft skill's table pays for a use at its level.
fn refine<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    def: &RecipeDef,
    outcome: &Outcome,
    spinner: u32,
    product: u32,
) -> u64 {
    let amount = outcome
        .actions
        .iter()
        .filter_map(|a| s.crafting.destroyed(a, spinner))
        .map(|(_, n)| n)
        .max()
        .unwrap_or(0);
    let Some(c) = s
        .players
        .get(&world)
        .and_then(|p| p.play.craft_skills.iter().find(|c| c.skill == def.skill))
        .copied()
    else {
        return 0;
    };
    let got = refining_yield(amount, craft_level(s, world, def.skill));
    let name = s.catalog.get(product).and_then(|t| t.facts.name);
    crate::inventory::give_stacked(s, net, world, product, got);
    if let Some(name) = name {
        let line = progress::text_with(
            refined_text::TEXT,
            vec![
                progress::word(refined_text::COUNT, 2, got),
                progress::nested(refined_text::PRODUCT, progress::text(name)),
            ],
        );
        progress::private(
            net,
            world,
            &crate::chat::show_text(progress::TEXT_KIND, line),
        );
    }
    s.crafting
        .skills
        .iter()
        .find(|d| d.did == def.skill)
        .map_or(0, |d| usage_xp(d, &c))
}

/// Use up so many of a held item: a whole stack leaves the pack, part of one shows its new
/// quantity.
pub(crate) fn use_up<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    item: u64,
    n: u32,
) {
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let Some(h) = p.play.items.iter_mut().find(|h| h.id == item) else {
        return;
    };
    if h.quantity.max(1) <= n {
        let template = h.template;
        crate::inventory::take_item(s, net, world, item, template);
        return;
    }
    h.quantity -= n;
    let left = h.quantity;
    if let Some(t) = s.things.get_mut(item) {
        t.quantity = left;
    }
    let m = ServerMessage::UpdateIntVisual(VisualQuality {
        object: crate::inventory::visual(s, item),
        quality: QUANTITY,
        value: i32::try_from(left).unwrap_or(i32::MAX),
    });
    crate::inventory::to_holder(s, net, world, item, &m);
}
