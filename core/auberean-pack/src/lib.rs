//! The Asheron's Call 2 dialect of the shared pack format: its game and era, the items an
//! override or a removal names, and what its symbols number.
//!
//! **Depends on** the shared pack format (`dereth-pack`), whose grammar it fills: worlds and
//! their packs, overrides, removals, provenance and the lockfile. **Used by** the world database
//! (`yalain-world-db`), which reads the database's packs and compiles them for the server, and by
//! the world builder when it builds worlds of this game.
//!
//! A world of this game says `game = "ac2"` in its world file and in every pack's header, and is
//! built on one era of the game's data files: today only the end of retail (`era = "eor"`), the
//! world as the final retail client's files and the January 2017 servers left it. Its packs hold
//! server rows (where people and objects stand, camps, loot, stores, quests, places, what people
//! say), each of which sets one **item** ([`Item`]), named in one form the build's report, an
//! override's `items` and a `[[remove]]` all use:
//!
//! | item | names |
//! |---|---|
//! | `object KEY@XXYY`, `camp KEY@XXYY` | a placed row by its key, in its landblock |
//! | `place KEY` | a destination |
//! | `loot 0xVALUE` | a death-loot profile |
//! | `keeper 0xTEMPLATE` | a storekeeper |
//! | `store KEY` | a shop of a world's own: a store template's copy under its own title |
//! | `sale 0xSTORE ORDINAL` | a store's listing |
//! | `quest 0xID` | a quest's server values |
//! | `step 0xQUEST KEY` | a step of a quest |
//! | `talk KEY` | a line a person says |
//!
//! `landblock XXYY`, everything of a landblock, is the grammar's own. Numbers are read in
//! hexadecimal after `0x`, else in decimal, and written back one way, so `keeper 0x470033D0` and
//! `keeper 0x470033d0` are one item.
//!
//! **Must never** know the rows themselves: their fields and what they compile to are the world
//! database's.

use std::fmt;

use dereth_pack::{Dialect, Numbering, SymbolKind};

/// The game's key, as its world and pack files write it.
pub const GAME: &str = "ac2";

/// The eras a world of the game may be built on, by name.
pub const ERAS: [&str; 1] = ["eor"];

/// The game's words for the shared pack format.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Ac2;

/// An era of the game's data files, which a world is built on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Era {
    /// The end of retail: the final retail client's files and the world the January 2017 servers
    /// ran.
    Eor,
}

impl Era {
    /// How a world file names it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Eor => "eor",
        }
    }
}

/// The saved states a world numbers its own records in (`0x6F...`), inclusive: well above the
/// highest the game's own files hold, those of the final retail client and of the end of retail
/// alike, so a world's record never takes the id of one a player's files have.
pub const WORLD_STATES: (u32, u32) = (0x6F10_0000, 0x6FFF_FFFF);

/// The world's own saved states a shop's copy of its store template takes, inclusive: the first of
/// [`WORLD_STATES`], up to below [`WORLD_RECIPES`].
pub const SHOP_COPIES: (u32, u32) = (WORLD_STATES.0, WORLD_RECIPES.0 - 1);

/// The world's own saved states its recipe rows (`[[recipe]]`) take, inclusive, above every shop's
/// copy ([`SHOP_COPIES`]).
pub const WORLD_RECIPES: (u32, u32) = (0x6F7E_0000, 0x6F7E_FFFF);

// The two lie inside the world's own saved states and share no id, however many shops a world has.
const _: () = assert!(
    WORLD_STATES.0 <= SHOP_COPIES.0
        && SHOP_COPIES.0 <= SHOP_COPIES.1
        && SHOP_COPIES.1 < WORLD_RECIPES.0
        && WORLD_RECIPES.0 <= WORLD_RECIPES.1
        && WORLD_RECIPES.1 <= WORLD_STATES.1
);

/// What the game's symbols name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    /// A static object a pack places: its world id is numbered within its landblock, above the
    /// highest the base uses there.
    Static,
    /// A shop's copy of its store template (`[[store]]`, by its key): a saved state numbered in
    /// [`SHOP_COPIES`].
    Store,
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.kind_name())
    }
}

