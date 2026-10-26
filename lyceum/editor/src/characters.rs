//! Characters: recipes in the world folder, each a player to play as, made into a playtest
//! character by the editor's own server.
//!
//! A recipe (`characters/<Name>.toml` beside `world.toml`) says what a player is: a level, an
//! archetype, the gear it carries, and what a player chooses beyond the archetype's (the heritage,
//! the innate split at creation, the skills and how the ranks are shared). It never holds a rank
//! or a raised value: the build is worked out from the data files' own tables
//! (`dereth-progression`), the same way the server
//! works it out when it makes the character, so what the tab shows is what the server makes. The
//! character itself lives in the server's database; after each apply the server writes what it
//! made (`.playtest/characters/<Name>.json`), and Play As compares that file's recipe hash with
//! the recipe's own to know whether the character is still the recipe's. Before it is made, the
//! server says what the recipe would make (`.playtest/previews/`): the gear it rolls and the items
//! it gives, which the same recipe always makes again.
//!
//! A builder's own archetypes (`archetypes/<Name>.toml` in the editor's settings folder) are a
//! recipe's choices without its level: its heritage, its innate split, its skills and how it
//! shares its ranks. Choosing one copies its choices into the recipe, so a world's recipes never
//! depend on one builder's settings.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use dereth_assets::tables::{Attribute2ndTable, CharGen, SkillTable, XpTable};
use dereth_progression::augmentations::Costs;
use dereth_progression::extras::{Luminous, Mastery};
use dereth_progression::gear::Slot;
use dereth_progression::order::{
    ExportNote, Gear, Item, Made, Order, SlotOrder, EXPORT_VERSION, ORDER_VERSION,
};
use dereth_progression::{check, Archetype, Build, Extras, Finding, Plan, Tables};
use serde::{Deserialize, Serialize};

/// The version of a recipe file this editor writes.
pub const RECIPE_VERSION: u32 = 2;

/// What gear a recipe asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GearChoice {
    /// None: the character keeps what it has.
    None,
    /// What a new character of its skills is given.
    Starter,
    /// Loot like what the place drops, or a character of its level hunts.
    #[default]
    Level,
    /// A tier up, the best of more rolls, and jewelry, a cloak and a trinket.
    Better,
    /// Loot of the recipe's tier.
    Tier,
}

/// One gear slot as the recipe asks for it: rolled again some times, or filled with a weenie (by
/// symbol or server class number) in place of loot.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecipeSlot {
    #[serde(default, skip_serializing_if = "is_zero")]
    pub roll: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weenie: Option<String>,
}

#[allow(clippy::trivially_copy_pass_by_ref)] // serde's signature
const fn is_zero(n: &u32) -> bool {
    *n == 0
}

/// Where Play As puts the character.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Start {
    /// On the floor of the camera's cell, or the ground under it.
    #[default]
    Camera,
    /// In front of the selected object, on its floor.
    Selected,
    /// Where the dungeon's portals drop it.
    Entrance,
    /// Where it was left: no teleport.
    Left,
}

impl Start {
    /// Every start, as the tab offers them.
    pub const ALL: [Self; 4] = [Self::Camera, Self::Selected, Self::Entrance, Self::Left];

    /// Its words' key in the text.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Camera => "character_gear.start_camera",
            Self::Selected => "character_gear.start_selected",
            Self::Entrance => "character_gear.start_entrance",
            Self::Left => "character_gear.start_left",
        }
    }

    fn is_camera(&self) -> bool {
        *self == Self::Camera
    }
}

/// A character's gender.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Gender {
    #[default]
    Male,
    Female,
}

impl Gender {
    /// Its id in the creation table.
    #[must_use]
    pub const fn id(self) -> u32 {
        match self {
            Self::Male => dereth_progression::gender::MALE,
            Self::Female => dereth_progression::gender::FEMALE,
        }
    }
}

/// Heritage `id`'s name as the game shows it to players: the creation table's own name `name`,
/// but for the one it names by its kind (`Gear`) and the Olthoi spitter (`OlthoiAcid`). A recipe
/// keeps the table's name.
#[must_use]
pub fn heritage_title(id: u32, name: &str) -> String {
    match id {
        dereth_progression::heritage::GEARKNIGHT => "Gear Knight".to_owned(),
        dereth_progression::heritage::OLTHOI_ACID => "Olthoi Spitter".to_owned(),
        _ => name.to_owned(),
    }
}

/// An item a recipe gives, by symbol (`pack:name`) or server class number, and how many.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecipeItem {
    pub weenie: String,
    #[serde(default = "one")]
    pub count: u32,
}

const fn one() -> u32 {
    1
}

const fn yes() -> bool {
    true
}

/// A player to play as.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recipe {
    pub version: u32,
    pub level: u32,
    /// A built-in archetype's id.
    pub archetype: String,
    #[serde(default)]
    pub gear: GearChoice,
    /// The tier, when the gear is of a chosen tier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tier: Option<u32>,
    /// Buffed as it enters.
    #[serde(default = "yes")]
    pub buffed: bool,
    /// The level of its buffs; by its level when not given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub buff_level: Option<u32>,
    /// The seed its gear is rolled with.
    #[serde(default)]
    pub seed: u64,
    /// The heritage, by the data files' name; Aluvian when not given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heritage: Option<String>,
    #[serde(default, skip_serializing_if = "is_male")]
    pub gender: Gender,
    /// The innate split, in place of the archetype's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub innate: Option<[u32; 6]>,
    /// The skills to specialize and train, most wanted first, in place of the archetype's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub specialized: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trained: Option<Vec<String>>,
    /// How the ranks are shared, in place of the archetype's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weights: Option<BTreeMap<String, u32>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<RecipeItem>,
    /// The kit it carries, by id: by its archetype when not given, `none` for none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kit: Option<String>,
    /// The gear slots rolled again, or filled with a chosen weenie.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub slots: BTreeMap<Slot, RecipeSlot>,
    /// Where Play As puts it.
    #[serde(default, skip_serializing_if = "Start::is_camera")]
    pub start: Start,
    /// The augmentations it bought with experience, by id, and how many; the heritage's free one
    /// is held besides.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub augmentations: BTreeMap<String, u32>,
    /// Whether it can earn luminance (from level 200), and the auras it bought.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub luminance: Option<Luminous>,
    /// The titles it holds, by id, the one shown first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub titles: Vec<u32>,
    /// The quests it has solved, by name, and how many times.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub quests: BTreeMap<String, u32>,
    /// The quests that grant skill credits, solved as far as its level allows.
    #[serde(default, skip_serializing_if = "is_false")]
    pub quest_credits: bool,
    /// Its summoning mastery; none when not given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summoning_mastery: Option<Mastery>,
}

#[allow(clippy::trivially_copy_pass_by_ref)] // serde's skip test takes a reference
fn is_male(g: &Gender) -> bool {
    *g == Gender::Male
}

#[allow(clippy::trivially_copy_pass_by_ref)] // serde's skip test takes a reference
const fn is_false(b: &bool) -> bool {
    !*b
}

