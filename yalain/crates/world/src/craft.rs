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
//! item not the player's, not one of the slot's items or of their classes, or too few of a stack.
//! The client writes the reason.
//!
//! **Making a recipe**: refused at once (only the answer, with why) when the player is not at
//! peace, the recipe is unknown or still waiting, or a slot lacks what it takes. Else the player
//! plays the recipe's animation for its cycles, lets it go, and some seconds after the request the
//! result: the check rolled against the recipe's difficulty and the player's craft skill, and the
//! outcome's actions in order (items made in the pack, ingredients used up, an effect on the
//! player such as a quest moved on, a text), the cost paid, the recipe's wait and charges told, the
//! answer; ingredients used up after the answer.
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
use crate::{progress, quals, session, Clock, Server, Timer};

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
    pub const NOT_RESET: u32 = 40_024;
    pub const FAILED: u32 = 40_025;
    pub const PEACE_REQUIRED: u32 = 40_026;
}

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
    /// Use up so many of a slot's ingredient.
    Destroy { ordinal: u32, quantity: u32 },
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
    /// Marks on the product the client does not show.
    Other,
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
    pub checks: Vec<Check>,
    /// The craft experience by the spinner chosen (none: the fixed `craft_xp`).
    pub xp_map: Option<LevelMap>,
    /// The difficulty by the spinner chosen (none: the fixed `difficulty`).
    pub difficulty_map: Option<LevelMap>,
}

impl RecipeDef {
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
                        Action::ItemEffect { by_spinner, .. } => *by_spinner,
                        _ => None,
                    })
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
        let mut ingredients: Vec<Ingredient> = nav
            .list(root, "m_ingredients")?
            .into_iter()
            .map(|o| Ingredient {
                ordinal: nav.field(o, "m_uiOrdinal").unwrap_or(0),
                quantity: nav.field(o, "m_uiQuantity").unwrap_or(1),
                items: keys(o, "m_hashItems"),
                classes: keys(o, "m_hashItemClasses"),
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
        for (_, v) in entries {
            let Some(o) = v.reference().and_then(|r| nav.object(r).ok()) else {
                continue;
            };
            let upto = nav.number(o, "m_fThresh").unwrap_or(2.0);
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
        Ok(Self {
            did,
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
            checks,
            xp_map: None,
            difficulty_map: None,
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
pub fn learn<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, recipe: u32) {
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    if p.play.recipes.iter().any(|r| r.recipe == recipe) {
        return;
    }
    p.play.recipes.push(new_recipe(recipe));
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

/// Does an item of a template fit an ingredient slot (one of its templates, or of a class the
/// template derives from)?
fn fits<S: Store>(s: &Server<S>, ing: &Ingredient, template: u32) -> bool {
    if ing.items.contains(&template) {
        return true;
    }
    s.catalog.get(template).is_some_and(|t| {
        ing.classes.iter().any(|c| {
            t.props
                .int("Item_Class")
                .is_some_and(|x| x == i64::from(*c))
        })
    })
}

/// Why an item does not go into a recipe's slot (0: it does).
fn check_ingredient<S: Store>(
    s: &Server<S>,
    world: ConnId,
    recipe: u32,
    slot: u32,
    item: u64,
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
    if !fits(s, ing, held.template) {
        return error::MISSING_ITEM;
    }
    if held.quantity.max(1) < ing.quantity {
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
    let err = check_ingredient(s, world, *recipe, *slot, *item);
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

/// An ALHash's pairs (a word key, a 64-bit value), from its packed bytes.
fn word_long_pairs(bytes: &[u8]) -> Vec<(u32, u64)> {
    let word = |i: usize| -> Option<u32> {
        bytes
            .get(i..i + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    };
    let Some(h) = word(0) else {
        return Vec::new();
    };
    let n = (h & 0x00FF_FFFF) as usize;
    (0..n)
        .filter_map(|k| {
            let at = 4 + k * 12;
            let key = word(at)?;
            let lo = word(at + 4)?;
            let hi = word(at + 8)?;
            Some((key, u64::from(hi) << 32 | u64::from(lo)))
        })
        .collect()
}

/// The client asks to make a recipe with ingredients by slot.
pub fn execute<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    v: &[Value],
    clock: Clock,
) {
    let [Value::Long(_target), Value::Graph(g), Value::Int(spinner), Value::Int(recipe)] = v else {
        s.unhandled.note("a craft that does not read");
        return;
    };
    let slots: Vec<(u32, u64)> = g
        .iter()
        .find_map(|(_, n)| match n {
            Node::Native { bytes, .. } => Some(word_long_pairs(bytes)),
            _ => None,
        })
        .unwrap_or_default();
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
    let Some(known) = p.play.recipes.iter().find(|r| r.recipe == recipe).copied() else {
        return refuse(net, error::BAD_RECIPE);
    };
    let Some(def) = s.crafting.recipes.get(&recipe).cloned() else {
        return refuse(net, error::BAD_RECIPE);
    };
    if known.use_again > now || known.charges == 0 {
        return refuse(net, error::NOT_RESET);
    }
    for ing in &def.ingredients {
        let ok = slots
            .iter()
            .find(|(slot, _)| *slot == ing.ordinal)
            .is_some_and(|(slot, item)| check_ingredient(s, world, recipe, *slot, *item) == 0);
        if !ok {
            return refuse(net, error::INVALID_INGREDIENTS);
        }
    }
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
    let level = s
        .players
        .get(&world)
        .and_then(|p| p.play.craft_skills.iter().find(|c| c.skill == def.skill))
        .map_or(1, |c| c.level);
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
    let failed = value <= 1.0 && def.checked();
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    let xp = (def.base_xp(m.spinner) as f64 * f64::from(outcome.xp_mod)).floor() as u64;
    let mut used: Vec<(u64, u32)> = Vec::new();
    let mut effects: Vec<(u32, f32)> = Vec::new();
    let mut made: Vec<(u32, u64)> = Vec::new();
    let mut binds: Vec<u32> = Vec::new();
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
            // at the spinner chosen).
            Action::ItemEffect {
                ordinal,
                state,
                kind: item_effect::WORN,
                spellcraft,
                by_spinner,
            } => {
                let item = made
                    .iter()
                    .chain(m.slots.iter())
                    .find(|(o, _)| o == ordinal)
                    .map(|(_, i)| *i);
                #[allow(clippy::cast_precision_loss)]
                let at = by_spinner.map_or(*spellcraft, |table| {
                    s.crafting
                        .spinner_tables
                        .get(&table)
                        .map_or(m.spinner, |t| t.at(m.spinner)) as f32
                });
                if let Some(t) = item.and_then(|i| s.things.get_mut(i)) {
                    t.bonuses.push((*state, at));
                }
            }
            Action::ItemEffect { .. } => {}
            Action::Destroy { ordinal, quantity } => {
                if let Some((_, item)) = m.slots.iter().find(|(o, _)| o == ordinal) {
                    used.push((*item, *quantity));
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
            Action::Other => {}
        }
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
    progress::private(net, world, &m);
}