impl SymbolKind for Kind {
    const KINDS: &'static [Self] = &[Self::Static, Self::Store];

    fn kind_name(self) -> &'static str {
        match self {
            Self::Static => "static",
            Self::Store => "store",
        }
    }

    fn numbering(self) -> Numbering {
        match self {
            Self::Static => Numbering::InLandblock,
            Self::Store => Numbering::Counted {
                start: SHOP_COPIES.0,
                world_sets: false,
            },
        }
    }

    fn written_in_hex(self) -> bool {
        true
    }
}

impl Dialect for Ac2 {
    const GAME: &'static str = GAME;
    const PRODUCT: &'static str = "Yalain";
    const BUILD_COMMAND: &'static str = "yalain world compile";
    const ITEM_FORMS: &'static str = "object <key>@<XXYY>, camp <key>@<XXYY>, place <key>, \
         loot <value>, keeper <template>, store <key>, sale <store> <ordinal>, quest <id>, \
         step <quest> <key>, talk <key>, weapon <name>, route <key>, area <XXYY>, \
         clearing <key>@<XXYY>, death_treasure <key>, treasure_pool <name>, \
         treasure_value <level>, treasure_rules, treasure_sort <sort>, treasure_word <sort> <word>, \
         treasure_look <template>, treasure_potion <kind>, \
         treasure_family <sort> <order>, item_use <template>, book <template>, \
         start_item <template>, item_life <template>, interaction <key>, recipe <id>, combination <key>, object_states <key>, \
         sequence <key>, concealing <effect> <class|any>, npc_table <name>, \
         friend_table <table>, creature <template>, away_regen <vital>, allegiance_oath, consignments, \
         or landblock <XXYY>";
    const NAMES_BASE: bool = true;
    type Kind = Kind;
    type Era = Era;

    fn era(name: Option<&str>) -> Result<Era, String> {
        match name {
            Some("eor") => Ok(Era::Eor),
            Some(other) => Err(format!(
                "era {other:?} is not one of {ERAS:?}: a world of this game is built on the end of retail"
            )),
            None => Err(format!(
                "a world of this game names its era, one of {ERAS:?}"
            )),
        }
    }

    fn item(_text: &str, words: &[&str]) -> Option<String> {
        Item::parse(words).map(|i| i.to_string())
    }
}