impl Default for Recipe {
    fn default() -> Self {
        Self {
            version: RECIPE_VERSION,
            level: 1,
            archetype: "melee-heavy".into(),
            gear: GearChoice::Level,
            tier: None,
            buffed: true,
            buff_level: None,
            seed: 1,
            heritage: None,
            gender: Gender::Male,
            innate: None,
            specialized: None,
            trained: None,
            weights: None,
            items: Vec::new(),
            kit: None,
            slots: BTreeMap::new(),
            start: Start::Camera,
            augmentations: BTreeMap::new(),
            luminance: None,
            titles: Vec::new(),
            quests: BTreeMap::new(),
            quest_credits: false,
            summoning_mastery: None,
        }
    }
}

impl Recipe {
    /// What its player chose: its archetype's plan, with its own innate split, skill lists and
    /// shares where it has them.
    ///
    /// # Errors
    /// An archetype Lyceum does not have.
    pub fn plan(&self) -> Result<Plan, String> {
        let mut plan = Archetype::named(&self.archetype)
            .ok_or_else(|| format!("There is no archetype {:?}.", self.archetype))?
            .plan;
        if let Some(innate) = self.innate {
            plan.innate = innate;
        }
        if let Some(s) = &self.specialized {
            plan.specialized.clone_from(s);
        }
        if let Some(s) = &self.trained {
            plan.trained.clone_from(s);
        }
        if let Some(w) = &self.weights {
            plan.weights.clone_from(w);
        }
        Ok(plan)
    }

    /// Change its plan with `edit`: from then on the recipe holds the whole of what it changed
    /// (all its skills, its innate split or its shares) rather than the archetype's.
    ///
    /// # Errors
    /// An archetype Lyceum does not have.
    pub fn edit_plan(&mut self, edit: impl FnOnce(&mut Plan)) -> Result<(), String> {
        let before = self.plan()?;
        let mut plan = before.clone();
        edit(&mut plan);
        if plan.innate != before.innate {
            self.innate = Some(plan.innate);
        }
        if plan.specialized != before.specialized || plan.trained != before.trained {
            self.specialized = Some(plan.specialized);
            self.trained = Some(plan.trained);
        }
        if plan.weights != before.weights {
            self.weights = Some(plan.weights);
        }
        Ok(())
    }

    /// Back to its archetype's own choices: its innate split, skills and shares.
    pub fn reset_plan(&mut self) {
        self.innate = None;
        self.specialized = None;
        self.trained = None;
        self.weights = None;
    }

    /// Whether its choices are its archetype's own.
    #[must_use]
    pub fn plan_is_archetypes(&self) -> bool {
        self.innate.is_none()
            && self.specialized.is_none()
            && self.trained.is_none()
            && self.weights.is_none()
    }

    /// Its heritage's id: the heritage it names, else Aluvian.
    ///
    /// # Errors
    /// A heritage these tables do not offer.
    pub fn heritage_id(&self, t: &Tables<'_>) -> Result<u32, String> {
        match &self.heritage {
            None => Ok(dereth_progression::heritage::ALUVIAN),
            Some(name) => t
                .heritage_named(name)
                .ok_or_else(|| format!("The data files have no heritage {name}.")),
        }
    }

    /// Take `own`'s choices: its archetype, heritage, gender, innate split, skills and shares.
    pub fn take(&mut self, own: &OwnArchetype) {
        self.archetype.clone_from(&own.archetype);
        self.heritage.clone_from(&own.heritage);
        self.gender = own.gender;
        self.innate = Some(own.innate);
        self.specialized = Some(own.specialized.clone());
        self.trained = Some(own.trained.clone());
        self.weights = Some(own.weights.clone());
    }

    /// What its player gained beyond the build: augmentations, luminance, titles, quests and its
    /// summoning mastery.
    #[must_use]
    pub fn extras(&self) -> Extras {
        Extras {
            augmentations: self.augmentations.clone(),
            luminance: self.luminance.clone(),
            titles: self.titles.clone(),
            quests: self.quests.clone(),
            quest_credits: self.quest_credits,
            summoning_mastery: self.summoning_mastery,
        }
    }

    /// The level of the buffs it enters with: its own, else by its level (3 up to 14, 5 to 39, 6
    /// to 99, 7 to 179, then 8).
    #[must_use]
    pub fn spell_level(&self) -> u32 {
        self.buff_level.unwrap_or(match self.level {
            0..=14 => 3,
            15..=39 => 5,
            40..=99 => 6,
            100..=179 => 7,
            _ => 8,
        })
    }

    /// The recipe as its file holds it.
    ///
    /// # Errors
    /// Never in practice: every field writes.
    pub fn text(&self) -> Result<String, String> {
        toml::to_string(self).map_err(|e| e.to_string())
    }

    /// What identifies this recipe's character: a hash of the recipe as written, but for how it
    /// plays (where it starts, and its buffs), which never needs the character made again.
    #[must_use]
    pub fn hash(&self) -> String {
        let made = Self {
            buffed: true,
            buff_level: None,
            start: Start::Camera,
            ..self.clone()
        };
        let text = made.text().unwrap_or_default();
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in text.bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
        format!("{h:016x}")
    }