/// One thing a pack's row sets, as the build's report, an override and a removal name it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Item {
    /// A person or an object placed in a landblock, by its key.
    Object { key: String, landblock: u16 },
    /// A spawn camp in a landblock, by its key.
    Camp { key: String, landblock: u16 },
    /// A destination, by its key.
    Place { key: String },
    /// A death-loot profile, by its value.
    Loot { profile: u32 },
    /// A storekeeper, by its template.
    Keeper { template: u32 },
    /// A shop of the world's own, by its key.
    Store { key: String },
    /// A store's listing, by its store template and its ordinal.
    Sale { store: u32, ordinal: u32 },
    /// A quest's server values, by its id.
    Quest { id: u32 },
    /// A step of a quest, by the quest and the step's key.
    Step { quest: u32, key: String },
    /// A give that moves no quest, by its key.
    Give { key: String },
    /// A line a person says, by its key.
    Talk { key: String },
    /// A craft skill or a recipe a new character starts with, by its data id.
    Craft { id: u32 },
    /// What a butchery profile yields, by the profile's data id.
    Butchery { profile: u32 },
    /// What a gathering tool yields when it was not made, by its template.
    Tool { template: u32 },
    /// A scene of the story, by its id.
    Scene { id: u32 },
    /// The scene a shard bestows, by the shard's world id.
    Shard { object: u64 },
    /// The template of a weapon creatures wield, by the name their templates give it.
    Weapon { name: String },
    /// What a creature's death effect makes around its killer, by the effect's name.
    DeathSpawn { effect: String },
    /// A fixed item a creature or a chest leaves at a chance of its own, by its key.
    DeathTreasure { key: String },
    /// A kind of pet, by the effect or contract that names it.
    Pet { effect: u32 },
    /// What a summoning skill allows its user, by the skill.
    PetLimit { skill: u32 },
    /// A route creatures walk at rest, by its key.
    Route { key: String },
    /// The kind of player conflict a landblock allows, by the landblock.
    Area { landblock: u16 },
    /// Ground kept free of what the land grows, by its key in its landblock.
    Clearing { key: String, landblock: u16 },
    /// A treasure pool, by its name.
    TreasurePool { name: String },
    /// The saved state an effect's name stands for, by the name.
    EffectName { name: String },
    /// A scheduled event, by its key.
    Event { key: String },
    /// A count the whole world keeps, by its key.
    Tally { key: String },
    /// What an effect does to kingdom points, by the effect's saved state.
    PointsBonus { effect: u32 },
    /// What a kingdom's shrines ask of a player who would join it, by the kingdom's word.
    KingdomJoin { kingdom: String },
    /// What a vital regains while its character is out of the world, by the vital's word.
    AwayRegen { vital: String },
    /// What an oath of allegiance asks besides what every oath does (a world has one).
    AllegianceOath,
    /// How many consignments a player holds at a broker (a world has one).
    Consignments,
    /// The tip a player is shown once, by the moment it is shown at.
    Popup { on: String },
    /// What the world says of an effect, by the effect's saved state.
    EffectRule { effect: u32 },
    /// What the world says an effect the install holds no saved state of does, by its name.
    EffectRuleNamed { name: String },
    /// The values treasure is drawn worth, by the level of the creature that leaves it.
    TreasureValue { level: u32 },
    /// A find any creature's corpse may hold, by key.
    CorpseFind { key: String },
    /// The unit treasure is worth from a least level on, by the level.
    TreasureUnit { least: u32 },
    /// The rules every treasure item draws by: the world has one.
    TreasureRules,
    /// The numbers one sort of treasure item draws by, by the sort's name.
    TreasureSort { sort: String },
    /// A word before treasure items' names, by its sort and its string.
    TreasureWord { sort: String, word: u32 },
    /// The look a template's treasure items draw, by the template.
    TreasureLook { template: u32 },
    /// A kind of treasure potion, by its name.
    TreasurePotion { kind: String },
    /// A family of enchantments treasure draws, by its sort and its place in the order.
    TreasureFamily { sort: String, order: u32 },
    /// What a mining craft skill's nodes do, by the skill.
    Mine { skill: u32 },
    /// The nodes a region's surveyor's maps lead to, by the region's key.
    MapRegion { region: String },
    /// What an item's use does where its template names nothing, by the template.
    ItemUse { template: u32 },
    /// What a book, card or person shows when used, by the template.
    Book { template: u32 },
    /// What examining an object of a template says where its template names nothing, by the
    /// template.
    Examination { template: u32 },
    /// What examining one placed object says of it, by the object (`key@XXYY`, or
    /// `pack:key@XXYY`).
    ObjectExamination { object: String },
    /// What every new character is given as it first enters, by the template.
    StartItem { template: u32 },
    /// How long an item lasts from when it is made, by the template.
    ItemLife { template: u32 },
    /// What an item used on another makes, by its key.
    Interaction { key: String },
    /// The states objects of some templates hold, by the row's key.
    ObjectStates { key: String },
    /// An order of uses judged against objects' states, by its key.
    Sequence { key: String },
    /// Levers whose pulls open doors, by the row's key.
    Lever { key: String },
    /// An object that takes a set of items and then opens the way, by the row's key.
    Offering { key: String },
    /// Placed objects a camp's creatures guard, by the row's key.
    Guarded { key: String },
    /// Placed objects of which one stands at a time, by the row's key.
    Shifting { key: String },
    /// Placed objects that each want one item of a set, drawn at random, by the row's key.
    Wanting { key: String },
    /// An allegiance hall's binding stone, by the row's key.
    AllegianceHall { key: String },
    /// One creature at a time going round places in order, by the row's key.
    Round { key: String },
    /// Placed objects or spots that harm whoever uses one or comes near, by the row's key.
    Hazard { key: String },
    /// A region's surveyor's map recipes, by the region's key.
    MapRecipes { region: String },
    /// The totem spellbinding makes of a sort of item, by the sort's name.
    Spellbinding { sort: String },
    /// What a refining recipe yields, by its script package's name.
    Refining { recipe: String },
    /// A recipe the world defines, by its id.
    Recipe { id: u32 },
    /// What a recipe that works out its own product makes of a set of ingredients, by its key.
    Combination { key: String },
    /// What creatures of a class (none: every creature) make of a concealing effect's holder, by
    /// the effect and the class.
    Concealing { effect: u32, class: Option<u32> },
    /// What the world says of a behavior table's creatures, by the table's name.
    NpcTable { name: String },
    /// What the world says of a friend table's creatures, by the table.
    FriendTable { table: u32 },
    /// What the world says of a template's creatures, by the template.
    Creature { template: u32 },
    /// What the world says of an AI class's creatures, by the class.
    AiClass { class: u32 },
    /// Creatures that let pass a player holding an item or bearing an effect, by the row's key.
    SafePassage { key: String },
    /// An effect creatures of some templates hold from their first moment, by the row's key.
    CreatureEffect { key: String },
    /// A defended device, by its template.
    Defense { device: u32 },
    /// A species of dye plant, by its plant's template.
    Plant { template: u32 },
    /// What a dye plant pollinated with a pollen gives, by the plant's template and the pollen's.
    PlantCross { plant: u32, pollen: u32 },
    /// A worn look of a template, by the template and the races and sexes it fits.
    Worn {
        template: u32,
        races: u32,
        sexes: u32,
    },
}