    /// The order the server makes the character from. `place_tier` is the tier the place drops,
    /// which gear for its level is never below; `class_of` turns an item's symbol into its class.
    ///
    /// # Errors
    /// An archetype Lyceum lacks, or an item that names no class.
    pub fn order(
        &self,
        place_tier: Option<u32>,
        class_of: impl Fn(&str) -> Option<u32>,
    ) -> Result<Order, String> {
        let gear = match self.gear {
            GearChoice::None => Gear::None,
            GearChoice::Starter => Gear::Starter,
            GearChoice::Level => Gear::Level,
            GearChoice::Better => Gear::Better,
            GearChoice::Tier => Gear::Tier(self.tier.unwrap_or(1).clamp(1, 8)),
        };
        let class = |w: &str| {
            w.trim()
                .parse::<u32>()
                .ok()
                .or_else(|| class_of(w.trim()))
                .ok_or_else(|| format!("No weenie is {w:?}."))
        };
        let slots = self
            .slots
            .iter()
            .map(|(slot, s)| {
                let class = s.weenie.as_deref().map(class).transpose()?;
                Ok((
                    *slot,
                    SlotOrder {
                        roll: s.roll,
                        class,
                    },
                ))
            })
            .collect::<Result<BTreeMap<_, _>, String>>()?;
        let items = self
            .items
            .iter()
            .map(|i| {
                Ok(Item {
                    class: class(&i.weenie)?,
                    count: i.count,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(Order {
            version: ORDER_VERSION,
            hash: self.hash(),
            level: self.level,
            plan: self.plan()?,
            // The caller names them, from the tables (`Characters::body`).
            heritage: None,
            gender: None,
            gear,
            seed: self.seed,
            items,
            place_tier,
            slots,
            extras: self.extras(),
        })
    }
}

/// The version of an own archetype's file this editor writes.
pub const OWN_ARCHETYPE_VERSION: u32 = 1;

/// A builder's own archetype: what a recipe chose, without its level, gear or items.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnArchetype {
    pub version: u32,
    /// The built-in archetype it was made from.
    pub archetype: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heritage: Option<String>,
    #[serde(default, skip_serializing_if = "is_male")]
    pub gender: Gender,
    pub innate: [u32; 6],
    #[serde(default)]
    pub specialized: Vec<String>,
    #[serde(default)]
    pub trained: Vec<String>,
    #[serde(default)]
    pub weights: BTreeMap<String, u32>,
}

impl OwnArchetype {
    /// What `recipe` chose, its archetype's choices and its own together.
    ///
    /// # Errors
    /// An archetype Lyceum does not have.
    pub fn of(recipe: &Recipe) -> Result<Self, String> {
        let plan = recipe.plan()?;
        Ok(Self {
            version: OWN_ARCHETYPE_VERSION,
            archetype: recipe.archetype.clone(),
            heritage: recipe.heritage.clone(),
            gender: recipe.gender,
            innate: plan.innate,
            specialized: plan.specialized,
            trained: plan.trained,
            weights: plan.weights,
        })
    }
}

/// The own archetypes in `dir`, by name, in name order.
#[must_use]
pub fn own_archetypes(dir: &Path) -> Vec<(String, OwnArchetype)> {
    let mut out: Vec<(String, OwnArchetype)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|e| {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) != Some("toml") {
                return None;
            }
            let name = p.file_stem()?.to_str()?.to_owned();
            let own: OwnArchetype = toml::from_str(&std::fs::read_to_string(&p).ok()?).ok()?;
            (valid_name(&name) && own.version == OWN_ARCHETYPE_VERSION).then_some((name, own))
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// The four tables a build is made from, read from the world's data files once.
#[derive(Debug)]
pub struct OwnedTables {
    pub xp: XpTable,
    pub skills: SkillTable,
    pub chargen: CharGen,
    pub vitals: Attribute2ndTable,
}

impl OwnedTables {
    /// The tables as a build reads them.
    #[must_use]
    pub fn tables(&self) -> Tables<'_> {
        Tables {
            xp: &self.xp,
            skills: &self.skills,
            chargen: &self.chargen,
            vitals: &self.vitals,
        }
    }

    /// The tables in `dats`.
    ///
    /// # Errors
    /// A table the files lack or that does not read.
    pub fn read(dats: &lyceum_foundry::dats::DatSet) -> Result<Self, String> {
        use dereth_primitives::DataId;
        fn one<T: dereth_assets::Decode>(
            dats: &lyceum_foundry::dats::DatSet,
            id: u32,
        ) -> Result<T, String> {
            let bytes = dats
                .read_portal(DataId(id))
                .ok_or_else(|| format!("The data files have no table {id:08X}."))?;
            dats.decode(DataId(id), &bytes).map_err(|e| e.to_string())
        }
        Ok(Self {
            xp: one(dats, 0x0E00_0018)?,
            skills: one(dats, 0x0E00_0004)?,
            chargen: one(dats, 0x0E00_0002)?,
            vitals: one(dats, 0x0E00_0003)?,
        })
    }
}

/// What the creatures round the camera are: their levels' range, and the tier most of them drop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Place {
    pub least: u32,
    pub most: u32,
    /// The median level, which the level is suggested from.
    pub median: u32,
    pub tier: Option<u32>,
}

impl Place {
    /// The place of the creatures `found`: each one's level, and the tier it drops.
    #[must_use]
    pub fn of(mut found: Vec<(u32, Option<u32>)>) -> Option<Self> {
        if found.is_empty() {
            return None;
        }
        found.sort_unstable();
        let mut tiers: std::collections::BTreeMap<u32, usize> = std::collections::BTreeMap::new();
        for t in found.iter().filter_map(|f| f.1) {
            *tiers.entry(t).or_default() += 1;
        }
        let tier = tiers
            .iter()
            .max_by_key(|(t, n)| (**n, std::cmp::Reverse(**t)))
            .map(|(t, _)| *t);
        Some(Self {
            least: found[0].0,
            most: found[found.len() - 1].0,
            median: found[found.len() / 2].0,
            tier,
        })
    }
}

/// The world's recipes, the one selected, and what its character is.
#[derive(Debug, Default)]
pub struct Characters {
    /// The folder `world.toml` is in.
    pub root: PathBuf,
    pub names: Vec<String>,
    /// The selected recipe's name, which is its character's.
    pub name: String,
    pub recipe: Recipe,
    /// What the server last made of it.
    pub made: Option<Made>,
    pub message: String,
    pub error: Option<String>,
    /// The name being typed for a new recipe.
    pub new_name: String,
    /// The character already on the server that Play As asks before replacing.
    pub replace_asked: Option<String>,
    /// The character to replace without asking again.
    pub replace_ok: Option<String>,
    /// What the old templates' upgrade did, each a line for the log.
    pub upgraded: Vec<String>,
    /// Where the builder's own archetypes are kept: the editor's settings folder's `archetypes`.
    pub archetypes_dir: Option<PathBuf>,
    /// The builder's own archetypes, as last read.
    pub own: Vec<(String, OwnArchetype)>,
    /// The name being typed for an own archetype.
    pub own_name: String,
    /// The tab shown, and the part of the Build and Augments tabs shown.
    pub tab: CharacterTab,
    pub build_view: BuildView,
    pub augment_view: AugmentView,
    pub tables: Option<OwnedTables>,
    /// The creatures round the camera, and what they were gathered at: the landblock, the
    /// edits' history, and how many of the server's weenies were known.
    pub place: Option<Place>,
    pub place_at: Option<(u16, (usize, usize), usize)>,
    /// The selected recipe's build, and the recipe and augmentation costs it was worked out from;
    /// what the checks found of it, and the same.
    built: Option<(Recipe, Option<Costs>, Result<Build, String>)>,
    found: Option<(Recipe, Option<Costs>, Checked)>,
    /// What the gear and packs checks found of the selected recipe, and what they were worked
    /// out from: the recipe, and how many of the server's weenies were known.
    pub gear_checked: Option<(Recipe, usize, Vec<Finding>)>,
    /// The tier of loot each level hunts at, from the server's creatures, and how many weenies
    /// it was worked out from.
    pub level_tiers: Option<(usize, [u32; 8])>,
    /// The order the server makes the selected recipe's character from, and what it was worked
    /// out from: the recipe, the place's tier, and how many of the server's weenies were known.
    pub ordered: Option<(OrderKey, Result<Order, String>)>,
    /// What the server said the order would make, before it is made.
    pub preview: Preview,
    /// As drawn last: each tab's button, Reset, the carried items (where weenies are dropped to be
    /// carried) and their Add, and the weenies the Add picker lists.
    pub tab_rects: Vec<(CharacterTab, egui::Rect)>,
    pub reset_button: Option<egui::Rect>,
    pub carried_rect: Option<egui::Rect>,
    pub add_button: Option<egui::Rect>,
    pub picker_rows: Vec<(lyceum_foundry::edit::Weenie, egui::Rect)>,
    /// What is typed in the Add picker's search.
    pub picker_search: String,
    /// What the SQL says of each weenie the recipe names, as last read.
    pub facts: std::collections::BTreeMap<String, Option<lyceum_foundry::sql_ids::WeenieFacts>>,
    /// The augmentations' costs, the titles and the quests the tab offers, as last worked out.
    pub extras: crate::character_extras::Offered,
}

/// What the checks found of a build, or why there is none.
type Checked = Result<Vec<Finding>, String>;

/// What an order is worked out from: the recipe, the place's tier, how many of the server's
/// weenies are known, and the world's edits' history.
pub type OrderKey = (Recipe, Option<u32>, usize, (usize, usize));

/// The Characters tab's tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CharacterTab {
    /// The attributes, vitals and skills, the credits and the experience.
    #[default]
    Build,
    /// The innate split and the weights the ranks are spent by.
    Tweak,
    /// The gear and what it carries.
    Items,
    /// The augmentations and luminance.
    Augments,
    /// The titles and quests.
    Quests,
}

impl CharacterTab {
    /// Every tab, in the order shown.
    pub const ALL: [Self; 5] = [
        Self::Build,
        Self::Tweak,
        Self::Items,
        Self::Augments,
        Self::Quests,
    ];

    /// Its words' key in the text.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Build => "characters.tab_build",
            Self::Tweak => "characters.tab_tweak",
            Self::Items => "characters.tab_items",
            Self::Augments => "characters.tab_augments",
            Self::Quests => "characters.tab_quests",
        }
    }
}

/// The Build tab's two parts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BuildView {
    /// The attributes and vitals.
    #[default]
    Attributes,
    Skills,
}

/// The Augments tab's two parts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AugmentView {
    #[default]
    Augmentations,
    Luminance,
}

/// One skill's row of the Build tab, worked out when the build changes.
#[derive(Debug, Clone)]
pub struct SkillRow {
    pub id: u32,
    pub name: String,
    /// What the build has it as, and what the recipe wants it as.
    pub sac: dereth_progression::Sac,
    pub wants: dereth_progression::Sac,
    /// Whether it can be untrained, trained and specialized.
    pub can: [bool; 3],
    /// Its credits to train and to specialize, as the row says them.
    pub cost: String,
    /// Its value, when it is trained.
    pub value: Option<u32>,
}

/// What the editor's own server said an order would make, before anything is made of it.
#[derive(Debug, Default)]
pub struct Preview {
    /// The order asked about last, and the server's revision of the world then.
    pub asked: Option<(Order, u64)>,
    /// Where the server's log stood when it was asked, while its answer is awaited.
    pub mark: Option<usize>,
    /// Its answer: the order, and what it would make or why it would not.
    pub said: Option<(Order, Result<Made, String>)>,
}