impl Item {
    /// An item as an override or a removal writes it, split into words; `None` when it names
    /// none of the game's.
    #[must_use]
    pub fn parse(words: &[&str]) -> Option<Self> {
        Some(match words {
            ["object", placed] => {
                let (key, landblock) = placed_key(placed)?;
                Self::Object { key, landblock }
            }
            ["camp", placed] => {
                let (key, landblock) = placed_key(placed)?;
                Self::Camp { key, landblock }
            }
            ["clearing", placed] => {
                let (key, landblock) = placed_key(placed)?;
                Self::Clearing { key, landblock }
            }
            ["place", key] => Self::Place { key: symbol(key)? },
            ["loot", v] => Self::Loot {
                profile: number(v)?,
            },
            ["keeper", t] => Self::Keeper {
                template: number(t)?,
            },
            ["store", key] => Self::Store { key: symbol(key)? },
            ["sale", s, o] => Self::Sale {
                store: number(s)?,
                ordinal: number(o)?,
            },
            ["quest", q] => Self::Quest { id: number(q)? },
            ["step", q, key] => Self::Step {
                quest: number(q)?,
                key: symbol(key)?,
            },
            ["give", key] => Self::Give { key: symbol(key)? },
            ["talk", key] => Self::Talk { key: symbol(key)? },
            ["craft", id] => Self::Craft { id: number(id)? },
            ["butchery", p] => Self::Butchery {
                profile: number(p)?,
            },
            ["tool", t] => Self::Tool {
                template: number(t)?,
            },
            ["scene", id] => Self::Scene { id: number(id)? },
            ["shard", o] => Self::Shard {
                object: number64(o)?,
            },
            ["weapon", name] => Self::Weapon {
                name: identifier(name)?,
            },
            ["death_spawn", effect] => Self::DeathSpawn {
                effect: identifier(effect)?,
            },
            ["death_treasure", key] => Self::DeathTreasure { key: symbol(key)? },
            ["pet", effect] => Self::Pet {
                effect: number(effect)?,
            },
            ["pet_limit", skill] => Self::PetLimit {
                skill: number(skill)?,
            },
            ["route", key] => Self::Route { key: symbol(key)? },
            ["treasure_pool", name] => Self::TreasurePool {
                name: identifier(name)?,
            },
            ["event", key] => Self::Event { key: symbol(key)? },
            ["tally", key] => Self::Tally { key: symbol(key)? },
            ["points_bonus", effect] => Self::PointsBonus {
                effect: number(effect)?,
            },
            ["effect_name", name] => Self::EffectName {
                name: identifier(name)?,
            },
            ["kingdom_join", kingdom] => Self::KingdomJoin {
                kingdom: symbol(kingdom)?,
            },
            ["allegiance_oath"] => Self::AllegianceOath,
            ["consignments"] => Self::Consignments,
            ["away_regen", vital] => Self::AwayRegen {
                vital: symbol(vital)?,
            },
            ["popup", on] => Self::Popup { on: symbol(on)? },
            ["effect_rule", effect] => match number(effect) {
                Some(effect) => Self::EffectRule { effect },
                None => Self::EffectRuleNamed {
                    name: identifier(effect)?,
                },
            },
            ["treasure_value", level] => Self::TreasureValue {
                level: number(level)?,
            },
            ["corpse_find", key] => Self::CorpseFind { key: symbol(key)? },
            ["treasure_unit", least] => Self::TreasureUnit {
                least: number(least)?,
            },
            ["treasure_rules"] => Self::TreasureRules,
            ["treasure_sort", sort] => Self::TreasureSort {
                sort: (*sort).to_string(),
            },
            ["treasure_word", sort, word] => Self::TreasureWord {
                sort: (*sort).to_string(),
                word: number(word)?,
            },
            ["treasure_look", template] => Self::TreasureLook {
                template: number(template)?,
            },
            ["treasure_potion", kind] => Self::TreasurePotion {
                kind: (*kind).to_string(),
            },
            ["treasure_family", sort, order] => Self::TreasureFamily {
                sort: (*sort).to_string(),
                order: number(order)?,
            },
            ["mine", skill] => Self::Mine {
                skill: number(skill)?,
            },
            ["map_region", region] => Self::MapRegion {
                region: symbol(region)?,
            },
            ["item_use", template] => Self::ItemUse {
                template: number(template)?,
            },
            ["book", template] => Self::Book {
                template: number(template)?,
            },
            ["examination", object] if object.contains('@') => Self::ObjectExamination {
                object: (*object).to_string(),
            },
            ["examination", template] => Self::Examination {
                template: number(template)?,
            },
            ["start_item", template] => Self::StartItem {
                template: number(template)?,
            },
            ["item_life", template] => Self::ItemLife {
                template: number(template)?,
            },
            ["interaction", key] => Self::Interaction { key: symbol(key)? },
            ["object_states", key] => Self::ObjectStates { key: symbol(key)? },
            ["sequence", key] => Self::Sequence { key: symbol(key)? },
            ["lever", key] => Self::Lever { key: symbol(key)? },
            ["offering", key] => Self::Offering { key: symbol(key)? },
            ["guarded", key] => Self::Guarded { key: symbol(key)? },
            ["shifting", key] => Self::Shifting { key: symbol(key)? },
            ["wanting", key] => Self::Wanting { key: symbol(key)? },
            ["allegiance_hall", key] => Self::AllegianceHall { key: symbol(key)? },
            ["round", key] => Self::Round { key: symbol(key)? },
            ["hazard", key] => Self::Hazard { key: symbol(key)? },
            ["map_recipes", region] => Self::MapRecipes {
                region: symbol(region)?,
            },
            ["spellbinding", sort] => Self::Spellbinding {
                sort: (*sort).to_string(),
            },
            ["refining", recipe] => Self::Refining {
                recipe: identifier(recipe)?,
            },
            ["recipe", id] => Self::Recipe { id: number(id)? },
            ["combination", key] => Self::Combination { key: symbol(key)? },
            ["concealing", effect, class] => Self::Concealing {
                effect: number(effect)?,
                class: match *class {
                    "any" => None,
                    c => Some(number(c)?),
                },
            },
            ["npc_table", name] => Self::NpcTable {
                name: (*name).to_string(),
            },
            ["friend_table", table] => Self::FriendTable {
                table: number(table)?,
            },
            ["creature", template] => Self::Creature {
                template: number(template)?,
            },
            ["ai_class", class] => Self::AiClass {
                class: number(class)?,
            },
            ["safe_passage", key] => Self::SafePassage { key: symbol(key)? },
            ["creature_effect", key] => Self::CreatureEffect { key: symbol(key)? },
            ["defense", device] => Self::Defense {
                device: number(device)?,
            },
            ["plant", template] => Self::Plant {
                template: number(template)?,
            },
            ["plant_cross", plant, pollen] => Self::PlantCross {
                plant: number(plant)?,
                pollen: number(pollen)?,
            },
            ["worn", template, races, sexes] => Self::Worn {
                template: number(template)?,
                races: number(races)?,
                sexes: number(sexes)?,
            },
            ["area", lb] => Self::Area {
                landblock: u16::from_str_radix(lb, 16).ok().filter(|_| lb.len() == 4)?,
            },
            _ => return None,
        })
    }