/// What the Items tab shows of the selected recipe's character.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shown<'a> {
    /// What the server made of it, which is still the recipe's.
    Made(&'a Made),
    /// What the server says the order would make.
    Preview(&'a Made),
    /// The server is asked; what it said of the order before, if anything.
    Asking(Option<&'a Made>),
    /// Why the server would not make it.
    Refused(&'a str),
    /// No server to ask.
    NoServer,
}

impl Characters {
    /// What the Items tab shows: the character the server made, while it is still the recipe's;
    /// else what the server says `order` (the order Play As and Reset send now) would make.
    #[must_use]
    pub fn shown(&self, order: Option<&Order>, server: bool) -> Shown<'_> {
        if let Some(m) = self.made.as_ref().filter(|m| m.hash == self.recipe.hash()) {
            return Shown::Made(m);
        }
        if !server {
            return Shown::NoServer;
        }
        match (&self.preview.said, order) {
            (Some((o, said)), Some(now)) if o == now && self.preview.mark.is_none() => match said {
                Ok(m) => Shown::Preview(m),
                Err(e) => Shown::Refused(e),
            },
            (said, _) => Shown::Asking(said.as_ref().and_then(|(_, s)| s.as_ref().ok())),
        }
    }

    /// Where the order a preview asks about is written, and where the server says what it would
    /// make.
    #[must_use]
    pub fn preview_paths(&self) -> (PathBuf, PathBuf) {
        let dir = self.playtest().join("previews");
        (
            dir.join(format!("{}.json", self.name)),
            dir.join(format!("{}-made.json", self.name)),
        )
    }
}

/// Character names are also file names and console arguments.
#[must_use]
pub fn valid_name(name: &str) -> bool {
    (3..=24).contains(&name.len())
        && name.trim() == name
        && name.chars().all(|c| c.is_ascii_alphabetic() || c == ' ')
        && !["CON", "PRN", "AUX", "NUL"].contains(&name.to_ascii_uppercase().as_str())
}

/// A character template as the first editors wrote them (`characters/<Name>.json`): typed
/// attribute values, skills with purchased ranks, and equipment by class.
#[derive(Debug, Clone, Deserialize)]
pub struct Template {
    pub version: u32,
    pub level: u32,
    #[serde(default)]
    pub skills: Vec<TemplateSkill>,
    #[serde(default)]
    pub equipment: Vec<TemplateItem>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TemplateSkill {
    pub name: String,
    pub training: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TemplateItem {
    pub weenie: u32,
}

/// The recipe a template becomes: its level, its trained and specialized skills in its order,
/// its equipment as items (a class `symbol_of` names a pack weenie of becomes its symbol), and
/// the archetype whose main skill it trains (else the first) for the innate split and the shares
/// of experience. Its typed attribute values and ranks are dropped.
#[must_use]
pub fn upgrade(t: &Template, symbol_of: impl Fn(u32) -> Option<String>) -> Recipe {
    let skills = |training: &str| -> Vec<String> {
        t.skills
            .iter()
            .filter(|s| s.training == training)
            .map(|s| s.name.clone())
            .collect()
    };
    let specialized = skills("Specialized");
    let trained = skills("Trained");
    let chosen: Vec<String> = specialized
        .iter()
        .chain(&trained)
        .map(|s| dereth_progression::normalized(s))
        .collect();
    let archetype = dereth_progression::archetypes::builtin()
        .into_iter()
        .find(|a| {
            a.plan.specialized.first().is_some_and(|main| {
                main.split('|')
                    .any(|n| chosen.contains(&dereth_progression::normalized(n)))
            })
        })
        .map_or_else(|| "melee-heavy".to_owned(), |a| a.id);
    Recipe {
        level: t.level.max(1),
        archetype,
        specialized: (!specialized.is_empty()).then_some(specialized),
        trained: (!trained.is_empty()).then_some(trained),
        items: t
            .equipment
            .iter()
            .map(|e| RecipeItem {
                weenie: symbol_of(e.weenie).unwrap_or_else(|| e.weenie.to_string()),
                count: 1,
            })
            .collect(),
        ..Recipe::default()
    }
}

impl Characters {
    /// The world `world`'s recipes, its old templates upgraded first, and the one selected last.
    /// `symbol_of` names the pack weenie a class number is, for the upgrade.
    pub fn open(world: &Path, symbol_of: impl Fn(u32) -> Option<String>) -> Self {
        let root = world.parent().unwrap_or(Path::new(".")).to_path_buf();
        let mut out = Self {
            root,
            name: "Builder".into(),
            ..Default::default()
        };
        out.upgrade_templates(&symbol_of);
        out.refresh();
        if let Ok(name) = std::fs::read_to_string(out.playtest().join("selected-character")) {
            if valid_name(&name) {
                out.name = name;
            }
        }
        if let Err(e) = out.load() {
            out.message = e;
        }
        out
    }

    fn folder(&self) -> PathBuf {
        self.root.join("characters")
    }

    fn playtest(&self) -> PathBuf {
        self.root.join(".playtest")
    }

    /// Each old template made a recipe, once: the template kept beside it as
    /// `<Name>.json.before-upgrade`.
    fn upgrade_templates(&mut self, symbol_of: &impl Fn(u32) -> Option<String>) {
        let Ok(dir) = std::fs::read_dir(self.folder()) else {
            return;
        };
        for e in dir.filter_map(Result::ok) {
            let p = e.path();
            let Some(name) = p
                .file_stem()
                .and_then(|n| n.to_str())
                .filter(|n| valid_name(n))
                .map(str::to_owned)
            else {
                continue;
            };
            if p.extension().and_then(|x| x.to_str()) != Some("json") {
                continue;
            }
            let toml_path = p.with_extension("toml");
            let result = (|| -> Result<(), String> {
                let t: Template =
                    serde_json::from_slice(&std::fs::read(&p).map_err(|e| e.to_string())?)
                        .map_err(|e| e.to_string())?;
                if !toml_path.exists() {
                    let r = upgrade(&t, symbol_of);
                    std::fs::write(&toml_path, r.text()?).map_err(|e| e.to_string())?;
                }
                let mut kept = p.clone().into_os_string();
                kept.push(".before-upgrade");
                std::fs::rename(&p, kept).map_err(|e| e.to_string())
            })();
            self.upgraded.push(match result {
                Ok(()) => format!(
                    "Upgraded the character template {name} to a recipe: its attribute values and skill ranks are dropped, and skills its level's credits do not cover stay untrained."
                ),
                Err(e) => format!("The character template {name} was not upgraded: {e}"),
            });
        }
    }

    /// The recipes in the world folder, again.
    pub fn refresh(&mut self) {
        self.names = std::fs::read_dir(self.folder())
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .filter_map(|e| {
                let p = e.path();
                (p.extension().and_then(|x| x.to_str()) == Some("toml"))
                    .then(|| p.file_stem()?.to_str().map(str::to_owned))
                    .flatten()
            })
            .filter(|n| valid_name(n))
            .collect();
        self.names.sort();
    }

    /// The selected recipe's file.
    ///
    /// # Errors
    /// A name that is not one.
    pub fn path(&self) -> Result<PathBuf, String> {
        self.path_of(&self.name)
    }

    fn path_of(&self, name: &str) -> Result<PathBuf, String> {
        if !valid_name(name) {
            return Err("Use 3 to 24 letters and spaces for the character name.".into());
        }
        Ok(self.folder().join(format!("{name}.toml")))
    }

    /// The order the server makes the selected character from.
    #[must_use]
    pub fn order_path(&self) -> PathBuf {
        self.playtest()
            .join("recipes")
            .join(format!("{}.json", self.name))
    }

    /// What the server says it made of the selected recipe.
    #[must_use]
    pub fn made_path(&self) -> PathBuf {
        self.playtest()
            .join("characters")
            .join(format!("{}.json", self.name))
    }

    /// The file the selected recipe's character is exported to, in the world's output folder
    /// `out`.
    #[must_use]
    pub fn export_path(&self, out: &Path) -> PathBuf {
        out.join("export")
            .join("characters")
            .join(format!("{}.empyrean-character", self.name))
    }

    /// What the editor tells the server of the selected recipe's character it exports.
    #[must_use]
    pub fn export_note_path(&self) -> PathBuf {
        self.playtest()
            .join("exports")
            .join(format!("{}.json", self.name))
    }

    /// What the editor tells the server of the character it exports: the recipe it was made
    /// from, while it is still that recipe's, and the symbol of each of the world's own weenies,
    /// by class (`symbols`).
    #[must_use]
    pub fn export_note(&self, symbols: BTreeMap<u32, String>) -> ExportNote {
        let current = self
            .made
            .as_ref()
            .is_some_and(|m| m.hash == self.recipe.hash());
        ExportNote {
            version: EXPORT_VERSION,
            recipe: if current {
                self.recipe.text().unwrap_or_default()
            } else {
                String::new()
            },
            hash: self
                .made
                .as_ref()
                .map(|m| m.hash.clone())
                .unwrap_or_default(),
            symbols,
        }
    }

    /// Read the selected recipe (a new one when it has no file), and what its character is.
    ///
    /// # Errors
    /// A recipe that does not read.
    pub fn load(&mut self) -> Result<(), String> {
        let path = self.path()?;
        self.recipe = if path.is_file() {
            toml::from_str(&std::fs::read_to_string(path).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?
        } else {
            Recipe::default()
        };
        self.read_made();
        if self.recipe.version != RECIPE_VERSION {
            return Err("Unsupported character recipe version.".into());
        }
        Ok(())
    }

    /// What the server last said it made of the selected recipe.
    pub fn read_made(&mut self) {
        self.made = std::fs::read(self.made_path())
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok());
    }

    /// Select recipe `name`.
    ///
    /// # Errors
    /// A name that is not one, or a recipe that does not read.
    pub fn select(&mut self, name: &str) -> Result<(), String> {
        if !valid_name(name) {
            return Err("Use 3 to 24 letters and spaces for the character name.".into());
        }
        name.clone_into(&mut self.name);
        self.load()?;
        self.save()
    }

    /// Write the selected recipe, and remember it as selected: each file only when it changes,
    /// and the recipes listed again only when one is new.
    ///
    /// # Errors
    /// A file that will not write.
    pub fn save(&mut self) -> Result<(), String> {
        let path = self.path()?;
        std::fs::create_dir_all(self.folder()).map_err(|e| e.to_string())?;
        let text = self.recipe.text()?;
        if std::fs::read_to_string(&path).ok().as_deref() != Some(text.as_str()) {
            std::fs::write(&path, text).map_err(|e| e.to_string())?;
        }
        let selected = self.playtest().join("selected-character");
        if std::fs::read_to_string(&selected).ok().as_deref() != Some(self.name.as_str()) {
            std::fs::create_dir_all(self.playtest()).map_err(|e| e.to_string())?;
            std::fs::write(selected, &self.name).map_err(|e| e.to_string())?;
        }
        if !self.names.contains(&self.name) {
            self.refresh();
        }
        Ok(())
    }

    /// Remove recipe `name`. Its character stays on the server.
    ///
    /// # Errors
    /// A file that will not go.
    pub fn delete(&mut self, name: &str) -> Result<(), String> {
        let path = self.path_of(name)?;
        if path.is_file() {
            std::fs::remove_file(path).map_err(|e| e.to_string())?;
        }
        self.refresh();
        if self.name == name {
            self.name = self
                .names
                .first()
                .cloned()
                .unwrap_or_else(|| "Builder".into());
            self.load()?;
        }
        Ok(())
    }

    /// Whether the character is no longer its recipe's: never made, or made from another recipe.
    #[must_use]
    pub fn needs_making(&self) -> bool {
        self.made
            .as_ref()
            .is_none_or(|m| m.hash != self.recipe.hash())
    }

    /// Left by the first editors when they applied a template: such a character is Lyceum's.
    #[must_use]
    pub fn applied(&self) -> PathBuf {
        self.playtest().join(format!("{}.applied", self.name))
    }

    /// Whether the server may replace the selected character without asking: Lyceum made it, or
    /// the builder said to.
    #[must_use]
    pub fn may_replace(&self) -> bool {
        self.applied().exists() || self.replace_ok.as_deref() == Some(self.name.as_str())
    }

    /// The selected recipe's heritage and gender, by the creation table's ids.
    ///
    /// # Errors
    /// No tables yet, or a heritage they do not offer.
    pub fn body(&self) -> Result<(u32, u32), String> {
        let t = self
            .tables
            .as_ref()
            .ok_or("The data files' tables are not read.")?;
        Ok((
            self.recipe.heritage_id(&t.tables())?,
            self.recipe.gender.id(),
        ))
    }

    /// Read the builder's own archetypes again.
    pub fn read_own(&mut self) {
        self.own = self
            .archetypes_dir
            .as_deref()
            .map(own_archetypes)
            .unwrap_or_default();
    }

    /// Keep the selected recipe's choices as the builder's own archetype `name`, in the settings
    /// folder; one of that name is replaced.
    ///
    /// # Errors
    /// No settings folder, a name that is not one, or a file that will not write.
    pub fn save_own(&mut self, name: &str) -> Result<PathBuf, String> {
        if !valid_name(name) {
            return Err("Use 3 to 24 letters and spaces for the archetype's name.".into());
        }
        let dir = self
            .archetypes_dir
            .clone()
            .ok_or("The editor has no settings folder to keep archetypes in.")?;
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let path = dir.join(format!("{name}.toml"));
        let text = toml::to_string(&OwnArchetype::of(&self.recipe)?).map_err(|e| e.to_string())?;
        std::fs::write(&path, text).map_err(|e| e.to_string())?;
        self.read_own();
        Ok(path)
    }

    /// The selected recipe's build, worked out from the tables when it changed.
    pub fn build(&mut self) -> Option<&Result<Build, String>> {
        let tables = self.tables.as_ref()?;
        let costs = self.extras.costs.clone();
        if self
            .built
            .as_ref()
            .is_none_or(|(r, c, _)| *r != self.recipe || *c != costs)
        {
            let t = tables.tables();
            let extras = self.recipe.extras();
            let made = self.recipe.plan().and_then(|plan| {
                let heritage = self.recipe.heritage_id(&t)?;
                // Before the world's gems are read only the augmentations of fixed cost are priced.
                let known = costs.clone().unwrap_or_else(|| Costs::of(|_, _| None));
                if costs.is_none()
                    && extras
                        .augmentations
                        .iter()
                        .any(|(id, n)| *n > 0 && known.get(id).is_none())
                {
                    return Err(
                        "The augmentations' costs are not known until the world database is read."
                            .into(),
                    );
                }
                dereth_progression::extras::build(
                    &t,
                    self.recipe.level,
                    heritage,
                    &plan,
                    &extras,
                    &known,
                )
            });
            let b = match made {
                Ok((b, found)) => {
                    self.extras.found = found;
                    Ok(b)
                }
                Err(e) => {
                    self.extras.found.clear();
                    Err(e)
                }
            };
            self.built = Some((self.recipe.clone(), costs, b));
        }
        self.built.as_ref().map(|(_, _, b)| b)
    }

    /// What the checks find of the selected recipe's build, worked out again when it changed.
    ///
    /// # Errors
    /// Why there is no build.
    pub fn findings(&mut self) -> Result<Vec<Finding>, String> {
        if self.tables.is_none() {
            return Ok(Vec::new());
        }
        let costs = self.extras.costs.clone();
        if let Some((r, c, found)) = &self.found {
            if *r == self.recipe && *c == costs {
                return found.clone();
            }
        }
        let found = self.work_out_findings();
        self.found = Some((self.recipe.clone(), costs, found.clone()));
        found
    }

    fn work_out_findings(&mut self) -> Result<Vec<Finding>, String> {
        let Some(b) = self.build().cloned() else {
            return Ok(Vec::new());
        };
        let b = b?;
        let Some(t) = self.tables.as_ref() else {
            return Ok(Vec::new());
        };
        let t = t.tables();
        let mut found = check(&t, &b.sheet());
        // The chosen skills the level's credits do not cover stay untrained: said, not refused.
        let plan = self.recipe.plan()?;
        let untrained: Vec<String> = plan
            .specialized
            .iter()
            .chain(&plan.trained)
            .filter_map(|token| t.resolve_skill(token))
            .filter(|id| {
                b.skills
                    .iter()
                    .any(|s| s.id == *id && s.sac < dereth_progression::Sac::Trained)
            })
            .map(|id| t.skill_name(id))
            .collect();
        if !untrained.is_empty() {
            found.push(Finding {
                field: dereth_progression::Field::Credits,
                text: format!(
                    "Level {} has too few credits for {}.",
                    self.recipe.level,
                    untrained.join(", ")
                ),
                warning: true,
            });
        }
        found.extend(self.extras.found.iter().cloned());
        Ok(found)
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: character recipes, their upgrade and safe names).
    use super::*;

    fn scratch() -> PathBuf {
        let mut serial = 0;
        loop {
            let p = std::env::temp_dir().join(format!(
                "lyceum-character-recipe-{}-{serial}",
                std::process::id()
            ));
            match std::fs::create_dir(&p) {
                Ok(()) => return p,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => serial += 1,
                Err(e) => panic!("{e}"),
            }
        }
    }

    #[test]
    fn character_names_cannot_escape_the_world_or_inject_console_commands() {
        assert!(valid_name("Test Mage"));
        for name in ["../escape", "A\nexit", "A\"B", "CON", " Test ", "ab"] {
            assert!(!valid_name(name), "{name:?}");
        }
    }

    #[test]
    fn a_recipe_round_trips_as_toml_and_names_no_rank_or_attribute_value() {
        let r = Recipe {
            level: 80,
            items: vec![RecipeItem {
                weenie: "mypack:coolsword".into(),
                count: 1,
            }],
            ..Recipe::default()
        };
        let text = r.text().unwrap();
        assert_eq!(toml::from_str::<Recipe>(&text).unwrap(), r);
        for word in ["ranks", "attributes", "account"] {
            assert!(!text.contains(word), "{text}");
        }
        assert!(
            toml::from_str::<Recipe>("version = 2\nlevel = 1\narchetype = \"x\"\nranks = 3\n")
                .is_err()
        );
    }

    #[test]
    fn each_heritage_shows_the_name_players_read_and_a_recipe_keeps_the_tables() {
        // The end of retail's creation table, by id.
        let table = [
            "Aluvian",
            "Gharu'ndim",
            "Sho",
            "Viamontian",
            "Umbraen",
            "Gear",
            "Tumerok",
            "Lugian",
            "Empyrean",
            "Penumbraen",
            "Undead",
        ];
        let shown: Vec<String> = (1..)
            .zip(table)
            .map(|(id, n)| heritage_title(id, n))
            .collect();
        assert_eq!(
            shown,
            [
                "Aluvian",
                "Gharu'ndim",
                "Sho",
                "Viamontian",
                "Umbraen",
                "Gear Knight",
                "Tumerok",
                "Lugian",
                "Empyrean",
                "Penumbraen",
                "Undead"
            ]
        );
    }

    #[test]
    fn a_recipe_with_its_own_choices_round_trips_and_its_plan_is_them() {
        let mut r = Recipe {
            level: 80,
            heritage: Some("Sho".into()),
            gender: Gender::Female,
            ..Recipe::default()
        };
        assert!(r.plan_is_archetypes());
        r.edit_plan(|p| {
            p.innate = [100, 10, 100, 100, 10, 10];
            p.specialized.retain(|s| s == "MeleeDefense");
            p.trained.clear();
            p.weights.insert("Strength".into(), 90);
        })
        .unwrap();
        assert!(!r.plan_is_archetypes());
        let text = r.text().unwrap();
        let back: Recipe = toml::from_str(&text).unwrap();
        assert_eq!(back, r, "{text}");
        let plan = back.plan().unwrap();
        assert_eq!(plan.innate, [100, 10, 100, 100, 10, 10]);
        assert_eq!(plan.specialized, ["MeleeDefense"]);
        assert!(plan.trained.is_empty(), "an emptied list is kept empty");
        assert_eq!(plan.weights.get("Strength"), Some(&90));
        assert_ne!(
            back.hash(),
            Recipe {
                level: 80,
                ..Recipe::default()
            }
            .hash()
        );
        r.reset_plan();
        assert_eq!(
            r.plan().unwrap(),
            Archetype::named("melee-heavy").unwrap().plan
        );
        // A recipe of the archetype's own choices and the default body writes none of them.
        let plain = Recipe::default().text().unwrap();
        for word in ["innate", "weights", "heritage", "gender", "specialized"] {
            assert!(!plain.contains(word), "{plain}");
        }
    }

    #[test]
    fn a_recipes_choices_kept_as_an_own_archetype_are_taken_whole_by_another_recipe() {
        let dir = scratch();
        let world = dir.join("world.toml");
        let mut c = Characters::open(&world, |_| None);
        assert!(c.save_own("My Mage").is_err(), "no settings folder");
        c.archetypes_dir = Some(dir.join("settings").join("archetypes"));
        c.recipe = Recipe {
            level: 120,
            archetype: "war-mage".into(),
            heritage: Some("Gharu'ndim".into()),
            ..Recipe::default()
        };
        c.recipe
            .edit_plan(|p| {
                p.innate = [10, 90, 10, 20, 100, 100];
            })
            .unwrap();
        assert!(c.save_own("no").is_err(), "not a name");
        let path = c.save_own("My Mage").unwrap();
        assert!(path.is_file());
        assert_eq!(c.own.len(), 1);
        let (name, own) = c.own[0].clone();
        assert_eq!(name, "My Mage");
        assert!(!std::fs::read_to_string(path).unwrap().contains("level"));
        let mut other = Recipe {
            level: 30,
            ..Recipe::default()
        };
        other.take(&own);
        assert_eq!(other.level, 30, "its own level");
        assert_eq!(other.archetype, "war-mage");
        assert_eq!(other.heritage.as_deref(), Some("Gharu'ndim"));
        assert_eq!(other.plan().unwrap(), c.recipe.plan().unwrap());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_changed_recipe_has_another_hash_and_its_order_carries_it_with_the_places_tier() {
        let r = Recipe {
            level: 80,
            ..Recipe::default()
        };
        let o = r.order(Some(5), |_| None).unwrap();
        assert_eq!(
            (o.level, o.gear, o.place_tier, o.hash.clone()),
            (80, Gear::Level, Some(5), r.hash()),
            "for its level, never below the place's tier"
        );
        assert_eq!(r.order(None, |_| None).unwrap().place_tier, None);
        let mut other = r.clone();
        other.level = 81;
        assert_ne!(other.hash(), r.hash());
        // How it plays is not what it is: the character is not made again for it.
        let plays = Recipe {
            buffed: false,
            buff_level: Some(4),
            start: Start::Left,
            ..r.clone()
        };
        assert_eq!(plays.hash(), r.hash());
        // A summoning mastery is part of what it is; none is written as nothing.
        assert!(!r.text().unwrap().contains("summoning"));
        let summoner = Recipe {
            summoning_mastery: Some(Mastery::Naturalist),
            ..r.clone()
        };
        assert_ne!(summoner.hash(), r.hash());
        assert!(summoner
            .text()
            .unwrap()
            .contains("summoning_mastery = \"naturalist\""));
        assert_eq!(
            summoner
                .order(None, |_| None)
                .unwrap()
                .extras
                .summoning_mastery,
            Some(Mastery::Naturalist)
        );
        let rolled = Recipe {
            slots: BTreeMap::from([(
                Slot::Head,
                RecipeSlot {
                    roll: 1,
                    weenie: None,
                },
            )]),
            ..r.clone()
        };
        assert_ne!(rolled.hash(), r.hash(), "a slot rolled again");
        let chosen = Recipe {
            gear: GearChoice::Better,
            slots: BTreeMap::from([
                (
                    Slot::Weapon,
                    RecipeSlot {
                        roll: 0,
                        weenie: Some("pack:sword".into()),
                    },
                ),
                (
                    Slot::Head,
                    RecipeSlot {
                        roll: 2,
                        weenie: None,
                    },
                ),
            ]),
            ..r.clone()
        };
        let o = chosen
            .order(None, |s| (s == "pack:sword").then_some(0x8000_0002))
            .unwrap();
        assert_eq!(o.gear, Gear::Better);
        assert_eq!(
            o.slots[&Slot::Weapon],
            SlotOrder {
                roll: 0,
                class: Some(0x8000_0002)
            }
        );
        assert_eq!(
            o.slots[&Slot::Head],
            SlotOrder {
                roll: 2,
                class: None
            }
        );
        let text = chosen.text().unwrap();
        assert!(text.contains("[slots.weapon]"), "{text}");
        assert_eq!(toml::from_str::<Recipe>(&text).unwrap(), chosen);
        let items = Recipe {
            items: vec![
                RecipeItem {
                    weenie: "pack:food".into(),
                    count: 20,
                },
                RecipeItem {
                    weenie: "273".into(),
                    count: 1,
                },
            ],
            ..Recipe::default()
        };
        let o = items
            .order(None, |s| (s == "pack:food").then_some(0x8000_0001))
            .unwrap();
        assert_eq!(
            o.items,
            [
                Item {
                    class: 0x8000_0001,
                    count: 20
                },
                Item {
                    class: 273,
                    count: 1
                }
            ]
        );
        let missing = Recipe {
            items: vec![RecipeItem {
                weenie: "pack:nothing".into(),
                count: 1,
            }],
            ..Recipe::default()
        };
        assert!(missing.order(None, |_| None).is_err());
    }

    #[test]
    fn buffs_go_by_level_unless_the_recipe_names_one() {
        let at = |level| Recipe {
            level,
            ..Recipe::default()
        };
        assert_eq!(
            [1, 14, 15, 40, 99, 100, 180, 275].map(|l| at(l).spell_level()),
            [3, 3, 5, 6, 6, 7, 8, 8]
        );
        let named = Recipe {
            buff_level: Some(4),
            ..at(200)
        };
        assert_eq!(named.spell_level(), 4);
    }

    #[test]
    fn an_old_template_becomes_a_recipe_once_keeping_its_level_skills_and_equipment() {
        let dir = scratch();
        std::fs::create_dir_all(dir.join("characters")).unwrap();
        std::fs::write(
            dir.join("characters/Test Mage.json"),
            r#"{"version":1,"level":100,"attributes":[10,60,10,50,100,100],"skills":[{"name":"WarMagic","training":"Specialized","ranks":200},{"name":"LifeMagic","training":"Trained","ranks":50}],"equipment":[{"weenie":2000,"location":0},{"weenie":273,"location":0}]}"#,
        )
        .unwrap();
        let world = dir.join("world.toml");
        let c = Characters::open(&world, |n| (n == 2000).then(|| "mine:staff".to_owned()));
        assert_eq!(c.names, ["Test Mage"]);
        assert_eq!(c.upgraded.len(), 1, "{:?}", c.upgraded);
        assert!(dir
            .join("characters/Test Mage.json.before-upgrade")
            .is_file());
        assert!(!dir.join("characters/Test Mage.json").exists());
        let text = std::fs::read_to_string(dir.join("characters/Test Mage.toml")).unwrap();
        let r: Recipe = toml::from_str(&text).unwrap();
        assert_eq!((r.level, r.archetype.as_str()), (100, "war-mage"));
        assert_eq!(r.specialized.as_deref(), Some(&["WarMagic".to_owned()][..]));
        assert_eq!(r.trained.as_deref(), Some(&["LifeMagic".to_owned()][..]));
        assert_eq!(
            r.items
                .iter()
                .map(|i| i.weenie.as_str())
                .collect::<Vec<_>>(),
            ["mine:staff", "273"]
        );
        // Opened again, nothing is upgraded twice.
        let again = Characters::open(&world, |_| None);
        assert!(again.upgraded.is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn reopening_a_world_recovers_its_selected_recipe_and_what_the_server_made_of_it() {
        let dir = scratch();
        let world = dir.join("world.toml");
        let mut c = Characters::open(&world, |_| None);
        c.name = "Test Mage".into();
        c.recipe.level = 100;
        c.save().unwrap();
        assert!(c.needs_making(), "never made");
        let made = Made {
            version: 1,
            hash: c.recipe.hash(),
            name: "Test Mage".into(),
            id: 0x5000_0001,
            level: 100,
            tier: 6,
            items: Vec::new(),
            burden: 0,
            capacity: 0,
        };
        std::fs::create_dir_all(c.made_path().parent().unwrap()).unwrap();
        std::fs::write(c.made_path(), serde_json::to_vec(&made).unwrap()).unwrap();
        let mut reopened = Characters::open(&world, |_| None);
        assert_eq!(reopened.name, "Test Mage");
        assert_eq!(reopened.recipe.level, 100);
        assert_eq!(reopened.made.as_ref(), Some(&made));
        assert!(!reopened.needs_making(), "the character is its recipe's");
        reopened.recipe.level = 101;
        assert!(reopened.needs_making(), "the recipe changed");
        reopened.delete("Test Mage").unwrap();
        assert!(reopened.names.is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn an_export_names_the_recipe_only_while_the_character_is_still_what_it_made() {
        let dir = scratch();
        let mut c = Characters::open(&dir.join("world.toml"), |_| None);
        c.recipe.level = 60;
        let symbols = BTreeMap::from([(100_001, "mine:sword".to_owned())]);
        assert_eq!(c.export_note(symbols.clone()).hash, "", "never made");
        c.made = Some(Made {
            version: 1,
            hash: c.recipe.hash(),
            name: "Builder".into(),
            id: 0x5000_0001,
            level: 60,
            tier: 4,
            items: Vec::new(),
            burden: 0,
            capacity: 0,
        });
        let note = c.export_note(symbols.clone());
        assert_eq!(note.hash, c.recipe.hash());
        assert_eq!(note.recipe, c.recipe.text().unwrap());
        assert_eq!(note.symbols, symbols);
        let made = c.recipe.hash();
        c.recipe.level = 61;
        let note = c.export_note(symbols.clone());
        assert_eq!(
            (note.hash.as_str(), note.recipe.as_str()),
            (made.as_str(), "")
        );
        assert_eq!(
            c.export_path(Path::new("out")),
            Path::new("out/export/characters/Builder.empyrean-character")
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn the_items_shown_are_what_was_made_while_it_is_the_recipes_else_what_the_order_would_make() {
        use dereth_progression::order::MadeItem;
        let dir = scratch();
        let mut c = Characters::open(&dir.join("world.toml"), |_| None);
        c.recipe.level = 80;
        let made_of = |hash: &str, sword: &str| Made {
            version: 1,
            hash: hash.to_owned(),
            name: String::new(),
            id: 0,
            level: 80,
            tier: 5,
            items: vec![MadeItem {
                name: sword.to_owned(),
                class: 1,
                slot: 0x0100_0000,
                wield: String::new(),
                count: 1,
                gear: Some(Slot::Weapon),
            }],
            burden: 0,
            capacity: 0,
        };
        let order = c.recipe.order(None, |_| None).unwrap();
        // No server: nothing to ask.
        assert_eq!(c.shown(Some(&order), false), Shown::NoServer);
        // Asked, then answered: what the order would make.
        c.preview.asked = Some((order.clone(), 1));
        c.preview.mark = Some(10);
        assert_eq!(c.shown(Some(&order), true), Shown::Asking(None));
        let shown = made_of(&c.recipe.hash(), "Shown Sword");
        c.preview.mark = None;
        c.preview.said = Some((order.clone(), Ok(shown.clone())));
        assert_eq!(c.shown(Some(&order), true), Shown::Preview(&shown));
        // Made from it: what was made, while the recipe is as it was.
        let made = made_of(&c.recipe.hash(), "Made Sword");
        c.made = Some(made.clone());
        assert_eq!(c.shown(Some(&order), true), Shown::Made(&made));
        assert_eq!(c.shown(Some(&order), false), Shown::Made(&made));
        // A slot rolled again: the recipe changed, and what the old order would make is not
        // shown as the new one's.
        c.recipe.slots.insert(
            Slot::Weapon,
            RecipeSlot {
                roll: 1,
                weenie: None,
            },
        );
        let rolled = c.recipe.order(None, |_| None).unwrap();
        assert_eq!(c.shown(Some(&rolled), true), Shown::Asking(Some(&shown)));
        c.preview.said = Some((rolled.clone(), Err("too heavy".into())));
        assert_eq!(c.shown(Some(&rolled), true), Shown::Refused("too heavy"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn the_place_suggests_the_median_level_and_the_tier_most_creatures_drop() {
        let p = Place::of(vec![
            (76, Some(5)),
            (84, Some(5)),
            (80, Some(4)),
            (78, None),
        ])
        .unwrap();
        assert_eq!((p.least, p.most, p.median, p.tier), (76, 84, 80, Some(5)));
        assert_eq!(Place::of(Vec::new()), None);
    }
}