    /// The landblock a placed item stands in.
    #[must_use]
    pub const fn landblock(&self) -> Option<u16> {
        match self {
            Self::Object { landblock, .. } | Self::Camp { landblock, .. } => Some(*landblock),
            _ => None,
        }
    }
}

impl fmt::Display for Item {
    /// The one form the build's report names it in.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Object { key, landblock } => write!(f, "object {key}@{landblock:04X}"),
            Self::Camp { key, landblock } => write!(f, "camp {key}@{landblock:04X}"),
            Self::Place { key } => write!(f, "place {key}"),
            Self::Loot { profile } => write!(f, "loot {profile:#x}"),
            Self::Keeper { template } => write!(f, "keeper {template:#010x}"),
            Self::Store { key } => write!(f, "store {key}"),
            Self::Sale { store, ordinal } => write!(f, "sale {store:#010x} {ordinal}"),
            Self::Quest { id } => write!(f, "quest {id:#010x}"),
            Self::Step { quest, key } => write!(f, "step {quest:#010x} {key}"),
            Self::Give { key } => write!(f, "give {key}"),
            Self::Talk { key } => write!(f, "talk {key}"),
            Self::Craft { id } => write!(f, "craft {id:#010x}"),
            Self::Butchery { profile } => write!(f, "butchery {profile:#010x}"),
            Self::Tool { template } => write!(f, "tool {template:#010x}"),
            Self::Scene { id } => write!(f, "scene {id:#010x}"),
            Self::Shard { object } => write!(f, "shard {object:#018x}"),
            Self::Weapon { name } => write!(f, "weapon {name}"),
            Self::DeathSpawn { effect } => write!(f, "death_spawn {effect}"),
            Self::DeathTreasure { key } => write!(f, "death_treasure {key}"),
            Self::Pet { effect } => write!(f, "pet {effect:#010x}"),
            Self::PetLimit { skill } => write!(f, "pet_limit {skill:#010x}"),
            Self::Route { key } => write!(f, "route {key}"),
            Self::Area { landblock } => write!(f, "area {landblock:04X}"),
            Self::Clearing { key, landblock } => write!(f, "clearing {key}@{landblock:04X}"),
            Self::TreasurePool { name } => write!(f, "treasure_pool {name}"),
            Self::EffectName { name } => write!(f, "effect_name {name}"),
            Self::Event { key } => write!(f, "event {key}"),
            Self::Tally { key } => write!(f, "tally {key}"),
            Self::PointsBonus { effect } => write!(f, "points_bonus {effect:#010x}"),
            Self::KingdomJoin { kingdom } => write!(f, "kingdom_join {kingdom}"),
            Self::AwayRegen { vital } => write!(f, "away_regen {vital}"),
            Self::AllegianceOath => write!(f, "allegiance_oath"),
            Self::Consignments => write!(f, "consignments"),
            Self::Popup { on } => write!(f, "popup {on}"),
            Self::EffectRule { effect } => write!(f, "effect_rule {effect:#010x}"),
            Self::EffectRuleNamed { name } => write!(f, "effect_rule {name}"),
            Self::TreasureValue { level } => write!(f, "treasure_value {level}"),
            Self::CorpseFind { key } => write!(f, "corpse_find {key}"),
            Self::TreasureUnit { least } => write!(f, "treasure_unit {least}"),
            Self::TreasureRules => write!(f, "treasure_rules"),
            Self::TreasureSort { sort } => write!(f, "treasure_sort {sort}"),
            Self::TreasureWord { sort, word } => write!(f, "treasure_word {sort} {word:#010x}"),
            Self::TreasureLook { template } => write!(f, "treasure_look {template:#010x}"),
            Self::TreasurePotion { kind } => write!(f, "treasure_potion {kind}"),
            Self::TreasureFamily { sort, order } => write!(f, "treasure_family {sort} {order}"),
            Self::Mine { skill } => write!(f, "mine {skill:#010x}"),
            Self::MapRegion { region } => write!(f, "map_region {region}"),
            Self::ItemUse { template } => write!(f, "item_use {template:#010x}"),
            Self::Book { template } => write!(f, "book {template:#010x}"),
            Self::Examination { template } => write!(f, "examination {template:#010x}"),
            Self::ObjectExamination { object } => write!(f, "examination {object}"),
            Self::StartItem { template } => write!(f, "start_item {template:#010x}"),
            Self::ItemLife { template } => write!(f, "item_life {template:#010x}"),
            Self::Interaction { key } => write!(f, "interaction {key}"),
            Self::ObjectStates { key } => write!(f, "object_states {key}"),
            Self::Sequence { key } => write!(f, "sequence {key}"),
            Self::Lever { key } => write!(f, "lever {key}"),
            Self::Offering { key } => write!(f, "offering {key}"),
            Self::Guarded { key } => write!(f, "guarded {key}"),
            Self::Shifting { key } => write!(f, "shifting {key}"),
            Self::Wanting { key } => write!(f, "wanting {key}"),
            Self::AllegianceHall { key } => write!(f, "allegiance_hall {key}"),
            Self::Round { key } => write!(f, "round {key}"),
            Self::Hazard { key } => write!(f, "hazard {key}"),
            Self::MapRecipes { region } => write!(f, "map_recipes {region}"),
            Self::Spellbinding { sort } => write!(f, "spellbinding {sort}"),
            Self::Refining { recipe } => write!(f, "refining {recipe}"),
            Self::Recipe { id } => write!(f, "recipe {id:#010x}"),
            Self::Combination { key } => write!(f, "combination {key}"),
            Self::Concealing {
                effect,
                class: Some(c),
            } => write!(f, "concealing {effect:#010x} {c:#x}"),
            Self::Concealing {
                effect,
                class: None,
            } => write!(f, "concealing {effect:#010x} any"),
            Self::NpcTable { name } => write!(f, "npc_table {name}"),
            Self::FriendTable { table } => write!(f, "friend_table {table:#x}"),
            Self::Creature { template } => write!(f, "creature {template:#010x}"),
            Self::AiClass { class } => write!(f, "ai_class {class:#x}"),
            Self::SafePassage { key } => write!(f, "safe_passage {key}"),
            Self::CreatureEffect { key } => write!(f, "creature_effect {key}"),
            Self::Defense { device } => write!(f, "defense {device:#010x}"),
            Self::Plant { template } => write!(f, "plant {template:#010x}"),
            Self::PlantCross { plant, pollen } => {
                write!(f, "plant_cross {plant:#010x} {pollen:#010x}")
            }
            Self::Worn {
                template,
                races,
                sexes,
            } => write!(f, "worn {template:#010x} {races:#x} {sexes:#x}"),
        }
    }
}

/// Whether `s` is a key: lowercase letters, digits and hyphens, starting with a letter or a
/// digit, at most 64 long.
#[must_use]
pub fn is_key(s: &str) -> bool {
    s.len() <= 64 && dereth_pack::symbol::is_name(s)
}

/// A name of the install's own: letters, digits and `_`, at most 80 long.
fn identifier(s: &str) -> Option<String> {
    (!s.is_empty() && s.len() <= 80 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'))
        .then(|| s.to_owned())
}

fn symbol(s: &str) -> Option<String> {
    is_key(s).then(|| s.to_owned())
}

/// A placed row's key with its landblock: `KEY@XXYY`.
fn placed_key(s: &str) -> Option<(String, u16)> {
    let (key, _) = s.split_once('@')?;
    let landblock = dereth_pack::symbol::instance_landblock(s).ok()??;
    Some((symbol(key)?, landblock))
}

/// A number written in hexadecimal after `0x`, else in decimal.
fn number64(s: &str) -> Option<u64> {
    match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(h) => u64::from_str_radix(h, 16).ok(),
        None => s.parse().ok(),
    }
}

fn number(s: &str) -> Option<u32> {
    match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(h) => u32::from_str_radix(h, 16).ok(),
        None => s.parse().ok(),
    }
}
