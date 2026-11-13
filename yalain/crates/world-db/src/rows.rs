//! A database's files read into rows: the world file, each pack's header, the lockfile, and every
//! server row with the file and line it came from. The world file, the pack headers, the
//! lockfile, every row's provenance and the items a removal names are read in the shared pack
//! format's grammar, in this game's dialect (`dereth-pack`, `auberean-pack`); the rows are this
//! database's own. Reading checks the schema: every table and field is one the format has, of
//! its type, with the fields its kind needs; a row that breaks it is a [`Problem`] and is left
//! out.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use auberean_pack::Ac2;
use dereth_pack::conflict::Override;
use dereth_pack::pack::{PackHeader, Remove};
use dereth_pack::world::Base;
use yalain_content::movement::{Gait, MovementOverrides, MovementType, Shape};
use yalain_content::talk::{How, Role, TalkUpdate, When};

use crate::{Files, Problem};

/// Where a row came from (`prov`), or where one field of it came from (`prov.fields`): the shared
/// pack format's provenance.
pub use dereth_pack::provenance::{Confidence, Provenance as Prov, Source};

/// The world file: the database's key, game and era, its packs in load order, which pack wins
/// where two set one item, and the base database it lies on, if it names one.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct World {
    pub format_version: i64,
    pub key: String,
    pub game: String,
    pub era: String,
    pub packs: Vec<String>,
    pub overrides: Vec<Override>,
    pub base: Option<Base>,
    /// The ids its lockfile gives its shops' copies of their store templates, by the shops' keys.
    pub stores: BTreeMap<String, u32>,
}

/// One pack: its header and its rows.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Pack {
    /// The folder, as the world file names it.
    pub folder: String,
    pub name: String,
    pub version: String,
    pub game: String,
    pub license: String,
    pub rows: Vec<Located>,
}

/// A row and where it was written.
#[derive(Debug, Clone, PartialEq)]
pub struct Located {
    pub row: Row,
    /// The file, relative to the database's folder.
    pub file: String,
    pub line: usize,
    /// The landblock of a `server/landblocks/XXYY.toml` file.
    pub landblock: Option<u16>,
    /// The pack's name.
    pub pack: String,
}

/// A database read: its world file and its packs in load order.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Database {
    pub world: World,
    pub packs: Vec<Pack>,
}

impl Database {
    /// Every row of every pack, in load order.
    pub fn rows(&self) -> impl Iterator<Item = &Located> {
        self.packs.iter().flat_map(|p| p.rows.iter())
    }
}

/// A string reference into the install's text: a table and a string id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StrRef {
    pub table: u32,
    pub string: u32,
}

/// What a person or an object of the world is: a person the server numbers, or an object with an
/// id of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectKind {
    Npc,
    Static,
}

/// A point: x and y in the landblock, and z, or the land's height there.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct At {
    pub x: f32,
    pub y: f32,
    /// `None`: the land's height at x, y.
    pub z: Option<f32>,
}

/// A recall's landing at a lifestone.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Landing {
    pub cell: u32,
    pub at: At,
    pub heading: Option<f32>,
    pub seen: Option<u32>,
}

/// `[[object]]`: a person or an object of the world.
#[derive(Debug, Clone, PartialEq)]
pub struct Object {
    pub key: String,
    pub kind: ObjectKind,
    pub id: Option<u64>,
    pub template: u32,
    pub cell: Option<u32>,
    pub at: At,
    pub rotation: Option<[f32; 4]>,
    pub heading: Option<f32>,
    /// How large it stands, in place of the scale its template gives.
    pub scale: Option<f32>,
    pub name: Option<StrRef>,
    pub name_text: Option<String>,
    pub to: Option<String>,
    pub landings: Vec<Landing>,
    /// A door's scene: the scene a player plays on the way to `to`, in place of portal space.
    pub scene: Option<u32>,
    /// A locked door, and the key id that opens it (a key's `Lock_KeyID`).
    pub locked: bool,
    pub lock: Option<u32>,
    /// The route a person walks (`key`, or `pack:key` in another pack), the waypoint it sets out
    /// toward (from 0) and whether it starts the way back.
    pub route: Option<String>,
    pub route_start: u32,
    pub route_backward: bool,
    /// The quest a player must hold, and where, to use it (a portal its placed instance keeps
    /// closed).
    pub requires: Option<(u32, Gate)>,
    /// The least level a player must be to use it (over its template's).
    pub min_level: Option<i32>,
    /// Only a hero may use it.
    pub hero: bool,
    /// The kingdom a player must be of to use it, as a description's faction names it.
    pub kingdom: Option<u32>,
    /// A portal a player who used it may recall to, and summon a portal to.
    pub recall: bool,
    pub summon: bool,
    /// The `[[event]]` it stands only while it runs.
    pub event: Option<String>,
    pub prov: Prov,
}

/// Where an object's quest must be for a player to use it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gate {
    /// Completed.
    Done,
    /// Under way, at any phase.
    UnderWay,
    /// Under way at this phase.
    Phase(u32),
    /// Failed, completed or under way.
    Had,
}

/// `[[camp]]`: a generator, as the recordings saw it.
#[derive(Debug, Clone, PartialEq)]
pub struct Camp {
    pub key: String,
    pub cell: Option<u32>,
    pub at: At,
    pub radius: f32,
    pub spots: Vec<(u32, At)>,
    pub population: u32,
    pub regen: f64,
    pub spawns: Vec<(u32, u32)>,
    pub profile: Option<u32>,
    pub profile_name: Option<String>,
    pub parent: Option<String>,
    /// How its creatures move, over their templates' settings.
    pub moves: MovementOverrides,
    /// The route its creatures walk at rest (`key`, or `pack:key` in another pack), the waypoint
    /// they start toward (from 0) and whether they start the way back.
    pub route: Option<String>,
    pub route_start: u32,
    pub route_backward: bool,
    /// The name its creatures are told by in place of their template's.
    pub name: Option<StrRef>,
    /// The `[[event]]` it keeps its creatures only while it runs.
    pub event: Option<String>,
    pub prov: Prov,
}

/// One place of a route: a cell, a point, and the rest there when the waypoint sets its own.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Waypoint {
    pub cell: u32,
    pub at: At,
    pub rest: Option<f64>,
}

/// `[[route]]`: places creatures walk at rest in turn, back and forth or round a loop, resting at
/// each (a waypoint's own rest, else the route's), at a walk or a run.
#[derive(Debug, Clone, PartialEq)]
pub struct Route {
    pub key: String,
    pub waypoints: Vec<Waypoint>,
    pub shape: Shape,
    pub gait: Gait,
    /// How fast it is walked by a creature of scale 1, in meters a second, when not its gait's own.
    pub speed: Option<f32>,
    pub rest: f64,
    pub prov: Prov,
}

/// `[[loot]]`: what a dead creature leaves, by its death-loot profile.
#[derive(Debug, Clone, PartialEq)]
pub struct Loot {
    pub profile: u32,
    pub profile_name: Option<String>,
    pub counts: Vec<f32>,
    pub items: Vec<(u32, u32)>,
    pub extras: Vec<(u32, f32)>,
    /// A chest's: the level its draw is treasure of, and the worth each piece draws from.
    pub level: Option<i32>,
    pub value: Vec<u32>,
    /// What its items are drawn from: treasure pools, each by a weight and the item levels it is
    /// drawn at.
    pub draws: Vec<PoolEntry>,
    /// The pool its rare is drawn from, and the chance, rolled once a corpse.
    pub rare: Option<(String, f32)>,
    pub prov: Prov,
}

/// One entry of a treasure pool, or one draw of a `[[loot]]` row: a pool or a template, its
/// weight, and the least and most item level it is drawn at (none: open that way).
#[derive(Debug, Clone, PartialEq)]
pub struct PoolEntry {
    pub pool: Option<String>,
    pub template: Option<u32>,
    pub weight: f32,
    pub least: Option<i32>,
    pub most: Option<i32>,
}

/// `[[treasure_pool]]`: a named pool treasure is drawn from: other pools, item templates, or both.
#[derive(Debug, Clone, PartialEq)]
pub struct TreasurePool {
    pub name: String,
    /// The race it is for, by name.
    pub race: Option<String>,
    /// The share of a draw from its parent by a killer of its race that it takes outright.
    pub own: Option<f32>,
    pub pools: Vec<PoolEntry>,
    pub items: Vec<PoolEntry>,
    pub prov: Prov,
}

/// `[[treasure_value]]`: the own values a treasure item a creature of a level leaves is drawn from,
/// each as often as it was given.
#[derive(Debug, Clone, PartialEq)]
pub struct TreasureValue {
    /// The creature's level.
    pub level: u32,
    pub values: Vec<u32>,
    pub prov: Prov,
}

/// `[[treasure_rules]]`: what every treasure item draws by, whatever its sort: its own least level
/// above its dropper's base, its Arcane Lore, a potion's tier and spellcraft, and its look. A world
/// has one.
#[derive(Debug, Clone, PartialEq)]
pub struct TreasureRules {
    pub rules: yalain_content::treasure_tables::Rules,
    pub prov: Prov,
}

/// `[[treasure_sort]]`: the numbers one sort of treasure item draws by: how many enchantments by
/// level band, and (by the sort) its share of a made item's damage, its vigor cost, its span, its
/// combat delay, its share of a made item's rating and its enchantment slot. Its families and
/// words are not among them.
#[derive(Debug, Clone, PartialEq)]
pub struct TreasureSort {
    pub sort: yalain_content::treasure_tables::Sort,
    pub tables: yalain_content::treasure_tables::SortTables,
    pub prov: Prov,
}

/// The fields of a `[[treasure_sort]]` row beside `sort`, `counts` and `prov` that each sort has;
/// a row of a sort has all of its own and none of the others.
pub const TREASURE_SORT_FIELDS: [(&str, &[&str]); 4] = [
    ("weapon", &["damage_share", "vigor", "delays"]),
    (
        "armor",
        &["delays", "rating_share", "rating_spread", "slot_chance"],
    ),
    (
        "shield",
        &[
            "damage_share",
            "vigor",
            "span",
            "delays",
            "rating_share",
            "rating_spread",
            "slot_chance",
        ],
    ),
    ("jewelry", &[]),
];

/// `[[treasure_word]]`: a word before a treasure item's name: its sort, its place in the order
/// the words are drawn in, its string, the combat delays it goes with, and its weight in each of
/// its sort's count bands.
#[derive(Debug, Clone, PartialEq)]
pub struct TreasureWord {
    pub sort: yalain_content::treasure_tables::Sort,
    pub order: u32,
    pub word: yalain_content::treasure_tables::Word,
    pub prov: Prov,
}

/// `[[treasure_look]]`: the look a template's treasure items draw: the appearance table, and each
/// key with the highest value it draws.
#[derive(Debug, Clone, PartialEq)]
pub struct TreasureLook {
    pub look: yalain_content::treasure_tables::LookRow,
    pub prov: Prov,
}

/// `[[treasure_potion]]`: a kind of treasure potion: its name, its place in the order the kinds
/// are drawn in, and its effect's saved state at each tier.
#[derive(Debug, Clone, PartialEq)]
pub struct TreasurePotion {
    pub kind: String,
    pub order: u32,
    pub states: Vec<u32>,
    pub prov: Prov,
}

/// `[[treasure_family]]`: a family of enchantments one sort of treasure item draws from: its place
/// in the order the families are drawn in, and the family; and, from its tiers, the states that
/// come with a tier's (its pairs) and the tiers no item is named for.
#[derive(Debug, Clone, PartialEq)]
pub struct TreasureFamily {
    pub sort: yalain_content::treasure_tables::Sort,
    pub order: u32,
    pub family: yalain_content::treasure_tables::Family,
    pub pairs: Vec<(u32, u32)>,
    pub unnamed: Vec<u32>,
    pub prov: Prov,
}

/// The string table the words before treasure items' names are in.
pub const WORD_TABLE: u32 = 0x2500_035F;

/// The races a treasure pool can be for, and their bits.
pub const POOL_RACES: [(&str, u32); 5] = [
    ("human", 1),
    ("lugian", 2),
    ("tumerok", 4),
    ("drudge", 8),
    ("empyrean", 16),
];

/// `[[keeper]]`: a storekeeper template, the store it opens and its greeting.
#[derive(Debug, Clone, PartialEq)]
pub struct Keeper {
    pub template: u32,
    pub store: Option<u32>,
    /// The `[[store]]` it opens, by key: that shop's store template, under the shop's title.
    pub shop: Option<String>,
    pub greeting: Option<StrRef>,
    pub greeting_text: Option<String>,
    pub prov: Prov,
}

/// `[[store]]`: a shop of the world's own, a store template's stock, prices and portrait under a
/// title of its own.
#[derive(Debug, Clone, PartialEq)]
pub struct Store {
    pub key: String,
    /// The store template it copies.
    pub from: u32,
    /// The shop window's title.
    pub name_text: String,
    /// The line under the title, else the template's.
    pub description_text: Option<String>,
    pub prov: Prov,
}

/// `[[sale]]`: what a store's listing shows that the store template lacks.
#[derive(Debug, Clone, PartialEq)]
pub struct Sale {
    pub store: u32,
    pub ordinal: u32,
    pub stack: Option<u32>,
    pub icon: Vec<(u32, u32, u32)>,
    pub product: Option<u32>,
    pub cost: Option<f32>,
    pub currency: Option<u32>,
    pub name: Option<StrRef>,
    pub name_text: Option<String>,
    pub prov: Prov,
}

/// `[[quest]]`: a quest's server values.
#[derive(Debug, Clone, PartialEq)]
pub struct Quest {
    pub id: u32,
    pub id_name: Option<String>,
    pub state: Option<u32>,
    pub challenge: Option<i32>,
    pub duration: Option<f64>,
    /// The one phase its duration runs in, from when that phase comes; left out, all of them from
    /// its giving.
    pub timed_phase: Option<u32>,
    pub retry: Option<f64>,
    /// The wait after a cancel before it may be taken again; left out, the rule's.
    pub cancel_wait: Option<f64>,
    pub xp: Option<u64>,
    /// In place of an amount, the share of what the player's level takes to reach the next that its
    /// completion pays; left out, as the quest's setup in the install says.
    pub xp_share: Option<f64>,
    /// The level at and above which that share pays no more.
    pub xp_share_level: Option<i32>,
    /// The least a row's share pays.
    pub xp_least: Option<u64>,
    /// The most a row's share pays.
    pub xp_most: Option<u64>,
    /// Its amount is paid whole, whatever its challenge against the player's level; left out, as
    /// the quest's setup in the install says.
    pub xp_whole: Option<bool>,
    /// What its completion adds to the player's quest rating; left out, as the quest's setup in the
    /// install says.
    pub qp: Option<u32>,
    pub cancellable: Option<bool>,
    /// Whether the quest is shown to its player; left out, a quest is shown when its definition
    /// has a description.
    pub visible: Option<bool>,
    /// The least level a player is given it at.
    pub min_level: Option<i32>,
    /// Whether it fails when its player dies, leaves the world, or is taken through portal space;
    /// each left out, as the quest's setup in the install says.
    pub fails_on_death: Option<bool>,
    pub fails_on_logout: Option<bool>,
    pub fails_on_teleport: Option<bool>,
    /// Whether it is given only once.
    pub once: Option<bool>,
    /// Whether a kill's move on it reaches the killer's fellows; left out, as the quest's setup in
    /// the install says.
    pub fellows: Option<bool>,
    /// The quest this one goes with: as a player's quest of that id is done, fails or is cancelled,
    /// this one under way, the player's and each of its fellows' in the world, is done or fails.
    pub companion_of: Option<u32>,
    pub prov: Prov,
}

/// What moves a quest step: using something of a template (or a held item of one on an object of
/// another), arriving through a portal of a template, taking or killing something of a template,
/// completing a quest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Trigger {
    Use(u32),
    UseOn(u32, u32),
    Give(u32, u32),
    /// Giving an item of any template of a class (a script package, by number) to one of a template.
    GiveClass(u32, u32),
    Arrive(u32),
    /// Arriving through one placed portal of the template: the object's id.
    ArriveFrom(u32, u64),
    Take(u32),
    Kill(u32),
    /// Killing a creature of the template that one camp made: the camp's `key`, or `key@XXYY` for
    /// the one in that landblock where several camps share the key.
    KillIn(u32, String),
    /// A creature of the template dying within some meters of the player, whoever kills it.
    KillNear(u32, u32),
    Done(u32),
    Given(u32),
}

/// What a step requires of another quest: under way at a phase, done, free to be given, or failed
/// (its time run out).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Need {
    Phase(u32),
    Done,
    Free,
    Failed,
}

/// The update a step makes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Update {
    Bestow,
    Advance,
    Complete,
    Fail,
    /// Moves it nowhere: the step only says, gives and sends.
    Nothing,
}

/// `[[step]]`: what moves a quest on.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub key: String,
    pub quest: u32,
    pub needs: u32,
    pub trigger: Trigger,
    pub tell: Option<StrRef>,
    pub tell_text: Option<String>,
    pub update: Update,
    pub phase: Option<u32>,
    pub xp: Option<u64>,
    pub gives: Vec<u32>,
    /// How many of each item given, in order (1 when left out).
    pub counts: Vec<u32>,
    /// One more item it gives, one of these templates picked by weight (none: no such item).
    pub gives_one_of: Vec<(u32, u32)>,
    /// The effects it puts on the player (their saved states), and a text shown after them.
    pub effects: Vec<u32>,
    pub text: Option<StrRef>,
    /// The recipes it teaches the player (the install's, or the world's `[[recipe]]` rows').
    pub recipes: Vec<u32>,
    /// Other quests it requires.
    pub requires: Vec<(u32, Need)>,
    /// The `[[event]]`s it runs only while they run.
    pub events: Vec<String>,
    /// The `[[tally]]`s it runs only while they are full, or not yet full.
    pub tallies: Vec<yalain_content::tallies::Gate>,
    /// The kingdom it runs for alone (a player in none counting as of the neutral one).
    pub kingdom: Option<u32>,
    /// The acts it runs for only once they are whole (the scenes an act needs held).
    pub acts: Vec<u32>,
    /// The craft skills it runs for only at a level (any one of a gate's skills at it).
    pub crafts: Vec<yalain_content::crafts::CraftGate>,
    /// It runs only for a player with an untraining under way.
    pub untraining: bool,
    /// It runs only for a hero.
    pub heroes: bool,
    /// What it adds to a `[[tally]]` as it runs (boxed: few steps add).
    pub adds: Option<Box<yalain_content::tallies::Adds>>,
    /// The items it takes from the player's pack, which the player must hold.
    pub takes: Vec<u32>,
    /// The place it takes the player to, its update made on arrival.
    pub to: Option<String>,
    /// The races it is for (none: every race).
    pub races: Vec<u32>,
    /// Its effects come after its update, not before it.
    pub effects_last: bool,
    /// Its effects tell no line of their own; its text is still told.
    pub effects_quiet: bool,
    /// Its update is made for each of the player's fellows as well.
    pub fellows: bool,
    /// For a give, exactly how many of the item it wants (none: whatever is given).
    pub wants: Option<u32>,
    /// The least and the most level it is for, either left out for no such limit.
    pub least_level: Option<i32>,
    pub most_level: Option<i32>,
    /// It pays this share of what the player's level takes to reach the next, whole.
    pub xp_share: Option<f64>,
    /// For a give, it makes its player a hero, after this transformation (boxed: few steps have
    /// one).
    pub transforms: Option<Box<yalain_content::quests::Transformation>>,
    /// The chance it runs each time it may, above 0 and at most 1 (none: every time).
    pub chance: Option<f64>,
    /// The creature it sets to follow its player for the quest (boxed: few steps have one).
    pub escort: Option<Box<yalain_content::escorts::Escort>>,
    /// Where it takes the player, the player's fellows in the world go too.
    pub to_fellows: bool,
    pub prov: Prov,
}

/// `[[give]]`: what a person or an object does with an item given it that moves no quest.
#[derive(Debug, Clone, PartialEq)]
pub struct Give {
    pub key: String,
    /// The template of the person or the object given to.
    pub to: u32,
    /// The template of the item given (none: any item).
    pub item: Option<u32>,
    /// A class (a script package, by number) of the items it takes, any template of which counts.
    pub item_class: Option<u32>,
    /// Exactly how many of the item it wants (none: whatever is given).
    pub wants: Option<u32>,
    /// The items it gives back in its place.
    pub gives: Vec<u32>,
    /// How many of each item given, in order (1 when left out).
    pub counts: Vec<u32>,
    /// One more item it gives, one of these templates picked by weight (none: no such item).
    pub gives_one_of: Vec<(u32, u32)>,
    /// The effects it puts on the player (their saved states), and a text shown after them.
    pub effects: Vec<u32>,
    pub text: Option<StrRef>,
    /// Its effects tell no line of their own; its text is still told.
    pub effects_quiet: bool,
    /// What the person or the object plays as it takes the item.
    pub fx: Option<u32>,
    /// The quests it requires.
    pub requires: Vec<(u32, Need)>,
    /// The `[[tally]]`s it takes the item only while they are full, or not yet full.
    pub tallies: Vec<yalain_content::tallies::Gate>,
    /// The kingdom it takes the item from alone (a player in none counting as of the neutral one).
    pub kingdom: Option<u32>,
    /// The craft skills it takes the item for only at a level (any one of a gate's skills at it).
    pub crafts: Vec<yalain_content::crafts::CraftGate>,
    /// It takes the item only from a player with an untraining under way.
    pub untraining: bool,
    /// It takes the item only from a hero.
    pub heroes: bool,
    /// What it adds to a `[[tally]]` as it takes the item.
    pub adds: Option<yalain_content::tallies::Adds>,
    pub prov: Prov,
}

/// `[[talk]]`: a line a person says, and when.
#[derive(Debug, Clone, PartialEq)]
pub struct Talk {
    pub key: String,
    /// The template that says it.
    pub who: u32,
    /// One placed person only (`key@XXYY`, `pack:key@XXYY` in another pack).
    pub object: Option<String>,
    pub line: Option<StrRef>,
    /// The string token's name, which hashes to `line`'s string id.
    pub line_name: Option<String>,
    pub line_text: Option<String>,
    pub role: Option<Role>,
    pub how: How,
    pub when: When,
    pub quest: Option<u32>,
    pub update: Option<TalkUpdate>,
    pub phase: Option<u32>,
    pub item: Option<u32>,
    pub order: Option<u32>,
    /// The `[[tally]]`s it is said only while they are full, or not yet full.
    pub requires: Vec<yalain_content::tallies::Gate>,
    /// The `[[tally]]` whose count, as a percent, is its one variable.
    pub fills: Option<String>,
    /// What is said in its place once that tally is full.
    pub full_line: Option<StrRef>,
    pub full_line_text: Option<String>,
    pub prov: Prov,
}

/// `[[place]]`: a destination.
#[derive(Debug, Clone, PartialEq)]
pub struct Place {
    pub key: String,
    pub cell: Option<u32>,
    pub at: At,
    pub rotation: Option<[f32; 4]>,
    pub heading: Option<f32>,
    pub named_by: Vec<u32>,
    pub destination: Option<String>,
    /// The row of like rooms it is one of, and the cell its creature stands in.
    pub row: Option<String>,
    pub holds: Option<u32>,
    pub prov: Prov,
}

/// One row of a pack.
#[derive(Debug, Clone, PartialEq)]
pub enum Row {
    Object(Object),
    Camp(Camp),
    Loot(Loot),
    Keeper(Keeper),
    Store(Store),
    Sale(Sale),
    Quest(Quest),
    /// Boxed: a step is the largest row by far.
    Step(Box<Step>),
    Give(Give),
    Place(Place),
    Talk(Talk),
    /// `[[remove]]`: items a pack takes away, each in the one form the build's report names it.
    Remove(Vec<String>),
    Craft(Craft),
    Butchery(Butchery),
    Tool(Tool),
    Scene(Scene),
    Shard(Shard),
    Weapon(Weapon),
    DeathSpawn(DeathSpawn),
    DeathTreasure(DeathTreasure),
    Pet(Pet),
    PetLimit(PetLimit),
    Route(Route),
    Area(Area),
    Clearing(Clearing),
    TreasurePool(TreasurePool),
    EffectName(EffectName),
    Event(Event),
    Tally(Tally),
    PointsBonus(PointsBonus),
    Popup(Popup),
    EffectRule(EffectRule),
    TreasureValue(TreasureValue),
    TreasureRules(TreasureRules),
    TreasureSort(TreasureSort),
    TreasureWord(TreasureWord),
    TreasureLook(TreasureLook),
    TreasurePotion(TreasurePotion),
    TreasureFamily(TreasureFamily),
    Mine(Mine),
    MapRegion(MapRegion),
    ItemUse(ItemUse),
    Book(Book),
    StartItem(StartItem),
    Interaction(Interaction),
    ObjectStates(ObjectStatesRow),
    Sequence(SequenceRow),
    Lever(LeverRow),
    Offering(OfferingRow),
    Guarded(GuardedRow),
    MapRecipes(MapRecipes),
    Spellbinding(Spellbinding),
    Refining(Refining),
    Recipe(Recipe),
    Combination(Combination),
    Concealing(Concealing),
    NpcTable(NpcTable),
    FriendTable(FriendTable),
    Creature(Creature),
    Defense(DefenseRow),
    Plant(Plant),
    PlantCross(PlantCross),
    Worn(WornRow),
}

/// `[[concealing]]`: what creatures of a class (none: every creature) make of the holder of a
/// concealing effect: they spare it unless it hurts them, or they hate it, coming at it even if they
/// never attack first.
#[derive(Debug, Clone, PartialEq)]
pub struct Concealing {
    /// The effect, by its saved state.
    pub effect: u32,
    /// The creatures' class of behavior.
    pub class: Option<u32>,
    pub regard: yalain_content::creature_rows::Regard,
    pub prov: Prov,
}

/// `[[npc_table]]`: what the world says of the creatures of a behavior table, by its name: whether
/// they heal their kin.
#[derive(Debug, Clone, PartialEq)]
pub struct NpcTable {
    pub name: String,
    pub heals_kin: bool,
    pub prov: Prov,
}

/// `[[friend_table]]`: what the world says of the creatures of a friend table: whether they may
/// attack a player first, and the friend tables whose creatures they take on.
#[derive(Debug, Clone, PartialEq)]
pub struct FriendTable {
    pub table: u32,
    pub attacks_first: Option<bool>,
    pub fights: Vec<u32>,
    pub prov: Prov,
}

/// `[[creature]]`: what the world says of the creatures of a template: its shooting rhythm, the
/// gap between its shots times its shot's time scale and its windup before a missile leaves at the
/// creatures' median time scale, in seconds; the effect it puts on a player that comes within its
/// perception, with its lines for that by race; whether it takes on only the players it hates;
/// whether players may not harm it, with the line a player's attack on it is told.
#[derive(Debug, Clone, PartialEq)]
pub struct Creature {
    pub template: u32,
    pub rhythm: Option<yalain_content::creature_rows::Rhythm>,
    pub marks: Option<yalain_content::creature_rows::Mark>,
    pub only_hated: bool,
    pub players_kept_out: bool,
    pub kept_out_line: Option<StrRef>,
    pub prov: Prov,
}

/// `[[event]]`: a named state of the world, on for its times: what it gates (camps, objects and
/// steps that name it) stands or runs only while it is on, and its effects are on every player in
/// the world while it is.
#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub key: String,
    /// The install's game event it stands for (a value of its game-event enumeration).
    pub game_event: Option<u32>,
    /// Its windows; none for an event a `[[tally]]` alone starts.
    pub when: Vec<yalain_content::events::Window>,
    pub on_players: Vec<u32>,
    pub prov: Prov,
}

/// `[[tally]]`: a count the whole world keeps, which steps and gives fill: when it is full, what
/// it starts then, when it goes back to nothing, and what goes with it.
#[derive(Debug, Clone, PartialEq)]
pub struct Tally {
    pub tally: yalain_content::tallies::Tally,
    pub prov: Prov,
}

/// `[[points_bonus]]`: what an effect does to the kingdom points of the player it is put on, where the
/// install's record gives it no amount: raises them by `add`, never above `up_to`, and never lowers
/// points already above it.
#[derive(Debug, Clone, PartialEq)]
pub struct PointsBonus {
    /// The effect, by its saved state.
    pub effect: u32,
    pub add: i32,
    pub up_to: i32,
    pub prov: Prov,
}

/// `[[popup]]`: the tip a player is shown once, the first time a moment comes for its character:
/// the description of an effect of the install.
#[derive(Debug, Clone, PartialEq)]
pub struct Popup {
    /// The moment it is shown at.
    pub on: yalain_content::popups::Moment,
    /// The effect whose description it shows, by its saved state.
    pub effect: u32,
    pub prov: Prov,
}

/// What a step's or a give's `requires` names: other quests, the `[[event]]`s that must be on, the
/// one kingdom it is for, the acts that must be whole, the craft skills it waits on, whether it
/// waits on an untraining under way and whether it is for heroes alone.
struct Requires {
    quests: Vec<(u32, Need)>,
    events: Vec<String>,
    tallies: Vec<yalain_content::tallies::Gate>,
    kingdom: Option<u32>,
    acts: Vec<u32>,
    crafts: Vec<yalain_content::crafts::CraftGate>,
    untraining: bool,
    heroes: bool,
}

/// `[[effect_rule]]`: what the world says of an effect where its own data says nothing: what a ward
/// keeps off, the share of its player's vitae it takes, the effect an immunity leaves as it ends,
/// the complete reset of skills it grants, the untraining it finishes, the skill it trains, the
/// items it gives.
#[derive(Debug, Clone, PartialEq)]
pub struct EffectRule {
    /// The effect, by its saved state, or by its name for one the install holds no state of.
    pub effect: EffectOf,
    pub ward: Option<yalain_content::effect_rules::Ward>,
    pub vitae_share: Option<f32>,
    /// The vitae it gives back, and to whom.
    pub vitae_restore: Option<yalain_content::effect_rules::VitaeRestore>,
    /// The effect is an immunity under way, which leaves this one (by its saved state) as it ends.
    pub then: Option<u32>,
    /// The kind of skills a complete reset it grants gives back.
    pub reset: Option<yalain_content::effect_rules::Reset>,
    /// It finishes its player's untraining under way at once.
    pub untrains: bool,
    /// The skill it trains its player for good, by its number.
    pub skill: Option<u32>,
    /// The items it gives its player: a template and how many.
    pub gives: Option<yalain_content::effect_rules::Grant>,
    /// What it places in the world at its player.
    pub places: Option<yalain_content::effect_rules::Placing>,
    pub prov: Prov,
}

/// The effect an `[[effect_rule]]` is of: a saved state of the install, or a name the install's
/// templates give an effect it holds no state of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EffectOf {
    State(u32),
    Name(String),
}

impl EffectOf {
    /// Its saved state, when it has one.
    #[must_use]
    pub fn state(&self) -> Option<u32> {
        match self {
            Self::State(s) => Some(*s),
            Self::Name(_) => None,
        }
    }
}

/// `[[mine]]`: what a mining craft skill's nodes play, answer and unearth, and the node a delving
/// stone of the skill conjures.
#[derive(Debug, Clone, PartialEq)]
pub struct Mine {
    pub mine: yalain_content::gathering::Mine,
    pub prov: Prov,
}

/// `[[map_region]]`: the nodes a region's surveyor's maps lead to, each by the name of the effect
/// that conjures it.
#[derive(Debug, Clone, PartialEq)]
pub struct MapRegion {
    pub region: yalain_content::gathering::MapRegion,
    pub prov: Prov,
}

/// `[[map_recipes]]`: a region's surveyor's map recipes, the effect a drawing of one puts on to have
/// them all wait, the effect a find of the region's maps puts on to end it, and how long they wait.
#[derive(Debug, Clone, PartialEq)]
pub struct MapRecipes {
    pub recipes: yalain_content::crafts::MapRecipes,
    pub prov: Prov,
}

/// `[[spellbinding]]`: the totem spellbinding makes of a sort of item, and the word it says it goes
/// on.
#[derive(Debug, Clone, PartialEq)]
pub struct Spellbinding {
    pub totem: yalain_content::crafts::Totem,
    pub prov: Prov,
}

/// `[[refining]]`: what a refining recipe yields, by its script package, the craft skill level at
/// which it yields all it takes, and the percent of it it yields before any skill.
#[derive(Debug, Clone, PartialEq)]
pub struct Refining {
    pub refining: yalain_content::crafts::Refining,
    pub prov: Prov,
}

/// `[[recipe]]`: a recipe the world defines where the install holds none, made from an install
/// recipe: its id (one of the world's recipes' saved states, [`auberean_pack::WORLD_RECIPES`]), the
/// recipe it is made from, its name, description, slots and product.
#[derive(Debug, Clone, PartialEq)]
pub struct Recipe {
    pub recipe: yalain_content::crafts::Recipe,
    pub prov: Prov,
}

/// `[[combination]]`: what a recipe that works out its own product makes of one set of
/// ingredients: the recipe, the templates its slots hold (in any order), and the product, how many.
#[derive(Debug, Clone, PartialEq)]
pub struct Combination {
    pub combination: yalain_content::crafts::Combination,
    pub prov: Prov,
}

/// `[[plant]]`: a species of plant: the plant its seed plants, the seed, its pollen and the pollen's
/// name among the plants' texts, the plant item a harvest gives, how many one seed plants, when they
/// are ripe, whether they need care, and where their lines are.
#[derive(Debug, Clone, PartialEq)]
pub struct Plant {
    pub species: yalain_content::plants::Species,
    pub prov: Prov,
}

/// `[[plant_cross]]`: a plant of one species pollinated with another's pollen gives so many seeds
/// of a third.
#[derive(Debug, Clone, PartialEq)]
pub struct PlantCross {
    pub cross: yalain_content::plants::Cross,
    pub prov: Prov,
}

/// `[[worn]]`: a worn look of a template whose data names none: the races and sexes it fits, and
/// the appearance it gives them.
#[derive(Debug, Clone, PartialEq)]
pub struct WornRow {
    pub worn: yalain_content::worn_table::Worn,
    pub prov: Prov,
}

/// `[[effect_name]]`: the saved state an effect's name stands for, where the install names the
/// effect (an item's use, say) and the server's own resolution of the name (its effect
/// enumeration's value, then the install's one effect of that value, then a package of that name)
/// finds none.
#[derive(Debug, Clone, PartialEq)]
pub struct EffectName {
    pub name: String,
    pub state: u32,
    pub prov: Prov,
}

/// `[[clearing]]`: ground in a landblock kept free of what the land grows: a circle of `radius`
/// meters, or a rectangle `size` meters along its own axes turned by `heading`. The world builder
/// builds it into the client's data; the server has no use for it.
#[derive(Debug, Clone, PartialEq)]
pub struct Clearing {
    pub key: String,
    pub at: At,
    pub heading: Option<f32>,
    pub radius: Option<f32>,
    pub size: Option<[f32; 2]>,
    pub prov: Prov,
}

/// `[[area]]`: the kind of player conflict a landblock allows (a landblock no row names is a
/// peace area).
#[derive(Debug, Clone, PartialEq)]
pub struct Area {
    pub landblock: u16,
    pub conflict: yalain_content::Conflict,
    pub prov: Prov,
}

/// `[[death_spawn]]`: what a creature's death effect makes around its killer, or an item's use effect
/// before its user: creatures, or a portal.
#[derive(Debug, Clone, PartialEq)]
pub struct DeathSpawn {
    /// The death effect, as a creature's template names it; for a creature's own row, its template
    /// as [`yalain_content::DeathSpawn::of_creature`] writes it.
    pub effect: String,
    /// The creature whose death makes it, for a creature's own row.
    pub creature: Option<u32>,
    pub template: u32,
    pub count: u32,
    pub chance: f32,
    pub offset: At,
    /// The place (a `[[place]]` key) each stands at, for a creature's own row.
    pub at: Option<String>,
    /// The place a portal it makes leads to.
    pub to: Option<String>,
    /// How long each stands, in seconds; it stays until killed when left out (a portal until the
    /// server stops).
    pub lasts: Option<f32>,
    /// The kingdom a player must be of to use a portal it makes, as a description's faction names
    /// it.
    pub kingdom: Option<u32>,
    pub prov: Prov,
}

/// `[[death_treasure]]`: a fixed item every creature or chest of a template, or every creature a
/// camp keeps, leaves at a chance of its own, rolled once at each death or opening.
#[derive(Debug, Clone, PartialEq)]
pub struct DeathTreasure {
    pub key: String,
    /// The creature's or chest's template; or
    pub template: Option<u32>,
    /// the camp whose creatures leave it (`key@XXYY`, or `pack:key@XXYY` in another pack).
    pub camp: Option<String>,
    pub item: u32,
    pub chance: f32,
    /// The quest a chest's opener must hold, and where, for the chest to hold it at that opening:
    /// `{ quest, phase }`, `{ quest, underway = true }`, `{ quest, done = true }` or
    /// `{ quest, had = true }`.
    pub requires: Option<(u32, Gate)>,
    pub prov: Prov,
}

/// `[[pet]]`: what a pet effect, or a contract, makes, as its effect's description has it and as
/// the recordings show it fight.
#[derive(Debug, Clone, PartialEq)]
pub struct Pet {
    /// The effect's saved state (or the contract's template).
    pub effect: u32,
    pub template: u32,
    /// The flags of a pet's data as its owner is told them.
    pub flags: u32,
    /// It stays where it is made.
    pub stays: bool,
    /// How many its effect makes at once.
    pub count: u32,
    /// Its greatest health at levels, lowest first; its template's when left out.
    pub health: Vec<(i32, i32)>,
    /// Its armor against a creature's hit at levels, lowest first; left out, a creature's hit on
    /// it goes by its level's share.
    pub armor: Vec<(i32, i32)>,
    /// What a hit takes: this times its level squared. The server's general number when left out.
    pub per_level_squared: Option<f32>,
    pub variance: Option<f32>,
    pub lands: Option<f32>,
    /// Hits a swing on its one target.
    pub swing: Option<u32>,
    /// Its missile lands on every creature this near where it falls.
    pub area: Option<f32>,
    /// Its strikes taunt; whether they harm is its skill's.
    pub taunts: bool,
    /// It ends as its first strike lands.
    pub ends_on_strike: bool,
    /// Its greatest vigor for each level; the pets' own when left out.
    pub vigor_per_level: Option<u32>,
    /// It pays its attacks' vigor, which comes back this much a second; left out, its attacks cost
    /// it nothing.
    pub vigor_per_second: Option<f32>,
    pub outdoors_only: bool,
    /// The attacks it uses, by skill, and the weight of each in its draw among those that reach;
    /// left out, every attack its template names, all alike.
    pub attacks: Vec<(u32, u32)>,
    /// Its corpse after its death, in seconds; the server's general number when left out.
    pub corpse_after: Option<f32>,
    /// It leaves no corpse.
    pub no_corpse: bool,
    pub tends: Option<PetTendsRow>,
    pub hire: Option<PetHireRow>,
    /// The effects an item's use may name, held by no install, that make this pet.
    pub item_effects: Vec<String>,
    pub prov: Prov,
}

/// A `[[pet]]`'s `tends`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PetTendsRow {
    /// Its owner, or its owner's other pets.
    pub pets: bool,
    pub effect: u32,
    pub every: f32,
    pub below: f32,
}

/// A `[[pet]]`'s `hire`: a contract's terms, or those of a pet only items make.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PetHireRow {
    pub class: u32,
    pub seconds: f32,
    pub level: f32,
}

/// `[[pet_limit]]`: what a summoning skill allows its user.
#[derive(Debug, Clone, PartialEq)]
pub struct PetLimit {
    pub skill: u32,
    /// As many as `most` of pets that share a bit of `classes`.
    pub limits: Vec<(u32, u32)>,
    pub prov: Prov,
}

/// `[[weapon]]`: the template of the weapon a creature's template names, where the name is no
/// template of the install's own.
#[derive(Debug, Clone, PartialEq)]
pub struct Weapon {
    /// The name a creature's template gives its weapon.
    pub name: String,
    pub template: u32,
    /// Of the creatures whose template asks two of it, the share given two (the rest one).
    pub two_share: Option<f32>,
    /// The stance it is fought with, where its template sets none.
    pub stance: Option<u32>,
    pub prov: Prov,
}

/// `[[butchery]]`: what a butchery profile yields: its materials and each one's share, and the
/// share of cuts that find nothing.
#[derive(Debug, Clone, PartialEq)]
pub struct Butchery {
    pub profile: u32,
    pub items: Vec<u32>,
    pub weights: Vec<u32>,
    pub empty: u32,
    pub prov: Prov,
}

/// `[[item_use]]`: what an item's use does where its template names no effect for it: the effects
/// it puts on its user (saved state, spellcraft), the behavior it plays where its template names
/// none, and the uses it wears out after where it wasn't made with its own durability.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemUse {
    pub template: u32,
    pub effects: Vec<(u32, f32)>,
    pub behavior: Option<u32>,
    pub wears: Option<u32>,
    pub prov: Prov,
}

/// `[[book]]`: what an item or a person of a template shows when used: a book's pages (`table`)
/// or a picture.
#[derive(Debug, Clone, PartialEq)]
pub struct Book {
    pub template: u32,
    pub shows: yalain_content::item_rows::Shows,
    pub prov: Prov,
}

/// `[[start_item]]`: an item every new character is given as it first enters, and how many.
#[derive(Debug, Clone, PartialEq)]
pub struct StartItem {
    pub template: u32,
    pub count: u32,
    pub prov: Prov,
}

/// `[[interaction]]`: what an item used on another makes, where the item's interaction table lets
/// it be used so and the table's outcome names nothing made: the item used, what it is used on (a
/// template, or a class the table names), and what it gives the user, how many, and instead where
/// the table's interaction the use met is for some characters only. What it gives is one template,
/// or one of several picked by weight (`gives_one_of`), each with its weight (1 for `gives`).
#[derive(Debug, Clone, PartialEq)]
pub struct Interaction {
    pub key: String,
    pub item: u32,
    pub on: yalain_content::item_rows::On,
    pub gives: Vec<(u32, u32)>,
    /// None: `gives`.
    pub gated_gives: Vec<(u32, u32)>,
    pub count: u32,
    /// What it makes at the object it was used on.
    pub places: Vec<yalain_content::effect_rules::Placing>,
    pub prov: Prov,
}

/// `[[object_states]]`: the states the world's objects of some templates hold, each object's first,
/// what items given change and when an object goes back to its first.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectStatesRow {
    pub states: yalain_content::item_rows::ObjectStates,
    pub prov: Prov,
}

/// `[[sequence]]`: uses in an order judged against the used objects' states, the quest whose phase
/// counts them, what the object used says at a right use and a wrong one, and the place its last
/// right use takes the player to (`key`, or `pack:key` in another pack).
#[derive(Debug, Clone, PartialEq)]
pub struct SequenceRow {
    pub sequence: yalain_content::item_rows::Sequence,
    /// The placed object each use names (`key@XXYY`), where it names one in place of a template.
    pub objects: Vec<Option<String>>,
    pub to: Option<String>,
    pub prov: Prov,
}

/// `[[lever]]`: placed objects whose pulls open doors: its levers and the doors they open (each
/// `key@XXYY`, or `pack:key@XXYY` in another pack), how long the doors stay open (until a use closes
/// them when left out) and how long a pull waits for the others (for as long as it takes when left
/// out).
#[derive(Debug, Clone, PartialEq)]
pub struct LeverRow {
    pub key: String,
    pub levers: Vec<String>,
    pub opens: Vec<String>,
    pub open_for: Option<f64>,
    pub within: Option<f64>,
    pub prov: Prov,
}

/// `[[offering]]`: a placed object that takes a set of items: the object (`key@XXYY`, or
/// `pack:key@XXYY`), each item with the placed object that stands while it is held, the placed
/// objects that stand once it holds the whole set, and for how many seconds.
#[derive(Debug, Clone, PartialEq)]
pub struct OfferingRow {
    pub key: String,
    pub object: String,
    pub takes: Vec<(u32, Option<String>)>,
    pub opens: Vec<String>,
    pub lasts: u32,
    pub prov: Prov,
}

/// `[[defense]]`: a device creatures come to attack while it stands, and what it leaves once it has
/// stood long enough.
#[derive(Debug, Clone, PartialEq)]
pub struct DefenseRow {
    pub defense: yalain_content::defenses::Defense,
    pub prov: Prov,
}

/// `[[guarded]]`: placed objects usable only while every creature of a camp is dead: the camp
/// (`key@XXYY`, or `pack:key@XXYY`), the objects (named so too), the line an object says to a use
/// it refuses, whether the objects stand only while the camp's creatures are dead, and how long a
/// use takes an object from the world.
#[derive(Debug, Clone, PartialEq)]
pub struct GuardedRow {
    pub key: String,
    pub camp: String,
    pub objects: Vec<String>,
    pub refused: Option<StrRef>,
    pub hidden: bool,
    pub gone_for: Option<f64>,
    pub prov: Prov,
}

/// `[[tool]]`: what a gathering tool yields when it was not made (a made one's yield is its own).
#[derive(Debug, Clone, PartialEq)]
pub struct Tool {
    pub template: u32,
    pub tool_yield: u32,
    pub prov: Prov,
}

/// A step of a scene: how long, its picture, music and voice, its subtitle, the tracks it defines.
#[derive(Debug, Clone, PartialEq)]
pub struct Keyframe {
    pub seconds: f64,
    pub picture: Option<u32>,
    pub music: Option<u32>,
    pub voice: Option<u32>,
    pub subtitle: Option<StrRef>,
    pub tracks: u32,
}

/// `[[scene]]`: a scene of the story: its act and number in it, its name, whether hidden and
/// playable, its steps; its place among the scenes every new character holds from the start.
#[derive(Debug, Clone, PartialEq)]
pub struct Scene {
    pub id: u32,
    pub act: u32,
    pub number: u32,
    pub name: StrRef,
    pub hidden: bool,
    pub playable: bool,
    pub keyframes: Vec<Keyframe>,
    pub at_start: Option<u32>,
    pub prov: Prov,
}

/// `[[shard]]`: the scene a story shard bestows, by the shard's world id.
#[derive(Debug, Clone, PartialEq)]
pub struct Shard {
    pub object: u64,
    pub scene: u32,
    /// The quest it completes for the player as it comes out, if any.
    pub quest: Option<u32>,
    pub prov: Prov,
}

/// `[[craft]]`: a craft skill or a recipe a new character starts with.
#[derive(Debug, Clone, PartialEq)]
pub struct Craft {
    /// A craft skill (its saved state), or a recipe.
    pub skill: Option<u32>,
    pub recipe: Option<u32>,
    pub prov: Prov,
}

impl Craft {
    /// What it names.
    #[must_use]
    pub fn id(&self) -> u32 {
        self.skill.or(self.recipe).unwrap_or(0)
    }
}

impl Row {
    /// The record kind's name, as its table is written.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Object(_) => "object",
            Self::Camp(_) => "camp",
            Self::Loot(_) => "loot",
            Self::Keeper(_) => "keeper",
            Self::Store(_) => "store",
            Self::Sale(_) => "sale",
            Self::Quest(_) => "quest",
            Self::Step(_) => "step",
            Self::Give(_) => "give",
            Self::Place(_) => "place",
            Self::Talk(_) => "talk",
            Self::Remove(_) => "remove",
            Self::Craft(_) => "craft",
            Self::Butchery(_) => "butchery",
            Self::Tool(_) => "tool",
            Self::Scene(_) => "scene",
            Self::Shard(_) => "shard",
            Self::Weapon(_) => "weapon",
            Self::DeathSpawn(_) => "death_spawn",
            Self::DeathTreasure(_) => "death_treasure",
            Self::Pet(_) => "pet",
            Self::PetLimit(_) => "pet_limit",
            Self::Route(_) => "route",
            Self::Area(_) => "area",
            Self::Clearing(_) => "clearing",
            Self::TreasurePool(_) => "treasure_pool",
            Self::EffectName(_) => "effect_name",
            Self::Event(_) => "event",
            Self::Tally(_) => "tally",
            Self::PointsBonus(_) => "points_bonus",
            Self::Popup(_) => "popup",
            Self::EffectRule(_) => "effect_rule",
            Self::TreasureValue(_) => "treasure_value",
            Self::TreasureRules(_) => "treasure_rules",
            Self::TreasureSort(_) => "treasure_sort",
            Self::TreasureWord(_) => "treasure_word",
            Self::TreasureLook(_) => "treasure_look",
            Self::TreasurePotion(_) => "treasure_potion",
            Self::TreasureFamily(_) => "treasure_family",
            Self::Mine(_) => "mine",
            Self::MapRegion(_) => "map_region",
            Self::ItemUse(_) => "item_use",
            Self::Book(_) => "book",
            Self::StartItem(_) => "start_item",
            Self::Interaction(_) => "interaction",
            Self::ObjectStates(_) => "object_states",
            Self::Sequence(_) => "sequence",
            Self::Lever(_) => "lever",
            Self::Offering(_) => "offering",
            Self::Guarded(_) => "guarded",
            Self::MapRecipes(_) => "map_recipes",
            Self::Spellbinding(_) => "spellbinding",
            Self::Refining(_) => "refining",
            Self::Recipe(_) => "recipe",
            Self::Combination(_) => "combination",
            Self::Concealing(_) => "concealing",
            Self::NpcTable(_) => "npc_table",
            Self::FriendTable(_) => "friend_table",
            Self::Creature(_) => "creature",
            Self::Defense(_) => "defense",
            Self::Plant(_) => "plant",
            Self::PlantCross(_) => "plant_cross",
            Self::Worn(_) => "worn",
        }
    }

    /// The row's provenance.
    #[must_use]
    pub const fn prov(&self) -> Option<&Prov> {
        match self {
            Self::Object(r) => Some(&r.prov),
            Self::Camp(r) => Some(&r.prov),
            Self::Loot(r) => Some(&r.prov),
            Self::Keeper(r) => Some(&r.prov),
            Self::Store(r) => Some(&r.prov),
            Self::Sale(r) => Some(&r.prov),
            Self::Quest(r) => Some(&r.prov),
            Self::Step(r) => Some(&r.prov),
            Self::Give(r) => Some(&r.prov),
            Self::Place(r) => Some(&r.prov),
            Self::Talk(r) => Some(&r.prov),
            Self::Craft(r) => Some(&r.prov),
            Self::Butchery(r) => Some(&r.prov),
            Self::Tool(r) => Some(&r.prov),
            Self::Scene(r) => Some(&r.prov),
            Self::Shard(r) => Some(&r.prov),
            Self::Weapon(r) => Some(&r.prov),
            Self::DeathSpawn(r) => Some(&r.prov),
            Self::DeathTreasure(r) => Some(&r.prov),
            Self::Pet(r) => Some(&r.prov),
            Self::PetLimit(r) => Some(&r.prov),
            Self::Route(r) => Some(&r.prov),
            Self::Area(r) => Some(&r.prov),
            Self::Clearing(r) => Some(&r.prov),
            Self::TreasurePool(r) => Some(&r.prov),
            Self::EffectName(r) => Some(&r.prov),
            Self::Event(r) => Some(&r.prov),
            Self::Tally(r) => Some(&r.prov),
            Self::PointsBonus(r) => Some(&r.prov),
            Self::Popup(r) => Some(&r.prov),
            Self::EffectRule(r) => Some(&r.prov),
            Self::TreasureValue(r) => Some(&r.prov),
            Self::TreasureRules(r) => Some(&r.prov),
            Self::TreasureSort(r) => Some(&r.prov),
            Self::TreasureWord(r) => Some(&r.prov),
            Self::TreasureLook(r) => Some(&r.prov),
            Self::TreasurePotion(r) => Some(&r.prov),
            Self::TreasureFamily(r) => Some(&r.prov),
            Self::Mine(r) => Some(&r.prov),
            Self::MapRegion(r) => Some(&r.prov),
            Self::ItemUse(r) => Some(&r.prov),
            Self::Book(r) => Some(&r.prov),
            Self::StartItem(r) => Some(&r.prov),
            Self::Interaction(r) => Some(&r.prov),
            Self::ObjectStates(r) => Some(&r.prov),
            Self::Sequence(r) => Some(&r.prov),
            Self::Lever(r) => Some(&r.prov),
            Self::Offering(r) => Some(&r.prov),
            Self::Guarded(r) => Some(&r.prov),
            Self::MapRecipes(r) => Some(&r.prov),
            Self::Spellbinding(r) => Some(&r.prov),
            Self::Refining(r) => Some(&r.prov),
            Self::Recipe(r) => Some(&r.prov),
            Self::Combination(r) => Some(&r.prov),
            Self::Concealing(r) => Some(&r.prov),
            Self::NpcTable(r) => Some(&r.prov),
            Self::FriendTable(r) => Some(&r.prov),
            Self::Creature(r) => Some(&r.prov),
            Self::Defense(r) => Some(&r.prov),
            Self::Plant(r) => Some(&r.prov),
            Self::PlantCross(r) => Some(&r.prov),
            Self::Worn(r) => Some(&r.prov),
            Self::Remove(_) => None,
        }
    }
}

/// The record kinds, as their tables are written.
pub const KINDS: [&str; 63] = [
    "object",
    "camp",
    "loot",
    "keeper",
    "store",
    "sale",
    "quest",
    "step",
    "give",
    "place",
    "talk",
    "remove",
    "craft",
    "butchery",
    "tool",
    "scene",
    "shard",
    "weapon",
    "route",
    "area",
    "clearing",
    "death_spawn",
    "pet",
    "pet_limit",
    "death_treasure",
    "treasure_pool",
    "effect_name",
    "event",
    "tally",
    "points_bonus",
    "popup",
    "effect_rule",
    "treasure_value",
    "treasure_rules",
    "treasure_sort",
    "treasure_word",
    "treasure_look",
    "treasure_potion",
    "treasure_family",
    "mine",
    "map_region",
    "item_use",
    "book",
    "start_item",
    "interaction",
    "object_states",
    "sequence",
    "lever",
    "offering",
    "guarded",
    "map_recipes",
    "spellbinding",
    "refining",
    "recipe",
    "combination",
    "concealing",
    "npc_table",
    "friend_table",
    "creature",
    "defense",
    "plant",
    "plant_cross",
    "worn",
];

/// Each record kind's fields.
pub const FIELDS: [(&str, &[&str]); 63] = [
    (
        "object",
        &[
            "key",
            "kind",
            "id",
            "template",
            "cell",
            "at",
            "rotation",
            "heading",
            "scale",
            "name",
            "name_text",
            "to",
            "landings",
            "scene",
            "locked",
            "lock",
            "route",
            "route_start",
            "route_direction",
            "requires",
            "min_level",
            "hero",
            "kingdom",
            "recall",
            "summon",
            "event",
            "prov",
        ],
    ),
    (
        "camp",
        &[
            "key",
            "cell",
            "at",
            "radius",
            "spots",
            "population",
            "regen",
            "spawns",
            "profile",
            "profile_name",
            "parent",
            "movement",
            "wanders",
            "wander_range",
            "wander_chance",
            "homesick",
            "perception",
            "uses_doors",
            "route",
            "route_start",
            "route_direction",
            "name",
            "event",
            "prov",
        ],
    ),
    (
        "loot",
        &[
            "profile",
            "profile_name",
            "counts",
            "items",
            "extras",
            "level",
            "value",
            "draws",
            "rare",
            "prov",
        ],
    ),
    (
        "keeper",
        &[
            "template",
            "store",
            "shop",
            "greeting",
            "greeting_text",
            "prov",
        ],
    ),
    (
        "store",
        &["key", "from", "name_text", "description_text", "prov"],
    ),
    (
        "sale",
        &[
            "store",
            "ordinal",
            "stack",
            "icon",
            "product",
            "cost",
            "currency",
            "name",
            "name_text",
            "prov",
        ],
    ),
    (
        "quest",
        &[
            "id",
            "id_name",
            "state",
            "challenge",
            "duration",
            "timed_phase",
            "retry",
            "cancel_wait",
            "xp",
            "xp_share",
            "xp_share_level",
            "xp_least",
            "xp_most",
            "xp_whole",
            "qp",
            "cancellable",
            "visible",
            "min_level",
            "fails_on_death",
            "fails_on_logout",
            "fails_on_teleport",
            "once",
            "fellows",
            "companion_of",
            "prov",
        ],
    ),
    (
        "step",
        &[
            "key",
            "quest",
            "needs",
            "trigger",
            "tell",
            "tell_text",
            "update",
            "phase",
            "xp",
            "gives",
            "counts",
            "gives_one_of",
            "effects",
            "text",
            "recipes",
            "requires",
            "takes",
            "to",
            "races",
            "effects_last",
            "effects_quiet",
            "fellows",
            "wants",
            "least_level",
            "most_level",
            "xp_share",
            "transforms",
            "chance",
            "adds",
            "escort",
            "to_fellows",
            "prov",
        ],
    ),
    (
        "give",
        &[
            "key",
            "to",
            "item",
            "item_class",
            "wants",
            "gives",
            "counts",
            "gives_one_of",
            "effects",
            "text",
            "effects_quiet",
            "fx",
            "requires",
            "adds",
            "prov",
        ],
    ),
    (
        "place",
        &[
            "key",
            "cell",
            "at",
            "rotation",
            "heading",
            "named_by",
            "destination",
            "row",
            "holds",
            "prov",
        ],
    ),
    (
        "talk",
        &[
            "key",
            "who",
            "object",
            "line",
            "line_name",
            "line_text",
            "role",
            "how",
            "when",
            "quest",
            "update",
            "phase",
            "item",
            "order",
            "requires",
            "fills",
            "full_line",
            "full_line_text",
            "prov",
        ],
    ),
    ("remove", &["items", "item", "prov"]),
    ("craft", &["skill", "recipe", "prov"]),
    (
        "butchery",
        &["profile", "items", "weights", "empty", "prov"],
    ),
    ("tool", &["template", "yield", "prov"]),
    (
        "scene",
        &[
            "id",
            "act",
            "number",
            "name",
            "hidden",
            "playable",
            "keyframes",
            "at_start",
            "prov",
        ],
    ),
    ("shard", &["object", "scene", "quest", "prov"]),
    (
        "weapon",
        &["name", "template", "two_share", "stance", "prov"],
    ),
    (
        "route",
        &["key", "waypoints", "shape", "gait", "speed", "rest", "prov"],
    ),
    ("area", &["landblock", "conflict", "prov"]),
    (
        "clearing",
        &["key", "at", "heading", "radius", "size", "prov"],
    ),
    (
        "death_spawn",
        &[
            "effect", "creature", "template", "count", "chance", "offset", "at", "to", "lasts",
            "kingdom", "prov",
        ],
    ),
    (
        "pet",
        &[
            "effect",
            "template",
            "flags",
            "stays",
            "count",
            "health",
            "armor",
            "per_level_squared",
            "variance",
            "lands",
            "swing",
            "area",
            "taunts",
            "ends_on_strike",
            "vigor_per_level",
            "vigor_per_second",
            "outdoors_only",
            "attacks",
            "corpse_after",
            "no_corpse",
            "tends",
            "hire",
            "item_effects",
            "prov",
        ],
    ),
    ("pet_limit", &["skill", "limits", "prov"]),
    (
        "death_treasure",
        &[
            "key", "template", "camp", "item", "chance", "requires", "prov",
        ],
    ),
    (
        "treasure_pool",
        &["name", "race", "own", "pools", "items", "prov"],
    ),
    ("effect_name", &["name", "state", "prov"]),
    (
        "event",
        &["key", "game_event", "when", "on_players", "prov"],
    ),
    (
        "tally",
        &["key", "full", "starts", "resets", "clears", "prov"],
    ),
    ("points_bonus", &["effect", "add", "up_to", "prov"]),
    ("popup", &["on", "effect", "prov"]),
    (
        "effect_rule",
        &[
            "effect",
            "ward",
            "vitae_share",
            "vitae_restore",
            "then",
            "reset",
            "untrains",
            "skill",
            "gives",
            "count",
            "places",
            "offset",
            "lasts",
            "prov",
        ],
    ),
    ("treasure_value", &["level", "values", "prov"]),
    (
        "treasure_rules",
        &[
            "above_base",
            "above_base_override",
            "most_level",
            "lore",
            "potion_tiers",
            "potion_spellcraft",
            "look",
            "prov",
        ],
    ),
    (
        "treasure_sort",
        &[
            "sort",
            "counts",
            "damage_share",
            "vigor",
            "span",
            "delays",
            "rating_share",
            "rating_spread",
            "slot_chance",
            "prov",
        ],
    ),
    (
        "treasure_word",
        &["sort", "order", "word", "delays", "weights", "prov"],
    ),
    ("treasure_look", &["template", "table", "keys", "prov"]),
    ("treasure_potion", &["kind", "order", "states", "prov"]),
    (
        "treasure_family",
        &[
            "sort", "order", "when", "weight", "by_level", "tiers", "prov",
        ],
    ),
    (
        "mine",
        &[
            "skill",
            "behavior",
            "tools",
            "answer",
            "rich_answer",
            "delving",
            "lodestones",
            "find_chance",
            "finds",
            "prov",
        ],
    ),
    ("map_region", &["region", "nodes", "prov"]),
    (
        "item_use",
        &["template", "effects", "behavior", "wears", "prov"],
    ),
    ("book", &["template", "table", "picture", "prov"]),
    ("start_item", &["template", "count", "prov"]),
    (
        "interaction",
        &[
            "key",
            "item",
            "on",
            "on_class",
            "gives",
            "gives_one_of",
            "gated_gives",
            "gated_gives_one_of",
            "count",
            "places",
            "prov",
        ],
    ),
    (
        "object_states",
        &["key", "templates", "states", "reverts", "changes", "prov"],
    ),
    (
        "sequence",
        &[
            "key",
            "quest",
            "uses",
            "wrong_starts_over",
            "right",
            "wrong",
            "to",
            "prov",
        ],
    ),
    (
        "lever",
        &["key", "levers", "opens", "open_for", "within", "prov"],
    ),
    (
        "offering",
        &["key", "object", "takes", "opens", "lasts", "prov"],
    ),
    (
        "guarded",
        &[
            "key", "camp", "objects", "refused", "hidden", "gone_for", "prov",
        ],
    ),
    (
        "map_recipes",
        &["region", "drawn", "found", "recipes", "wait", "prov"],
    ),
    ("spellbinding", &["sort", "template", "word", "prov"]),
    (
        "refining",
        &["recipe", "product", "full_at", "start", "prov"],
    ),
    (
        "recipe",
        &[
            "recipe",
            "like",
            "name",
            "description",
            "ingredients",
            "product",
            "count",
            "skill",
            "category",
            "prov",
        ],
    ),
    (
        "combination",
        &["key", "recipe", "items", "product", "count", "prov"],
    ),
    ("concealing", &["effect", "class", "regard", "prov"]),
    ("npc_table", &["name", "heals_kin", "prov"]),
    (
        "friend_table",
        &["table", "attacks_first", "fights", "prov"],
    ),
    (
        "creature",
        &[
            "template",
            "shot_gap",
            "windup",
            "marks",
            "mark_lines",
            "only_hated",
            "players_kept_out",
            "kept_out_line",
            "prov",
        ],
    ),
    (
        "defense",
        &[
            "device",
            "holds",
            "spawns",
            "population",
            "regen",
            "radius",
            "then",
            "then_lasts",
            "says",
            "hurt_line",
            "held_line",
            "prov",
        ],
    ),
    (
        "plant",
        &[
            "plant",
            "seed",
            "pollen",
            "name",
            "harvest",
            "count",
            "ripe",
            "tended",
            "water",
            "fertilizer",
            "garden",
            "texts",
            "prov",
        ],
    ),
    (
        "plant_cross",
        &["plant", "pollen", "seeds", "count", "prov"],
    ),
    (
        "worn",
        &["template", "races", "sexes", "appearance", "prov"],
    ),
];

/// The fields of a `[[treasure_family]]` row's tier.
pub const TIER_FIELDS: [&str; 7] = [
    "least", "state", "puts_on", "chance", "glow", "pair", "named",
];

/// A shared-format error as a problem at its file and line.
fn problem(e: &dereth_pack::Error) -> Problem {
    match e {
        dereth_pack::Error::Source { at, why } => Problem::new(
            &at.file.to_string_lossy().replace('\\', "/"),
            at.line,
            why.clone(),
        ),
        other => Problem::new("world.toml", 0, other.to_string()),
    }
}

/// The line (from 1) of a byte offset in a text.
fn line_of(text: &str, offset: usize) -> usize {
    text.as_bytes()[..offset.min(text.len())]
        .iter()
        .filter(|b| **b == b'\n')
        .count()
        + 1
}

/// A file's TOML, or the problem with it.
fn parse(file: &str, text: &str) -> Result<toml::Table, Problem> {
    text.parse::<toml::Table>().map_err(|e| {
        let line = e.span().map_or(0, |s| line_of(text, s.start));
        Problem::new(file, line, format!("not TOML: {}", e.message()))
    })
}

/// The lines of each `[[kind]]` header of a file, in order.
fn header_lines(text: &str) -> BTreeMap<String, Vec<usize>> {
    let mut out: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (n, line) in text.lines().enumerate() {
        let t = line.trim();
        if let Some(k) = t.strip_prefix("[[").and_then(|r| r.split_once("]]")) {
            out.entry(k.0.trim().to_string()).or_default().push(n + 1);
        }
    }
    out
}

/// The landblock of a `.../server/landblocks/XXYY.toml` path.
#[must_use]
pub fn landblock_of(path: &str) -> Option<u16> {
    let (dir, name) = path.rsplit_once('/')?;
    if !dir.ends_with("server/landblocks") {
        return None;
    }
    let hex = name.strip_suffix(".toml")?;
    (hex.len() == 4 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| u16::from_str_radix(hex, 16).ok())
        .flatten()
}

/// Read a database's files.
///
/// # Errors
/// Every problem found: a file that is not TOML, a world or pack file without its keys, a row
/// that breaks the schema. Rows with a problem are left out of the result's packs, so `Ok` means
/// every row read.
pub fn read(files: &Files) -> Result<Database, Vec<Problem>> {
    match read_partly(files) {
        (Some(db), problems) if problems.is_empty() => Ok(db),
        (_, problems) => Err(problems),
    }
}

/// Read a database's files as far as they read: the database without the rows (or packs) that
/// have a problem, `None` when its world file does not read, and every problem found, sorted.
#[must_use]
pub fn read_partly(files: &Files) -> (Option<Database>, Vec<Problem>) {
    let (read, problems) = read_files(files, read_rows);
    (
        read.map(|(world, packs)| Database { world, packs }),
        problems,
    )
}

/// A database read for its layout alone: its world file, each pack's header, and its rows counted
/// by the table they are written in and by provenance, without reading what a row says. A release
/// packaged from it is the same as one packaged from the database read in full.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Layout {
    pub world: World,
    /// Each pack's header, in load order, with no rows.
    pub packs: Vec<Pack>,
    /// Rows by kind: the table each is written in.
    pub rows: BTreeMap<String, usize>,
    /// Rows by provenance: its source, and a capture's by its confidence too (`capture high`).
    /// Removals are not counted here.
    pub provenance: BTreeMap<String, usize>,
}

impl Database {
    /// The database's layout: its world file, its packs' headers, and its rows counted.
    #[must_use]
    pub fn layout(&self) -> Layout {
        let mut rows: BTreeMap<String, usize> = BTreeMap::new();
        let mut provenance: BTreeMap<String, usize> = BTreeMap::new();
        for l in self.rows() {
            *rows.entry(l.row.kind().to_string()).or_default() += 1;
            if let Row::Remove(_) = l.row {
                continue;
            }
            if let Some(p) = l.row.prov() {
                *provenance.entry(prov_count_key(p)).or_default() += 1;
            }
        }
        Layout {
            world: self.world.clone(),
            packs: self
                .packs
                .iter()
                .map(|p| Pack {
                    rows: Vec::new(),
                    ..p.clone()
                })
                .collect(),
            rows,
            provenance,
        }
    }
}

/// What a row's provenance is counted under: its source, and a capture's confidence with it.
fn prov_count_key(p: &Prov) -> String {
    match (p.source, p.confidence) {
        (Source::Capture, Some(c)) => format!("capture {}", c.word()),
        (f, _) => f.word().to_string(),
    }
}

/// Read a database's files for their layout alone: the world file, the lockfile and each pack's
/// header in the shared grammar, every pack file as TOML of tables of rows, and each row's
/// provenance, where it gives one. A kind or a field this server does not know is no problem: it
/// is counted as written.
///
/// # Errors
/// Every problem found: no world file, a world, lockfile or pack header that does not read, a pack
/// file that is not TOML, a table that is not of rows, a row that is not a table, a provenance that
/// does not read.
pub fn read_layout(files: &Files) -> Result<Layout, Vec<Problem>> {
    let mut rows: BTreeMap<String, usize> = BTreeMap::new();
    let mut provenance: BTreeMap<String, usize> = BTreeMap::new();
    let read = read_files(files, |_, path, text, t, problems| {
        let lines = header_lines(text);
        for (kind, v) in t {
            let Some(items) = v.as_array() else {
                problems.push(Problem::new(
                    path,
                    0,
                    format!("{kind} is not a table of rows ([[{kind}]])"),
                ));
                continue;
            };
            for (i, item) in items.iter().enumerate() {
                let line = lines.get(kind).and_then(|l| l.get(i)).copied().unwrap_or(0);
                let Some(table) = item.as_table() else {
                    problems.push(Problem::new(
                        path,
                        line,
                        format!("a [[{kind}]] row is not a table"),
                    ));
                    continue;
                };
                *rows.entry(kind.clone()).or_default() += 1;
                let Some(prov) = table.get("prov") else {
                    continue;
                };
                match prov.clone().try_into::<Prov>() {
                    Ok(p) if kind != "remove" => {
                        *provenance.entry(prov_count_key(&p)).or_default() += 1;
                    }
                    Ok(_) => {}
                    Err(e) => problems.push(Problem::new(
                        path,
                        line,
                        format!("[[{kind}]] prov: {}", e.message()),
                    )),
                }
            }
        }
    });
    match read {
        (Some((world, packs)), problems) if problems.is_empty() => Ok(Layout {
            world,
            packs,
            rows,
            provenance,
        }),
        (_, problems) => Err(problems),
    }
}

/// Read the world file, the lockfile and each pack's header, and hand every TOML file of each
/// pack's `server/` folder, parsed, to `each`: the world and its packs in load order (`None` when
/// the world file does not read), and every problem found, sorted.
fn read_files(
    files: &Files,
    mut each: impl FnMut(&mut Pack, &str, &str, &toml::Table, &mut Vec<Problem>),
) -> (Option<(World, Vec<Pack>)>, Vec<Problem>) {
    let mut problems = Vec::new();
    let Some(text) = files.get("world.toml") else {
        return (
            None,
            vec![Problem::new(
                "world.toml",
                0,
                "the database has no world.toml",
            )],
        );
    };
    let Some(world) = read_world(text, &mut problems) else {
        return (None, problems);
    };
    let mut world = world;
    if let Some(lock) = files.get("world.lock") {
        match dereth_pack::Lock::<Ac2>::parse(Path::new("world.lock"), lock) {
            Ok(lock) => {
                world.stores = lock
                    .of(auberean_pack::Kind::Store)
                    .map(|(key, id)| (key.to_owned(), id))
                    .collect();
            }
            Err(e) => problems.push(problem(&e)),
        }
    }
    let mut packs = Vec::new();
    for folder in &world.packs {
        let header = format!("{folder}/pack.toml");
        let Some(text) = files.get(&header) else {
            problems.push(Problem::new(
                &header,
                0,
                "the world names a pack without a pack.toml",
            ));
            continue;
        };
        let mut pack = match PackHeader::parse::<Ac2>(Path::new(&header), text) {
            Ok(h) => Pack {
                folder: folder.clone(),
                name: h.name,
                version: h.version,
                game: auberean_pack::GAME.to_string(),
                license: h.license.unwrap_or_default(),
                rows: Vec::new(),
            },
            Err(e) => {
                problems.push(problem(&e));
                continue;
            }
        };
        let prefix = format!("{folder}/server/");
        for (path, text) in files.range(prefix.clone()..) {
            if !path.starts_with(&prefix) {
                break;
            }
            if !std::path::Path::new(path)
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("toml"))
            {
                continue;
            }
            match parse(path, text) {
                Ok(t) => each(&mut pack, path, text, &t, &mut problems),
                Err(p) => problems.push(p),
            }
        }
        packs.push(pack);
    }
    problems.sort();
    (Some((world, packs)), problems)
}

/// The world file, read in the shared grammar: `None` (with the problem) when it does not read.
fn read_world(text: &str, problems: &mut Vec<Problem>) -> Option<World> {
    let path = Path::new("world.toml");
    let w = match dereth_pack::World::<Ac2>::parse(path, text) {
        Ok(w) => w,
        Err(e) => {
            problems.push(problem(&e));
            return None;
        }
    };
    let format_version = dereth_pack::format::check::<Ac2>(path, text).map_or(0, i64::from);
    let mut packs = Vec::new();
    for p in &w.packs {
        let folder = p.to_string_lossy().replace('\\', "/");
        if is_symbol(&folder) {
            packs.push(folder);
        } else {
            problems.push(Problem::new(
                "world.toml",
                0,
                format!("packs: {folder:?} is not a pack folder (lowercase letters, digits and hyphens)"),
            ));
        }
    }
    let names_packs = text
        .parse::<toml::Table>()
        .is_ok_and(|t| t.contains_key("packs"));
    if !names_packs {
        problems.push(Problem::new(
            "world.toml",
            0,
            "packs: the load order is missing",
        ));
    }
    Some(World {
        format_version,
        key: w.key,
        game: auberean_pack::GAME.to_string(),
        era: w.era.name().to_string(),
        packs,
        overrides: w.overrides,
        base: w.base,
        stores: BTreeMap::new(),
    })
}

fn read_rows(
    pack: &mut Pack,
    path: &str,
    text: &str,
    t: &toml::Table,
    problems: &mut Vec<Problem>,
) {
    let lines = header_lines(text);
    let landblock = landblock_of(path);
    for (kind, v) in t {
        let Some(items) = v.as_array() else {
            problems.push(Problem::new(
                path,
                0,
                format!("{kind} is not a table of rows ([[{kind}]])"),
            ));
            continue;
        };
        if !KINDS.contains(&kind.as_str()) {
            let line = lines
                .get(kind)
                .and_then(|l| l.first())
                .copied()
                .unwrap_or(0);
            problems.push(Problem::new(
                path,
                line,
                format!("{kind} is not a record kind"),
            ));
            continue;
        }
        if matches!(kind.as_str(), "object" | "camp" | "clearing") && landblock.is_none() {
            problems.push(Problem::new(
                path,
                0,
                format!("[[{kind}]] rows stand in a landblock: write them in server/landblocks/XXYY.toml"),
            ));
            continue;
        }
        for (i, item) in items.iter().enumerate() {
            let line = lines.get(kind).and_then(|l| l.get(i)).copied().unwrap_or(0);
            let Some(table) = item.as_table() else {
                problems.push(Problem::new(
                    path,
                    line,
                    format!("a [[{kind}]] row is not a table"),
                ));
                continue;
            };
            let mut f = Fields::new(table, path, line, problems);
            let row = read_row(kind, &mut f);
            let ok = f.finish(kind);
            if let (Some(row), true) = (row, ok) {
                pack.rows.push(Located {
                    row,
                    file: path.to_string(),
                    line,
                    landblock,
                    pack: pack.name.clone(),
                });
            }
        }
    }
}

/// The most a clearing's `radius` or `size` may be, in meters: a block and a half.
const CLEARING_MOST: f32 = 240.0;

fn read_row(kind: &str, f: &mut Fields<'_>) -> Option<Row> {
    Some(match kind {
        "clearing" => {
            let key = f.req_symbol("key")?;
            let at = f.req_at("at")?;
            if !(0.0..160.0).contains(&at.x) || !(0.0..160.0).contains(&at.y) {
                f.bad(
                    "at",
                    "x and y are meters in the landblock, from 0 up to 160",
                );
                return None;
            }
            let heading = f.float("heading").map(to_f32);
            let radius = f.float("radius").map(to_f32);
            let size = match f.t.contains_key("size") {
                true => {
                    let v = f.req_floats("size")?;
                    let [x, y] = v[..] else {
                        f.bad("size", "two numbers, [x, y] in meters");
                        return None;
                    };
                    Some([x, y])
                }
                false => None,
            };
            let fits = |n: f32| n > 0.0 && n <= CLEARING_MOST;
            let good = match (radius, size) {
                (Some(r), None) => fits(r),
                (None, Some([x, y])) => fits(x) && fits(y),
                _ => false,
            };
            if !good {
                f.bad(
                    "radius",
                    "a clearing is a radius or a size of two numbers, not both, above 0 and at most 240 meters",
                );
                return None;
            }
            Row::Clearing(Clearing {
                key,
                at,
                heading,
                radius,
                size,
                prov: f.req_prov()?,
            })
        }
        "object" => {
            let route_backward = f.route_direction()?;
            let kind = match f.req_str("kind")?.as_str() {
                "npc" => ObjectKind::Npc,
                "static" => ObjectKind::Static,
                other => {
                    f.bad("kind", &format!("{other:?} is not npc or static"));
                    return None;
                }
            };
            Row::Object(Object {
                key: f.req_symbol("key")?,
                kind,
                id: f.id64("id"),
                template: f.req_id("template")?,
                cell: f.id("cell"),
                at: f.req_at("at")?,
                rotation: f.quat("rotation"),
                heading: f.float("heading").map(to_f32),
                scale: f.float("scale").map(to_f32),
                name: f.strref("name"),
                name_text: f.text("name_text"),
                to: f.symbol("to"),
                landings: f.landings("landings"),
                scene: f.id("scene"),
                locked: f.bool("locked").unwrap_or(false),
                lock: f.id("lock"),
                route: f.symbol("route"),
                route_start: f.u32("route_start").unwrap_or(0),
                route_backward,
                requires: f.gate("requires"),
                min_level: f.int("min_level").and_then(|v| i32::try_from(v).ok()),
                hero: f.bool("hero").unwrap_or(false),
                kingdom: f.int("kingdom").and_then(|v| u32::try_from(v).ok()),
                recall: f.bool("recall").unwrap_or(false),
                summon: f.bool("summon").unwrap_or(false),
                event: f.symbol("event"),
                prov: f.req_prov()?,
            })
        }
        "camp" => {
            let moves = f.movement()?;
            let route_backward = f.route_direction()?;
            Row::Camp(Camp {
                key: f.req_symbol("key")?,
                cell: f.id("cell"),
                at: f.req_at("at")?,
                radius: f.float("radius").map_or(0.0, to_f32),
                spots: f.spots("spots"),
                population: f.req_u32("population")?,
                regen: f.req_float("regen")?,
                spawns: f.weighted("spawns"),
                profile: f.u32("profile"),
                profile_name: f.ident("profile_name"),
                parent: f.symbol("parent"),
                moves,
                route: f.symbol("route"),
                route_start: f.u32("route_start").unwrap_or(0),
                route_backward,
                name: f.strref("name"),
                event: f.symbol("event"),
                prov: f.req_prov()?,
            })
        }
        "loot" => Row::Loot(Loot {
            profile: f.req_u32("profile")?,
            profile_name: f.ident("profile_name"),
            counts: f.req_floats("counts")?,
            items: f.weighted("items"),
            extras: f.chances("extras"),
            level: f.int("level").and_then(|v| i32::try_from(v).ok()),
            value: f.ids("value"),
            draws: f.pool_entries("draws", "pool"),
            rare: f.rare("rare"),
            prov: f.req_prov()?,
        }),
        "keeper" => Row::Keeper(Keeper {
            template: f.req_id("template")?,
            store: f.id("store"),
            shop: f.key("shop"),
            greeting: f.strref("greeting"),
            greeting_text: f.text("greeting_text"),
            prov: f.req_prov()?,
        }),
        "store" => Row::Store(Store {
            key: f.req_symbol("key")?,
            from: f.req_id("from")?,
            name_text: match f.get("name_text") {
                Some(_) => f.text("name_text")?,
                None => return f.missing("name_text"),
            },
            description_text: f.text("description_text"),
            prov: f.req_prov()?,
        }),
        "sale" => Row::Sale(Sale {
            store: f.req_id("store")?,
            ordinal: f.req_u32("ordinal")?,
            stack: f.u32("stack"),
            icon: f.icon("icon"),
            product: f.id("product"),
            cost: f.float("cost").map(to_f32),
            currency: f.id("currency"),
            name: f.strref("name"),
            name_text: f.text("name_text"),
            prov: f.req_prov()?,
        }),
        "quest" => Row::Quest(Quest {
            id: f.req_id("id")?,
            id_name: f.ident("id_name"),
            state: f.id("state"),
            challenge: f.int("challenge").and_then(|v| i32::try_from(v).ok()),
            duration: f.float("duration"),
            timed_phase: f.int("timed_phase").and_then(|v| u32::try_from(v).ok()),
            retry: f.float("retry"),
            cancel_wait: f.seconds("cancel_wait"),
            xp: f.int("xp").and_then(|v| u64::try_from(v).ok()),
            xp_share: f.float("xp_share"),
            xp_share_level: f.int("xp_share_level").and_then(|v| i32::try_from(v).ok()),
            xp_least: f.int("xp_least").and_then(|v| u64::try_from(v).ok()),
            xp_most: f.int("xp_most").and_then(|v| u64::try_from(v).ok()),
            xp_whole: f.bool("xp_whole"),
            qp: f.int("qp").and_then(|v| u32::try_from(v).ok()),
            cancellable: f.bool("cancellable"),
            visible: f.bool("visible"),
            min_level: f.int("min_level").and_then(|v| i32::try_from(v).ok()),
            fails_on_death: f.bool("fails_on_death"),
            fails_on_logout: f.bool("fails_on_logout"),
            fails_on_teleport: f.bool("fails_on_teleport"),
            once: f.bool("once"),
            fellows: f.bool("fellows"),
            companion_of: f.id("companion_of"),
            prov: f.req_prov()?,
        }),
        "step" => {
            let trigger = f.trigger("trigger")?;
            let update = match f.req_str("update")?.as_str() {
                "bestow" => Update::Bestow,
                "advance" => Update::Advance,
                "complete" => Update::Complete,
                "fail" => Update::Fail,
                "none" => Update::Nothing,
                other => {
                    f.bad(
                        "update",
                        &format!("{other:?} is not bestow, advance, complete, fail or none"),
                    );
                    return None;
                }
            };
            let Requires {
                quests: requires,
                events,
                tallies,
                kingdom,
                acts,
                crafts,
                untraining,
                heroes,
            } = f.requires_and_events("requires", true);
            Row::Step(Box::new(Step {
                key: f.req_symbol("key")?,
                quest: f.req_id("quest")?,
                needs: f.req_u32("needs")?,
                trigger,
                tell: f.strref("tell"),
                tell_text: f.text("tell_text"),
                update,
                phase: f.u32("phase"),
                xp: f.int("xp").and_then(|v| u64::try_from(v).ok()),
                gives: f.ids("gives"),
                counts: f.ids("counts"),
                gives_one_of: f.one_of("gives_one_of"),
                effects: f.ids("effects"),
                text: f.strref("text"),
                recipes: f.ids("recipes"),
                requires,
                events,
                tallies,
                kingdom,
                acts,
                crafts,
                untraining,
                heroes,
                adds: f.adds().map(Box::new),
                takes: f.ids("takes"),
                to: f.symbol("to"),
                races: f.ids("races"),
                effects_last: f.bool("effects_last").unwrap_or(false),
                effects_quiet: f.bool("effects_quiet").unwrap_or(false),
                fellows: f.bool("fellows").unwrap_or(false),
                wants: f.u32("wants"),
                least_level: f.int("least_level").and_then(|v| i32::try_from(v).ok()),
                most_level: f.int("most_level").and_then(|v| i32::try_from(v).ok()),
                xp_share: f.float("xp_share"),
                transforms: match f.get("transforms") {
                    Some(_) => Some(Box::new(f.transformation()?)),
                    None => None,
                },
                chance: f.share("chance"),
                escort: f.escort().map(Box::new),
                to_fellows: f.bool("to_fellows").unwrap_or(false),
                prov: f.req_prov()?,
            }))
        }
        "give" => {
            let Requires {
                quests: requires,
                tallies,
                kingdom,
                crafts,
                untraining,
                heroes,
                ..
            } = f.requires_and_events("requires", false);
            let item = f.id("item");
            let item_class = f.u32("item_class");
            if item.is_some() && item_class.is_some() {
                f.bad("item_class", "one of item or item_class");
                return None;
            }
            Row::Give(Give {
                key: f.req_symbol("key")?,
                to: f.req_id("to")?,
                item,
                item_class,
                wants: f.u32("wants"),
                gives: f.ids("gives"),
                counts: f.ids("counts"),
                gives_one_of: f.one_of("gives_one_of"),
                effects: f.ids("effects"),
                text: f.strref("text"),
                effects_quiet: f.bool("effects_quiet").unwrap_or(false),
                fx: f.id("fx"),
                requires,
                tallies,
                kingdom,
                crafts,
                untraining,
                heroes,
                adds: f.adds(),
                prov: f.req_prov()?,
            })
        }
        "talk" => {
            let how = match f.word("how", How::from_word, "tell, say, emote or broadcast") {
                Some(Some(h)) => h,
                Some(None) => return None,
                None => How::Tell,
            };
            let Some(Some(when)) = f.word(
                "when",
                When::from_word,
                "use, give, attack, death, near or idle",
            ) else {
                return f.missing("when");
            };
            let role = match f.word("role", Role::from_word, "a role of the format") {
                Some(None) => return None,
                r => r.flatten(),
            };
            let update = match f.word(
                "update",
                TalkUpdate::from_word,
                "bestow, advance, complete or fail",
            ) {
                Some(None) => return None,
                u => u.flatten(),
            };
            Row::Talk(Talk {
                key: f.req_symbol("key")?,
                who: f.req_id("who")?,
                object: f.symbol("object"),
                line: f.strref("line"),
                line_name: f.ident("line_name"),
                line_text: f.text("line_text"),
                role,
                how,
                when,
                quest: f.id("quest"),
                update,
                phase: f.u32("phase"),
                item: f.id("item"),
                order: f.u32("order"),
                requires: f.tally_gates("requires"),
                fills: f.key("fills"),
                full_line: f.strref("full_line"),
                full_line_text: f.text("full_line_text"),
                prov: f.req_prov()?,
            })
        }
        "place" => Row::Place(Place {
            key: f.req_symbol("key")?,
            cell: f.id("cell"),
            at: f.req_at("at")?,
            rotation: f.quat("rotation"),
            heading: f.float("heading").map(to_f32),
            named_by: f.ids("named_by"),
            destination: f.ident("destination"),
            row: f.symbol("row"),
            holds: f.id("holds"),
            prov: f.req_prov()?,
        }),
        "craft" => {
            let skill = f.u32("skill");
            let recipe = f.u32("recipe");
            if skill.is_some() == recipe.is_some() {
                f.bad(
                    "skill",
                    "a craft row names a skill or a recipe, one of them",
                );
                return None;
            }
            Row::Craft(Craft {
                skill,
                recipe,
                prov: f.req_prov()?,
            })
        }
        "butchery" => {
            let profile = f.req_id("profile")?;
            let items = f.ids("items");
            let weights = f.ids("weights");
            if items.is_empty() || items.len() != weights.len() {
                f.bad(
                    "weights",
                    "a butchery row names its items and a weight for each",
                );
                return None;
            }
            Row::Butchery(Butchery {
                profile,
                items,
                weights,
                empty: f.u32("empty").unwrap_or(0),
                prov: f.req_prov()?,
            })
        }
        "item_use" => {
            let template = f.req_id("template")?;
            let effects = f.req(
                "effects",
                "a list of { effect, spellcraft } (spellcraft may be left out: 1)",
                |v| {
                    v.as_array()?
                        .iter()
                        .map(|x| {
                            let t = exactly(x, &["effect", "spellcraft"])
                                .or_else(|| exactly(x, &["effect"]))?;
                            let spellcraft = match t.get("spellcraft") {
                                Some(s) => number_of(s).filter(|s| *s >= 0.0)?,
                                None => 1.0,
                            };
                            Some((as_u32(t.get("effect")?)?, spellcraft))
                        })
                        .collect::<Option<Vec<_>>>()
                },
            )?;
            let ok = !effects.is_empty();
            let effects = f.held("effects", effects, ok, "at least one")?;
            let behavior = f.id("behavior");
            let wears = f.u32("wears");
            if wears == Some(0) {
                f.bad("wears", "above 0");
            }
            Row::ItemUse(ItemUse {
                template,
                effects,
                behavior,
                wears,
                prov: f.req_prov()?,
            })
        }
        "book" => {
            use yalain_content::item_rows::Shows;
            let template = f.req_id("template")?;
            let shows = match (f.id("table"), f.id("picture")) {
                (Some(t), None) => Shows::Book(t),
                (None, Some(p)) => Shows::Picture(p),
                _ => {
                    f.bad("table", "one of table or picture");
                    return None;
                }
            };
            Row::Book(Book {
                template,
                shows,
                prov: f.req_prov()?,
            })
        }
        "start_item" => {
            let template = f.req_id("template")?;
            let count = f.u32("count").unwrap_or(1);
            let count = f.held("count", count, count > 0, "above 0")?;
            Row::StartItem(StartItem {
                template,
                count,
                prov: f.req_prov()?,
            })
        }
        "interaction" => {
            use yalain_content::item_rows::On;
            let key = f.req_symbol("key")?;
            let item = f.req_id("item")?;
            let on = match (f.id("on"), f.u32("on_class")) {
                (Some(t), None) => On::Template(t),
                (None, Some(c)) => On::Class(c),
                _ => {
                    f.bad("on", "one of on or on_class");
                    return None;
                }
            };
            let places = match f.get("places") {
                None => Vec::new(),
                Some(_) => f.placings("places")?,
            };
            let gives = match (f.get("gives").is_some(), f.get("gives_one_of").is_some()) {
                (true, false) => vec![(f.id("gives")?, 1)],
                (false, true) => f.one_of("gives_one_of"),
                (false, false) if !places.is_empty() => Vec::new(),
                _ => {
                    f.bad("gives", "one of gives or gives_one_of, or places");
                    return None;
                }
            };
            let gated_gives = match (
                f.get("gated_gives").is_some(),
                f.get("gated_gives_one_of").is_some(),
            ) {
                (true, false) => vec![(f.id("gated_gives")?, 1)],
                (false, true) => f.one_of("gated_gives_one_of"),
                (false, false) => Vec::new(),
                (true, true) => {
                    f.bad("gated_gives", "one of gated_gives or gated_gives_one_of");
                    return None;
                }
            };
            if gives.is_empty() && places.is_empty() {
                return None;
            }
            let count = f.u32("count").unwrap_or(1);
            let count = f.held("count", count, count > 0, "above 0")?;
            Row::Interaction(Interaction {
                key,
                item,
                on,
                gives,
                gated_gives,
                count,
                places,
                prov: f.req_prov()?,
            })
        }
        "object_states" => {
            use yalain_content::item_rows::{ObjectStates, StateChange};
            let key = f.req_symbol("key")?;
            let name = |v: &toml::Value| v.as_str().filter(|s| is_symbol(s)).map(str::to_string);
            let states = match f.get("states") {
                Some(_) => f.typed(
                    "states",
                    "a list of state names (lowercase letters, digits and hyphens)",
                    |v| v.as_array()?.iter().map(name).collect::<Option<Vec<_>>>(),
                ),
                None => f.missing("states"),
            }?;
            let ok =
                states.len() >= 2 && states.iter().collect::<BTreeSet<_>>().len() == states.len();
            let states = f.held("states", states, ok, "two or more, each named once")?;
            let templates = f.list_of(
                "templates",
                "a list of { template, start }: a template whose objects hold a state, and their first state",
                &["template", "start"],
                |t| Some((as_u32(t.get("template")?)?, name(t.get("start")?)?)),
            );
            let ok = !templates.is_empty() && templates.iter().all(|(_, s)| states.contains(s));
            let templates = f.held(
                "templates",
                templates,
                ok,
                "at least one, each starting in one of the states",
            )?;
            let reverts = f.float("reverts");
            let reverts = f.held(
                "reverts",
                reverts,
                reverts.is_none_or(|r| r > 0.0),
                "above 0",
            )?;
            let changes = f.list_of(
                "changes",
                "a list of { item, from, to }: an item given an object in one state puts it in another",
                &["item", "from", "to"],
                |t| {
                    Some(StateChange {
                        item: as_u32(t.get("item")?)?,
                        from: name(t.get("from")?)?,
                        to: name(t.get("to")?)?,
                    })
                },
            );
            let named = changes
                .iter()
                .all(|c| states.contains(&c.from) && states.contains(&c.to));
            let once = changes
                .iter()
                .map(|c| (c.item, &c.from))
                .collect::<BTreeSet<_>>()
                .len()
                == changes.len();
            let changes = f.held(
                "changes",
                changes,
                named && once,
                "each from and to one of the states, an item and a state changed once",
            )?;
            Row::ObjectStates(ObjectStatesRow {
                states: ObjectStates {
                    key,
                    templates,
                    states,
                    reverts,
                    changes,
                },
                prov: f.req_prov()?,
            })
        }
        "sequence" => {
            let key = f.req_symbol("key")?;
            let quest = f.req_id("quest")?;
            let uses = f.list_of(
                "uses",
                "a list of { use, state } or { object, state }: the template used, or the placed object (key@XXYY), and the state its object must be in (any when left out)",
                &["use", "object", "state"],
                |t| {
                    let state = match t.get("state") {
                        Some(v) => Some(v.as_str().filter(|s| is_symbol(s))?.to_string()),
                        None => None,
                    };
                    match (t.get("use"), t.get("object")) {
                        (Some(u), None) => Some((as_u32(u)?, None, state)),
                        (None, Some(o)) => {
                            let o = o.as_str().filter(|o| is_reference(o))?.to_string();
                            Some((0, Some(o), state))
                        }
                        _ => None,
                    }
                },
            );
            let ok = !uses.is_empty();
            let uses = f.held("uses", uses, ok, "at least one")?;
            let objects: Vec<Option<String>> = uses.iter().map(|u| u.1.clone()).collect();
            let uses = uses.into_iter().map(|(t, _, st)| (t, st)).collect();
            let wrong_starts_over = match f.t.get("wrong_starts_over") {
                Some(_) => f.bool("wrong_starts_over")?,
                None => true,
            };
            let right = f.strref("right").map(|r| (r.string, r.table));
            let wrong = f.strref("wrong").map(|r| (r.string, r.table));
            let to = f.symbol("to");
            Row::Sequence(SequenceRow {
                sequence: yalain_content::item_rows::Sequence {
                    key,
                    quest,
                    uses,
                    objects: Vec::new(),
                    wrong_starts_over,
                    right,
                    wrong,
                    to: None,
                },
                objects,
                to,
                prov: f.req_prov()?,
            })
        }
        "lever" => {
            let key = f.req_symbol("key")?;
            let levers = f.req_references("levers")?;
            let ok = !levers.is_empty();
            let levers = f.held("levers", levers, ok, "at least one")?;
            let opens = f.req_references("opens")?;
            let ok = !opens.is_empty();
            let opens = f.held("opens", opens, ok, "at least one")?;
            let open_for = f.float("open_for");
            let ok = open_for.is_none_or(|t| t > 0.0);
            let open_for = f.held("open_for", open_for, ok, "above 0")?;
            let within = f.float("within");
            let ok = within.is_none_or(|t| t > 0.0);
            let within = f.held("within", within, ok, "above 0")?;
            Row::Lever(LeverRow {
                key,
                levers,
                opens,
                open_for,
                within,
                prov: f.req_prov()?,
            })
        }
        "offering" => {
            let key = f.req_symbol("key")?;
            let object = match f.get("object") {
                Some(_) => f.symbol("object")?,
                None => return f.missing("object"),
            };
            let takes = f.list_of(
                "takes",
                "a list of { item, shows }: an item it takes, and the placed object that stands while it holds it (key@XXYY)",
                &["item", "shows"],
                |t| {
                    let shows = match t.get("shows") {
                        Some(v) => Some(v.as_str().filter(|s| is_reference(s))?.to_string()),
                        None => None,
                    };
                    Some((as_u32(t.get("item")?)?, shows))
                },
            );
            let ok = !takes.is_empty()
                && takes.iter().map(|(i, _)| i).collect::<BTreeSet<_>>().len() == takes.len();
            let takes = f.held("takes", takes, ok, "at least one, each item once")?;
            let opens = f.req_references("opens")?;
            let ok = !opens.is_empty();
            let opens = f.held("opens", opens, ok, "at least one")?;
            let lasts = f.req_u32("lasts")?;
            let ok = lasts > 0;
            let lasts = f.held("lasts", lasts, ok, "above 0")?;
            Row::Offering(OfferingRow {
                key,
                object,
                takes,
                opens,
                lasts,
                prov: f.req_prov()?,
            })
        }
        "guarded" => {
            let key = f.req_symbol("key")?;
            let camp = match f.get("camp") {
                Some(_) => f.symbol("camp")?,
                None => return f.missing("camp"),
            };
            let objects = f.req_references("objects")?;
            let ok = !objects.is_empty();
            let objects = f.held("objects", objects, ok, "at least one")?;
            let refused = f.strref("refused");
            let hidden = f.bool("hidden").unwrap_or(false);
            let gone_for = f.float("gone_for");
            let ok = gone_for.is_none_or(|t| t > 0.0);
            let gone_for = f.held("gone_for", gone_for, ok, "above 0")?;
            Row::Guarded(GuardedRow {
                key,
                camp,
                objects,
                refused,
                hidden,
                gone_for,
                prov: f.req_prov()?,
            })
        }
        "tool" => Row::Tool(Tool {
            template: f.req_id("template")?,
            tool_yield: f.req_u32("yield")?,
            prov: f.req_prov()?,
        }),
        "scene" => Row::Scene(Scene {
            id: f.req_id("id")?,
            act: f.req_id("act")?,
            number: f.u32("number").unwrap_or(0),
            name: f.strref("name")?,
            hidden: f.bool("hidden").unwrap_or(false),
            playable: f.bool("playable").unwrap_or(true),
            keyframes: f.keyframes("keyframes"),
            at_start: f.u32("at_start"),
            prov: f.req_prov()?,
        }),
        "shard" => Row::Shard(Shard {
            object: f.id64("object")?,
            scene: f.req_id("scene")?,
            quest: f.id("quest"),
            prov: f.req_prov()?,
        }),
        "weapon" => {
            if f.get("name").is_none() {
                return f.missing("name");
            }
            let name = f.ident("name")?;
            let two_share = f.float("two_share").map(to_f32);
            if two_share.is_some_and(|x| !(0.0..=1.0).contains(&x)) {
                f.bad("two_share", "a share is a number from 0 to 1");
                return None;
            }
            Row::Weapon(Weapon {
                name,
                template: f.req_id("template")?,
                two_share,
                stance: f.id("stance"),
                prov: f.req_prov()?,
            })
        }
        "tally" => {
            let key = f.req_symbol("key")?;
            let full = f.req_u32("full")?;
            let full = f.held("full", full, full > 0, "above 0")?;
            let starts = f.tally_start()?;
            let resets = f.tally_reset()?;
            Row::Tally(Tally {
                tally: yalain_content::tallies::Tally {
                    key,
                    full,
                    starts,
                    resets,
                    clears: f.ids("clears"),
                },
                prov: f.req_prov()?,
            })
        }
        "event" => {
            let when = match f.get("when") {
                Some(_) => f.windows("when")?,
                None => Vec::new(),
            };
            Row::Event(Event {
                key: f.req_symbol("key")?,
                game_event: f.id("game_event"),
                when,
                on_players: f.ids("on_players"),
                prov: f.req_prov()?,
            })
        }
        "points_bonus" => {
            let effect = f.req_id("effect")?;
            let (add, up_to) = (f.req_int("add")?, f.req_int("up_to")?);
            let Some(add) = i32::try_from(add).ok().filter(|a| *a > 0) else {
                f.bad("add", "an amount above 0, at most 2147483647");
                return None;
            };
            let Ok(up_to) = i32::try_from(up_to) else {
                f.bad("up_to", "kingdom points, from -2147483648 to 2147483647");
                return None;
            };
            Row::PointsBonus(PointsBonus {
                effect,
                add,
                up_to,
                prov: f.req_prov()?,
            })
        }
        "popup" => {
            use yalain_content::popups::Moment;
            let on = match f.word("on", Moment::from_word, "first-entry, level-up or glyph") {
                Some(Some(m)) => m,
                Some(None) => return None,
                None => return f.missing("on"),
            };
            Row::Popup(Popup {
                on,
                effect: f.req_id("effect")?,
                prov: f.req_prov()?,
            })
        }
        "effect_rule" => {
            let effect = match f.get("effect").map(toml::Value::as_str) {
                None => return f.missing("effect"),
                Some(Some(_)) => EffectOf::Name(f.ident("effect")?),
                Some(None) => EffectOf::State(f.id("effect")?),
            };
            let ward = match f.get("ward") {
                Some(_) => Some(f.ward()?),
                None => None,
            };
            let vitae_share = f.share("vitae_share").map(to_f32);
            let vitae_restore = match f.get("vitae_restore") {
                Some(_) => Some(f.vitae_restore()?),
                None => None,
            };
            let then = f.id("then");
            if then.is_some() && then == effect.state() {
                f.bad("then", "an immunity leaves another effect than itself");
                return None;
            }
            let reset = match f.word(
                "reset",
                yalain_content::effect_rules::Reset::from_word,
                "skills or hero-skills",
            ) {
                Some(Some(r)) => Some(r),
                Some(None) => return None,
                None => None,
            };
            let skill = f.typed("skill", "a skill (its number, such as 925)", |v| {
                as_u32(v).filter(|n| *n > 0)
            });
            if f.get("skill").is_some() && skill.is_none() {
                return None;
            }
            let gives = match f.get("gives") {
                Some(_) => {
                    let template = f.req_id("gives")?;
                    let count = f.u32("count").unwrap_or(1);
                    let count = f.held("count", count, count > 0, "above 0")?;
                    Some(yalain_content::effect_rules::Grant { template, count })
                }
                None => {
                    if f.get("count").is_some() {
                        f.bad("count", "a count is of the items a rule gives");
                        return None;
                    }
                    None
                }
            };
            let untrains = f.bool("untrains").unwrap_or(false);
            let places = f.placing()?;
            let others = ward.is_some()
                || vitae_share.is_some()
                || vitae_restore.is_some()
                || then.is_some()
                || reset.is_some()
                || untrains
                || skill.is_some()
                || gives.is_some();
            if !others && places.is_none() {
                f.bad(
                    "ward, vitae_share, vitae_restore, then, reset, untrains, skill, gives, places",
                    "a rule names a ward, a share of vitae, vitae given back, what an immunity leaves, \
                     a reset it grants, the untraining it finishes, a skill it trains, an item it \
                     gives or what it places",
                );
                return None;
            }
            if others && matches!(effect, EffectOf::Name(_)) {
                f.bad(
                    "effect",
                    "an effect named by its name, with no saved state, only places: name its saved state for the rest",
                );
                return None;
            }
            Row::EffectRule(EffectRule {
                effect,
                ward,
                vitae_share,
                vitae_restore,
                then,
                reset,
                untrains,
                skill,
                gives,
                places,
                prov: f.req_prov()?,
            })
        }
        "mine" => {
            let skill = f.req_id("skill")?;
            let behavior = f.req_id("behavior")?;
            let tools = f.ids("tools");
            let answer = f.req_strref("answer")?;
            let rich_answer = f.req_strref("rich_answer")?;
            let delving = f.delving();
            let lodestones = f.lodestones();
            let find_chance = f.float("find_chance").map_or(0.0, to_f32);
            let ok = (0.0..=1.0).contains(&find_chance);
            let find_chance = f.held("find_chance", find_chance, ok, "from 0 to 1")?;
            let finds = f.finds();
            Row::Mine(Mine {
                mine: yalain_content::gathering::Mine {
                    skill,
                    behavior,
                    tools,
                    answer: (answer.string, answer.table),
                    rich_answer: (rich_answer.string, rich_answer.table),
                    delving,
                    lodestones,
                    find_chance,
                    finds,
                },
                prov: f.req_prov()?,
            })
        }
        "map_region" => {
            let region = f.req_symbol("region")?;
            let nodes = f.list_of(
                "nodes",
                "a list of { effect, node }: an effect's name and the node it conjures",
                &["effect", "node"],
                |t| {
                    Some((
                        t.get("effect")?
                            .as_str()
                            .filter(|e| is_ident(e))?
                            .to_string(),
                        as_u32(t.get("node")?)?,
                    ))
                },
            );
            let ok = !nodes.is_empty();
            let nodes = f.held("nodes", nodes, ok, "at least one { effect, node }")?;
            Row::MapRegion(MapRegion {
                region: yalain_content::gathering::MapRegion { region, nodes },
                prov: f.req_prov()?,
            })
        }
        "map_recipes" => {
            let region = f.req_symbol("region")?;
            let drawn = f.req_id("drawn")?;
            let found = f.req_ident("found")?;
            let recipes = f.ids("recipes");
            let ok = !recipes.is_empty();
            let recipes = f.held("recipes", recipes, ok, "at least one recipe")?;
            let wait = f.float("wait");
            let ok = wait.is_none_or(|w| w >= 0.0);
            let wait = f.held("wait", wait, ok, "seconds from 0")?;
            Row::MapRecipes(MapRecipes {
                recipes: yalain_content::crafts::MapRecipes {
                    region,
                    drawn,
                    found,
                    recipes,
                    wait,
                },
                prov: f.req_prov()?,
            })
        }
        "spellbinding" => {
            use yalain_content::treasure_tables::Sort;
            let name = f.req_str("sort")?;
            let sort = Sort::of_name(&name).filter(|s| *s != Sort::Jewelry);
            let sort = f.held("sort", sort, sort.is_some(), "weapon, armor or shield")??;
            Row::Spellbinding(Spellbinding {
                totem: yalain_content::crafts::Totem {
                    sort,
                    template: f.req_id("template")?,
                    word: f.req_id("word")?,
                },
                prov: f.req_prov()?,
            })
        }
        "refining" => {
            let recipe = f.req_ident("recipe")?;
            let product = f.req_id("product")?;
            let full_at = f.u32("full_at");
            let ok = full_at.is_none_or(|l| l >= 2);
            let full_at = f.held("full_at", full_at, ok, "a craft skill level from 2")?;
            let start = f.u32("start");
            let ok = start.is_none_or(|p| p <= 100);
            let start = f.held("start", start, ok, "a percent from 0 to 100")?;
            Row::Refining(Refining {
                refining: yalain_content::crafts::Refining {
                    recipe,
                    product,
                    full_at,
                    start,
                },
                prov: f.req_prov()?,
            })
        }
        "recipe" => {
            use yalain_content::crafts::RecipeIngredient;
            let recipe = f.req_id("recipe")?;
            let (first, last) = auberean_pack::WORLD_RECIPES;
            let recipe = f.held(
                "recipe",
                recipe,
                (first..=last).contains(&recipe),
                &format!(
                    "{recipe:#010X} is outside the world's recipes ({first:#010X} to {last:#010X})"
                ),
            )?;
            let like = f.req_id("like")?;
            let name = f.req_strref("name")?;
            let description = f.strref("description");
            let ingredients = f.list_of(
                "ingredients",
                "a list of { items, count, name }: the templates that go in a slot, how many (1 \
                 when left out) and what the slot says it wants (a string reference, or left out)",
                &["items", "count", "name"],
                |t| {
                    let items = t
                        .get("items")?
                        .as_array()?
                        .iter()
                        .map(as_u32)
                        .collect::<Option<Vec<_>>>()
                        .filter(|i| !i.is_empty())?;
                    let count = match t.get("count") {
                        Some(v) => as_u32(v).filter(|n| *n > 0)?,
                        None => 1,
                    };
                    let name = match t.get("name") {
                        Some(v) => {
                            let r = v.as_table().filter(|r| r.len() == 2)?;
                            Some((as_u32(r.get("string")?)?, as_u32(r.get("table")?)?))
                        }
                        None => None,
                    };
                    Some(RecipeIngredient { items, count, name })
                },
            );
            let ok = !ingredients.is_empty();
            let ingredients = f.held("ingredients", ingredients, ok, "at least one slot")?;
            let product = f.req_id("product")?;
            let count = f.u32("count").unwrap_or(1);
            let count = f.held("count", count, count > 0, "above 0")?;
            Row::Recipe(Recipe {
                recipe: yalain_content::crafts::Recipe {
                    recipe,
                    like,
                    name: (name.string, name.table),
                    description: description.map(|d| (d.string, d.table)),
                    ingredients,
                    product,
                    count,
                    skill: f.id("skill"),
                    category: f.u32("category"),
                },
                prov: f.req_prov()?,
            })
        }
        "combination" => {
            let key = f.req_symbol("key")?;
            let recipe = f.req_id("recipe")?;
            let items = f.ids("items");
            let ok = !items.is_empty();
            let items = f.held("items", items, ok, "at least one template")?;
            let product = f.req_id("product")?;
            let count = f.u32("count").unwrap_or(1);
            let count = f.held("count", count, count > 0, "above 0")?;
            Row::Combination(Combination {
                combination: yalain_content::crafts::Combination {
                    key,
                    recipe,
                    items,
                    product,
                    count,
                },
                prov: f.req_prov()?,
            })
        }
        "concealing" => {
            use yalain_content::creature_rows::Regard;
            let effect = f.req_id("effect")?;
            let class = f.id("class");
            let regard = match f.word("regard", Regard::from_word, "spares or hates") {
                Some(Some(r)) => r,
                Some(None) => return None,
                None => return f.missing("regard"),
            };
            Row::Concealing(Concealing {
                effect,
                class,
                regard,
                prov: f.req_prov()?,
            })
        }
        "npc_table" => {
            let name = f.req_ident("name")?;
            let heals_kin = match f.t.get("heals_kin") {
                Some(_) => f.bool("heals_kin")?,
                None => return f.missing("heals_kin"),
            };
            Row::NpcTable(NpcTable {
                name,
                heals_kin,
                prov: f.req_prov()?,
            })
        }
        "friend_table" => {
            let table = f.req_id("table")?;
            let attacks_first = match f.t.get("attacks_first") {
                Some(_) => Some(f.bool("attacks_first")?),
                None => None,
            };
            let fights = match f.t.get("fights") {
                Some(_) => f.typed("fights", "a list of friend tables (whole numbers)", |v| {
                    v.as_array()?.iter().map(as_u32).collect::<Option<Vec<_>>>()
                })?,
                None => Vec::new(),
            };
            if attacks_first.is_none() && fights.is_empty() {
                f.bad(
                    "attacks_first, fights",
                    "a friend table's row gives attacks_first or the tables it fights",
                );
                return None;
            }
            Row::FriendTable(FriendTable {
                table,
                attacks_first,
                fights,
                prov: f.req_prov()?,
            })
        }
        "creature" => {
            use yalain_content::creature_rows::Rhythm;
            let template = f.req_id("template")?;
            let (gap, windup) = (f.float("shot_gap"), f.float("windup"));
            let seconds = |n: f64| n.is_finite() && n > 0.0;
            let rhythm = match (gap, windup) {
                (Some(g), Some(w)) if seconds(g) && seconds(w) => Some(Rhythm {
                    gap: to_f32(g),
                    windup: to_f32(w),
                }),
                (Some(_), Some(_)) => {
                    f.bad("shot_gap, windup", "a time is a number of seconds above 0");
                    return None;
                }
                (None, None) => None,
                _ => {
                    f.bad(
                        "shot_gap, windup",
                        "a shooter's rhythm gives both its shot_gap and its windup",
                    );
                    return None;
                }
            };
            let marks = f.id("marks");
            let lines = f.mark_lines()?;
            let marks = match (marks, lines) {
                (Some(effect), lines) => {
                    Some(yalain_content::creature_rows::Mark { effect, lines })
                }
                (None, lines) if lines.is_empty() => None,
                (None, _) => {
                    f.bad(
                        "mark_lines",
                        "lines for a mark need the effect it puts on (marks)",
                    );
                    return None;
                }
            };
            let only_hated = match f.t.get("only_hated") {
                Some(_) => f.bool("only_hated")?,
                None => false,
            };
            let players_kept_out = match f.t.get("players_kept_out") {
                Some(_) => f.bool("players_kept_out")?,
                None => false,
            };
            let kept_out_line = match f.t.get("kept_out_line") {
                Some(_) => Some(f.strref("kept_out_line")?),
                None => None,
            };
            if kept_out_line.is_some() && !players_kept_out {
                f.bad(
                    "kept_out_line",
                    "a line for a player's attack needs players kept out (players_kept_out = true)",
                );
                return None;
            }
            if rhythm.is_none() && marks.is_none() && !only_hated && !players_kept_out {
                f.bad(
                    "shot_gap, windup, marks, only_hated, players_kept_out",
                    "a creature's row gives its rhythm, a mark, only_hated or players_kept_out",
                );
                return None;
            }
            Row::Creature(Creature {
                template,
                rhythm,
                marks,
                only_hated,
                players_kept_out,
                kept_out_line,
                prov: f.req_prov()?,
            })
        }
        "defense" => {
            let device = f.req_id("device")?;
            let holds = match f.float("holds") {
                Some(t) => t,
                None => return f.missing("holds"),
            };
            let holds = f.held("holds", holds, holds > 0.0, "above 0")?;
            let spawns = f.weighted("spawns");
            let ok = !spawns.is_empty() && spawns.iter().all(|(_, w)| *w > 0);
            let spawns = f.held("spawns", spawns, ok, "at least one, each weight above 0")?;
            let population = f.req_u32("population")?;
            let population = f.held("population", population, population > 0, "at least 1")?;
            let regen = f.float("regen");
            let ok = regen.is_none_or(|t| t > 0.0);
            let regen = f.held("regen", regen, ok, "above 0")?;
            let radius = f.float("radius").unwrap_or(10.0);
            let radius = f.held("radius", radius, radius >= 0.0, "0 or above")?;
            let then = f.id("then");
            let then_lasts = f.u32("then_lasts");
            if then_lasts.is_some() && then.is_none() {
                f.bad(
                    "then_lasts",
                    "a time is of what stands in the device's place",
                );
                return None;
            }
            let ok = then_lasts.is_none_or(|t| t > 0);
            let then_lasts = f.held("then_lasts", then_lasts, ok, "above 0")?;
            let says = f.timed_lines("says")?;
            let ok = says.iter().all(|(at, _)| *at <= holds);
            let says = f.held("says", says, ok, "each at or before holds")?;
            let line = |l: Option<StrRef>| l.map(|l| (l.string, l.table));
            let hurt_line = line(f.strref("hurt_line"));
            let held_line = line(f.strref("held_line"));
            Row::Defense(DefenseRow {
                defense: yalain_content::defenses::Defense {
                    device,
                    holds,
                    spawns,
                    population,
                    regen,
                    radius: to_f32(radius),
                    then,
                    then_lasts,
                    says,
                    hurt_line,
                    held_line,
                },
                prov: f.req_prov()?,
            })
        }
        "plant" => {
            let plant = f.req_id("plant")?;
            let seed = f.req_id("seed")?;
            let pollen = f.id("pollen");
            let name = f.id("name");
            let both = pollen.is_some() == name.is_some();
            let name = f.held("name", name, both, "a pollen and its name, or neither")?;
            let harvest = f.req_id("harvest")?;
            let count = f.u32("count").unwrap_or(1);
            let count = f.held("count", count, count > 0, "above 0")?;
            let ripe = f
                .float("ripe")
                .unwrap_or(yalain_content::plants::RIPE_SECONDS);
            let ripe = f.held("ripe", ripe, ripe > 0.0, "above 0")?;
            let tended = f.bool("tended").unwrap_or(true);
            let water = f.id("water");
            let fertilizer = f.id("fertilizer");
            let garden = f.plant_garden();
            let texts = f.plant_texts();
            let cares = water.is_some()
                || fertilizer.is_some()
                || texts.is_some_and(|t| t.conditions.is_some() || t.stages.is_some());
            if cares && !tended {
                f.bad(
                    "water, fertilizer, texts",
                    "care items and how a plant fares are of one that needs care (tended)",
                );
                return None;
            }
            Row::Plant(Plant {
                species: yalain_content::plants::Species {
                    count,
                    ripe,
                    tended,
                    water: water.unwrap_or(yalain_content::plants::WATERING_CAN),
                    fertilizer: fertilizer.unwrap_or(yalain_content::plants::FERTILIZER),
                    garden,
                    texts,
                    ..yalain_content::plants::Species::new(
                        plant,
                        seed,
                        pollen,
                        harvest,
                        name.unwrap_or(0),
                    )
                },
                prov: f.req_prov()?,
            })
        }
        "plant_cross" => {
            let plant = f.req_id("plant")?;
            let pollen = f.req_id("pollen")?;
            let seeds = f.req_id("seeds")?;
            let count = f.req_u32("count")?;
            let count = f.held("count", count, (1..=3).contains(&count), "1, 2 or 3")?;
            Row::PlantCross(PlantCross {
                cross: yalain_content::plants::Cross {
                    plant,
                    pollen,
                    seeds,
                    count,
                },
                prov: f.req_prov()?,
            })
        }
        "worn" => {
            let template = f.req_id("template")?;
            let races = f.req_id("races")?;
            let races = f.held("races", races, races != 0, "a mask of races, not 0")?;
            let sexes = f.req_id("sexes")?;
            let sexes = f.held("sexes", sexes, sexes != 0, "a mask of sexes, not 0")?;
            Row::Worn(WornRow {
                worn: yalain_content::worn_table::Worn {
                    template,
                    races,
                    sexes,
                    appearance: f.req_id("appearance")?,
                },
                prov: f.req_prov()?,
            })
        }
        "effect_name" => Row::EffectName(EffectName {
            name: f.req_ident("name")?,
            state: f.req_id("state")?,
            prov: f.req_prov()?,
        }),
        "death_treasure" => {
            let key = f.req_symbol("key")?;
            let (template, camp) = (f.id("template"), f.symbol("camp"));
            if template.is_some() == camp.is_some() {
                f.bad(
                    "template, camp",
                    "a death treasure is left by the creatures or chests of a template, or by a camp's creatures: one of them",
                );
                return None;
            }
            let chance = to_f32(f.req_float("chance")?);
            if chance.is_nan() || chance <= 0.0 || chance > 1.0 {
                f.bad("chance", "a chance is a number above 0, at most 1");
                return None;
            }
            let requires = f.gate("requires");
            if requires.is_some() && camp.is_some() {
                f.bad(
                    "requires",
                    "a death treasure for a quest is a chest's: name the chest's template, not a camp",
                );
                return None;
            }
            Row::DeathTreasure(DeathTreasure {
                key,
                template,
                camp,
                item: f.req_id("item")?,
                chance,
                requires,
                prov: f.req_prov()?,
            })
        }
        "death_spawn" => {
            let creature = f.id("creature");
            let effect = match (f.get("effect").is_some(), creature) {
                (true, None) => f.ident("effect")?,
                (false, Some(c)) => yalain_content::DeathSpawn::of_creature(c),
                (true, Some(_)) => {
                    f.bad(
                        "effect, creature",
                        "a row is of a death effect or of a creature, not both",
                    );
                    return None;
                }
                (false, None) => return f.missing("effect"),
            };
            let at = f.symbol("at");
            if at.is_some() && (creature.is_none() || f.get("offset").is_some()) {
                f.bad(
                    "at",
                    "a place is for a creature's own row, in place of an offset",
                );
                return None;
            }
            let offset = match (creature, f.get("offset").is_some()) {
                (Some(_), false) => At {
                    x: 0.0,
                    y: 0.0,
                    z: None,
                },
                _ => f.req_at("offset")?,
            };
            let chance = f.float("chance").map_or(1.0, to_f32);
            if !(0.0..=1.0).contains(&chance) {
                f.bad("chance", "a share is a number from 0 to 1");
                return None;
            }
            let lasts = f.float("lasts").map(to_f32);
            if lasts.is_some_and(|t| t <= 0.0) {
                f.bad("lasts", "a time is a number of seconds above 0");
                return None;
            }
            let kingdom = f.int("kingdom");
            if kingdom.is_some_and(|k| ![1, 2, 4].contains(&k)) {
                f.bad(
                    "kingdom",
                    "a kingdom is 1 (Shadow), 2 (Order) or 4 (Dominion)",
                );
                return None;
            }
            Row::DeathSpawn(DeathSpawn {
                effect,
                creature,
                template: f.req_id("template")?,
                count: f
                    .int("count")
                    .and_then(|v| u32::try_from(v).ok())
                    .unwrap_or(1),
                chance,
                offset,
                at,
                to: f.symbol("to"),
                lasts,
                kingdom: kingdom.and_then(|v| u32::try_from(v).ok()),
                prov: f.req_prov()?,
            })
        }
        "pet" => {
            let share = |v: Option<f64>| v.map(to_f32);
            let (count, swing) = (f.u32("count").unwrap_or(1), f.u32("swing"));
            let lands = share(f.float("lands"));
            if count == 0 || swing == Some(0) {
                f.bad(
                    "count",
                    "a pet makes at least one, and hits at least once a swing",
                );
                return None;
            }
            if lands.is_some_and(|l| !(0.0..=1.0).contains(&l)) {
                f.bad("lands", "a share is a number from 0 to 1");
                return None;
            }
            let health = f.list_of(
                "health",
                "a list of { level, health }, lowest level first",
                &["level", "health"],
                |t| {
                    Some((
                        i32::try_from(t.get("level")?.as_integer()?).ok()?,
                        i32::try_from(t.get("health")?.as_integer()?).ok()?,
                    ))
                },
            );
            if health.windows(2).any(|w| w[0].0 >= w[1].0) {
                f.bad("health", "levels go up");
                return None;
            }
            let armor = f.list_of(
                "armor",
                "a list of { level, armor }, lowest level first",
                &["level", "armor"],
                |t| {
                    Some((
                        i32::try_from(t.get("level")?.as_integer()?).ok()?,
                        i32::try_from(t.get("armor")?.as_integer()?).ok()?,
                    ))
                },
            );
            if armor.windows(2).any(|w| w[0].0 >= w[1].0) {
                f.bad("armor", "levels go up");
                return None;
            }
            if armor.iter().any(|a| a.1 < 0) {
                f.bad("armor", "armor is 0 or more");
                return None;
            }
            let attacks = f.list_of(
                "attacks",
                "a list of { skill, weight }",
                &["skill", "weight"],
                |t| Some((as_u32(t.get("skill")?)?, as_u32(t.get("weight")?)?)),
            );
            let mut skills: Vec<u32> = attacks.iter().map(|a| a.0).collect();
            skills.sort_unstable();
            skills.dedup();
            if skills.len() < attacks.len() || attacks.iter().any(|a| a.1 == 0) {
                f.bad(
                    "attacks",
                    "each skill once, each with a weight of at least 1",
                );
                return None;
            }
            Row::Pet(Pet {
                effect: f.req_id("effect")?,
                template: f.req_id("template")?,
                flags: f.u32("flags").unwrap_or(0),
                stays: f.bool("stays").unwrap_or(false),
                count,
                health,
                armor,
                per_level_squared: share(f.float("per_level_squared")),
                variance: share(f.float("variance")),
                lands,
                swing,
                area: share(f.float("area")),
                taunts: f.bool("taunts").unwrap_or(false),
                ends_on_strike: f.bool("ends_on_strike").unwrap_or(false),
                vigor_per_level: f.u32("vigor_per_level"),
                vigor_per_second: share(f.float("vigor_per_second")),
                outdoors_only: f.bool("outdoors_only").unwrap_or(false),
                attacks,
                corpse_after: share(f.float("corpse_after")),
                no_corpse: f.bool("no_corpse").unwrap_or(false),
                tends: f.pet_tends(),
                hire: f.pet_hire(),
                item_effects: f.strings("item_effects"),
                prov: f.req_prov()?,
            })
        }
        "treasure_pool" => {
            let race = f.ident("race");
            if race
                .as_deref()
                .is_some_and(|r| !POOL_RACES.iter().any(|(n, _)| *n == r))
            {
                f.bad(
                    "race",
                    "a race is human, lugian, tumerok, drudge or empyrean",
                );
                return None;
            }
            Row::TreasurePool(TreasurePool {
                name: f.req_ident("name")?,
                race,
                own: f.float("own").map(to_f32),
                pools: f.pool_entries("pools", "pool"),
                items: f.pool_entries("items", "template"),
                prov: f.req_prov()?,
            })
        }
        "treasure_value" => {
            let level = f.req_u32("level")?;
            let values = f.values("values")?;
            if values.is_empty() {
                f.bad("values", "a list of at least one value");
                return None;
            }
            Row::TreasureValue(TreasureValue {
                level,
                values,
                prov: f.req_prov()?,
            })
        }
        "treasure_rules" => {
            let rules = f.treasure_rules()?;
            Row::TreasureRules(TreasureRules {
                rules,
                prov: f.req_prov()?,
            })
        }
        "treasure_sort" => {
            let (sort, tables) = f.treasure_sort()?;
            Row::TreasureSort(TreasureSort {
                sort,
                tables,
                prov: f.req_prov()?,
            })
        }
        "treasure_word" => {
            use yalain_content::treasure_tables::{Sort, Word};
            let name = f.req_str("sort")?;
            let sort = Sort::of_name(&name).filter(|s| *s != Sort::Jewelry);
            let sort = f.held("sort", sort, sort.is_some(), "weapon, armor or shield")??;
            let order = f.req_u32("order")?;
            let word = f.req_id("word")?;
            let delays = f.req("delays", "[least, most], whole numbers", |v| {
                let a = v.as_array().filter(|a| a.len() == 2)?;
                Some((int_of(&a[0])?, int_of(&a[1])?))
            })?;
            let delays = f.held("delays", delays, delays.0 <= delays.1, "least at most most")?;
            let weights = f.req_floats("weights")?;
            let ok = weights_ok(&weights);
            let weights = f.held("weights", weights, ok, WEIGHTS)?;
            Row::TreasureWord(TreasureWord {
                sort,
                order,
                word: Word {
                    word,
                    delays,
                    weights,
                },
                prov: f.req_prov()?,
            })
        }
        "treasure_look" => {
            let template = f.req_id("template")?;
            let table = f.req_id("table")?;
            let keys = f.req("keys", "a list of { key, most }", |v| {
                v.as_array()?
                    .iter()
                    .map(|k| {
                        let t = exactly(k, &["key", "most"])?;
                        Some((as_u32(t.get("key")?)?, number_of(t.get("most")?)?))
                    })
                    .collect::<Option<Vec<_>>>()
            })?;
            let ok = (1..=3).contains(&keys.len()) && keys.iter().all(|(_, m)| share_ok(*m));
            let keys = f.held(
                "keys",
                keys,
                ok,
                "one to three keys, each with its most from 0 to 1",
            )?;
            Row::TreasureLook(TreasureLook {
                look: yalain_content::treasure_tables::LookRow {
                    template,
                    table,
                    keys,
                },
                prov: f.req_prov()?,
            })
        }
        "treasure_family" => {
            use yalain_content::treasure_effects::{Tier, When};
            use yalain_content::treasure_tables::{Family, Sort};
            let name = f.req_str("sort")?;
            let sort = Sort::of_name(&name);
            let sort = f.held(
                "sort",
                sort,
                sort.is_some(),
                "weapon, armor, shield or jewelry",
            )??;
            let order = f.req_u32("order")?;
            let when = f.req("when", "permanent, equipped or hit", |v| {
                match v.as_str()? {
                    "permanent" => Some(When::Permanent),
                    "equipped" => Some(When::Equipped),
                    "hit" => Some(When::Hit),
                    _ => None,
                }
            })?;
            let weight = f.req("weight", "a number", number_of)?;
            let weight = f.held("weight", weight, weight >= 0.0, "0 or more")?;
            let by_level = f.req("by_level", "true or false", toml::Value::as_bool)?;
            type Read = (Tier, Option<u32>, bool);
            let tiers: Vec<Read> = f.req(
                "tiers",
                "a list of { least, state, chance, glow }, with puts_on, pair and named where given",
                |v| {
                    v.as_array()?
                        .iter()
                        .map(|x| {
                            let t = x.as_table()?;
                            if t.keys().any(|k| !TIER_FIELDS.contains(&k.as_str())) {
                                return None;
                            }
                            let opt_id = |k: &str| match t.get(k) {
                                Some(v) => as_u32(v).map(Some),
                                None => Some(None),
                            };
                            let tier = Tier {
                                least: int_of(t.get("least")?)?,
                                state: as_u32(t.get("state")?)?,
                                puts_on: opt_id("puts_on")?.unwrap_or(0),
                                chance: number_of(t.get("chance")?)?,
                                glow: number_of(t.get("glow")?)?,
                            };
                            let named = match t.get("named") {
                                Some(v) => v.as_bool()?,
                                None => true,
                            };
                            Some((tier, opt_id("pair")?, named))
                        })
                        .collect::<Option<Vec<_>>>()
                },
            )?;
            let ok = !tiers.is_empty()
                && tiers.iter().all(|(t, _, _)| {
                    t.least >= 0 && share_ok(t.chance) && share_ok(t.glow) && t.state != 0
                });
            let tiers = f.held(
                "tiers",
                tiers,
                ok,
                "at least one, least from 0, chance and glow from 0 to 1",
            )?;
            let rising = !by_level || tiers.windows(2).all(|w| w[0].0.least <= w[1].0.least);
            let tiers = f.held(
                "tiers",
                tiers,
                rising,
                "a family by level gives its tiers from the lowest least level",
            )?;
            let puts = when == When::Hit || tiers.iter().all(|(t, _, _)| t.puts_on == 0);
            let tiers = f.held(
                "tiers",
                tiers,
                puts,
                "puts_on only in a family of effects on what an item hits",
            )?;
            Row::TreasureFamily(TreasureFamily {
                sort,
                order,
                pairs: tiers
                    .iter()
                    .filter_map(|(t, pair, _)| pair.map(|p| (t.state, p)))
                    .collect(),
                unnamed: tiers
                    .iter()
                    .filter(|(_, _, named)| !named)
                    .map(|(t, _, _)| t.state)
                    .collect(),
                family: Family {
                    when,
                    weight,
                    by_level,
                    tiers: tiers.into_iter().map(|(t, _, _)| t).collect(),
                },
                prov: f.req_prov()?,
            })
        }
        "treasure_potion" => {
            let kind = f.req_symbol("kind")?;
            let order = f.req_u32("order")?;
            let states = f.ids("states");
            let ok = !states.is_empty();
            let states = f.held("states", states, ok, "a list of saved states, one a tier")?;
            Row::TreasurePotion(TreasurePotion {
                kind,
                order,
                states,
                prov: f.req_prov()?,
            })
        }
        "pet_limit" => {
            let limits = f.list_of(
                "limits",
                "a list of { most, classes }",
                &["most", "classes"],
                |t| Some((as_u32(t.get("most")?)?, as_u32(t.get("classes")?)?)),
            );
            if limits.is_empty() {
                f.bad("limits", "a limit names what it limits");
                return None;
            }
            Row::PetLimit(PetLimit {
                skill: f.req_id("skill")?,
                limits,
                prov: f.req_prov()?,
            })
        }
        "route" => {
            let shape = match f.word("shape", Shape::from_word, "back-and-forth or loop") {
                Some(Some(s)) => s,
                Some(None) => return None,
                None => Shape::default(),
            };
            let gait = match f.word("gait", Gait::from_word, "walk or run") {
                Some(Some(g)) => g,
                Some(None) => return None,
                None => Gait::default(),
            };
            let speed = f.float("speed").map(to_f32);
            if speed.is_some_and(|v| v <= 0.0) {
                f.bad("speed", "meters a second above 0");
                return None;
            }
            let rest = f.float("rest").unwrap_or(0.0);
            if rest < 0.0 {
                f.bad("rest", "a rest is seconds from 0");
                return None;
            }
            if f.get("waypoints").is_none() {
                return f.missing("waypoints");
            }
            let waypoints = f.waypoints("waypoints");
            if waypoints.is_empty() || waypoints.iter().any(|w| w.rest.is_some_and(|r| r < 0.0)) {
                f.bad(
                    "waypoints",
                    "a route has at least one waypoint, and each rest is seconds from 0",
                );
                return None;
            }
            Row::Route(Route {
                key: f.req_symbol("key")?,
                waypoints,
                shape,
                gait,
                speed,
                rest,
                prov: f.req_prov()?,
            })
        }
        "area" => {
            let landblock = f.req_id("landblock")?;
            let Ok(landblock) = u16::try_from(landblock) else {
                f.bad("landblock", "a landblock is 0xXXYY");
                return None;
            };
            let conflict = match f.word(
                "conflict",
                yalain_content::Conflict::from_word,
                "peace, controlled, kingdom-vs-kingdom or free-for-all",
            ) {
                Some(Some(c)) => c,
                Some(None) => return None,
                None => return f.missing("conflict"),
            };
            Row::Area(Area {
                landblock,
                conflict,
                prov: f.req_prov()?,
            })
        }
        "remove" => {
            let mut written = f.strings("items");
            if let Some(one) = f.typed("item", "an item", |v| v.as_str().map(str::to_string)) {
                written.push(one);
            }
            if f.get("prov").is_some() {
                f.req_prov()?;
            }
            if written.is_empty() {
                return f.missing("items");
            }
            let mut items = Vec::new();
            for item in written {
                let at = dereth_pack::Loc::new(Path::new(f.file), f.line);
                let r = Remove { item, prov: None };
                match r.item_name::<Ac2>(&at) {
                    Ok(name) => items.push(name),
                    Err(e) => {
                        let why = match &e {
                            dereth_pack::Error::Source { why, .. } => why.clone(),
                            other => other.to_string(),
                        };
                        f.bad("items", &why);
                        return None;
                    }
                }
            }
            Row::Remove(items)
        }
        _ => return None,
    })
}

#[allow(clippy::cast_possible_truncation)]
fn to_f32(v: f64) -> f32 {
    v as f32
}

/// A symbol: lowercase letters, digits and hyphens, starting with a letter or digit.
#[must_use]
pub fn is_symbol(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .next()
            .is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// An identifier of the install written as text: letters, digits, `_`, `:`, `.` and `-`, at most 80.
#[must_use]
pub fn is_ident(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 80
        && !s.starts_with(['-', ':', '.'])
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b':' | b'.' | b'-'))
}

/// The longest new prose a `*_text` field holds.
pub const TEXT_LIMIT: usize = 200;

/// A row's fields, read by name; what is wrong becomes a problem at the row's line.
struct Fields<'a> {
    t: &'a toml::Table,
    file: &'a str,
    line: usize,
    problems: &'a mut Vec<Problem>,
    bad: bool,
}

impl<'a> Fields<'a> {
    fn new(t: &'a toml::Table, file: &'a str, line: usize, problems: &'a mut Vec<Problem>) -> Self {
        Self {
            t,
            file,
            line,
            problems,
            bad: false,
        }
    }

    fn bad(&mut self, field: &str, what: &str) {
        self.bad = true;
        self.problems.push(Problem::new(
            self.file,
            self.line,
            format!("{field}: {what}"),
        ));
    }

    fn get(&mut self, name: &str) -> Option<&'a toml::Value> {
        self.t.get(name)
    }

    fn missing<T>(&mut self, name: &str) -> Option<T> {
        self.bad(name, "missing");
        None
    }

    /// Every field is one of the kind's, and none was wrong.
    fn finish(self, kind: &str) -> bool {
        let mut ok = !self.bad;
        let fields = FIELDS
            .iter()
            .find(|(k, _)| *k == kind)
            .map_or(&[][..], |(_, f)| *f);
        for k in self.t.keys() {
            if !fields.contains(&k.as_str()) {
                ok = false;
                self.problems.push(Problem::new(
                    self.file,
                    self.line,
                    format!("{k} is not a field of this record kind"),
                ));
            }
        }
        ok
    }

    fn typed<T>(
        &mut self,
        name: &str,
        what: &str,
        conv: impl FnOnce(&toml::Value) -> Option<T>,
    ) -> Option<T> {
        let v = self.get(name)?;
        let r = conv(v);
        if r.is_none() {
            self.bad(name, &format!("{v} is not {what}"));
        }
        r
    }

    fn id(&mut self, name: &str) -> Option<u32> {
        self.typed(name, "a data id (an integer such as 0x47002224)", as_u32)
    }

    fn req_id(&mut self, name: &str) -> Option<u32> {
        match self.t.get(name) {
            Some(_) => self.id(name),
            None => self.missing(name),
        }
    }

    fn id64(&mut self, name: &str) -> Option<u64> {
        self.typed(name, "a world id (an integer)", |v| {
            v.as_integer().and_then(|i| u64::try_from(i).ok())
        })
    }

    fn u32(&mut self, name: &str) -> Option<u32> {
        self.typed(name, "a count (a whole number from 0)", as_u32)
    }

    fn req_u32(&mut self, name: &str) -> Option<u32> {
        match self.t.get(name) {
            Some(_) => self.u32(name),
            None => self.missing(name),
        }
    }

    fn int(&mut self, name: &str) -> Option<i64> {
        self.typed(name, "a whole number", toml::Value::as_integer)
    }

    fn req_int(&mut self, name: &str) -> Option<i64> {
        match self.t.get(name) {
            Some(_) => self.int(name),
            None => self.missing(name),
        }
    }

    fn float(&mut self, name: &str) -> Option<f64> {
        self.typed(name, "a number", as_number)
    }

    fn req_float(&mut self, name: &str) -> Option<f64> {
        match self.t.get(name) {
            Some(_) => self.float(name),
            None => self.missing(name),
        }
    }

    fn bool(&mut self, name: &str) -> Option<bool> {
        self.typed(name, "true or false", toml::Value::as_bool)
    }

    fn req_str(&mut self, name: &str) -> Option<String> {
        match self.t.get(name) {
            Some(_) => self.typed(name, "a string", |v| v.as_str().map(str::to_string)),
            None => self.missing(name),
        }
    }

    fn symbol(&mut self, name: &str) -> Option<String> {
        self.typed(
            name,
            "a key (lowercase letters, digits, hyphens, and @XXYY from another landblock or pack:key from another pack)",
            |v| v.as_str().filter(|s| is_reference(s)).map(str::to_string),
        )
    }

    fn key(&mut self, name: &str) -> Option<String> {
        self.typed(name, "a key (lowercase letters, digits and hyphens)", |v| {
            v.as_str().filter(|s| is_symbol(s)).map(str::to_string)
        })
    }

    fn req_symbol(&mut self, name: &str) -> Option<String> {
        match self.t.get(name) {
            Some(_) => self.typed(name, "a key (lowercase letters, digits and hyphens)", |v| {
                v.as_str().filter(|s| is_symbol(s)).map(str::to_string)
            }),
            None => self.missing(name),
        }
    }

    fn ident(&mut self, name: &str) -> Option<String> {
        self.typed(
            name,
            "an identifier of the install (letters, digits, _ : . -, at most 80)",
            |v| v.as_str().filter(|s| is_ident(s)).map(str::to_string),
        )
    }

    fn text(&mut self, name: &str) -> Option<String> {
        self.typed(name, "text of 1 to 200 characters", |v| {
            v.as_str()
                .filter(|s| !s.is_empty() && s.chars().count() <= TEXT_LIMIT)
                .map(str::to_string)
        })
    }

    /// A list of references to placed objects (`key@XXYY`, or `pack:key@XXYY`); missing is wrong.
    fn req_references(&mut self, name: &str) -> Option<Vec<String>> {
        match self.t.get(name) {
            Some(_) => self.typed(
                name,
                "a list of placed objects (key@XXYY, or pack:key@XXYY from another pack)",
                |v| {
                    v.as_array()?
                        .iter()
                        .map(|x| x.as_str().filter(|s| is_reference(s)).map(str::to_string))
                        .collect::<Option<Vec<_>>>()
                },
            ),
            None => self.missing(name),
        }
    }

    fn strings(&mut self, name: &str) -> Vec<String> {
        self.typed(name, "a list of strings", |v| {
            v.as_array().and_then(|a| {
                a.iter()
                    .map(|x| x.as_str().map(str::to_string))
                    .collect::<Option<Vec<_>>>()
            })
        })
        .unwrap_or_default()
    }

    fn at_of(v: &toml::Value) -> Option<At> {
        let a = v.as_array()?;
        if a.len() != 3 {
            return None;
        }
        let z = match &a[2] {
            toml::Value::String(s) if s == "ground" => None,
            other => Some(to_f32(as_number(other)?)),
        };
        Some(At {
            x: to_f32(as_number(&a[0])?),
            y: to_f32(as_number(&a[1])?),
            z,
        })
    }

    fn req_at(&mut self, name: &str) -> Option<At> {
        match self.t.get(name) {
            Some(_) => self.typed(
                name,
                "a point [x, y, z] in meters (z may be \"ground\")",
                Self::at_of,
            ),
            None => self.missing(name),
        }
    }

    fn quat(&mut self, name: &str) -> Option<[f32; 4]> {
        self.typed(name, "a rotation [w, x, y, z]", |v| {
            let a = v.as_array()?;
            if a.len() != 4 {
                return None;
            }
            let n: Option<Vec<f32>> = a.iter().map(|x| as_number(x).map(to_f32)).collect();
            n.map(|n| [n[0], n[1], n[2], n[3]])
        })
    }

    fn strref(&mut self, name: &str) -> Option<StrRef> {
        self.typed(name, "a string reference { table, string }", |v| {
            let t = v.as_table()?;
            if t.len() != 2 {
                return None;
            }
            Some(StrRef {
                table: as_u32(t.get("table")?)?,
                string: as_u32(t.get("string")?)?,
            })
        })
    }

    fn ids(&mut self, name: &str) -> Vec<u32> {
        self.typed(name, "a list of data ids", |v| {
            v.as_array()?.iter().map(as_u32).collect::<Option<Vec<_>>>()
        })
        .unwrap_or_default()
    }

    fn values(&mut self, name: &str) -> Option<Vec<u32>> {
        match self.t.get(name) {
            Some(_) => self.typed(name, "a list of values (whole numbers from 0)", |v| {
                v.as_array()?.iter().map(as_u32).collect::<Option<Vec<_>>>()
            }),
            None => self.missing(name),
        }
    }

    fn req_floats(&mut self, name: &str) -> Option<Vec<f32>> {
        match self.t.get(name) {
            Some(_) => self.typed(name, "a list of numbers", |v| {
                v.as_array()?
                    .iter()
                    .map(|x| as_number(x).map(to_f32))
                    .collect::<Option<Vec<_>>>()
            }),
            None => self.missing(name),
        }
    }

    fn list_of<T>(
        &mut self,
        name: &str,
        what: &str,
        keys: &[&str],
        one: impl Fn(&toml::Table) -> Option<T>,
    ) -> Vec<T> {
        self.typed(name, what, |v| {
            v.as_array()?
                .iter()
                .map(|x| {
                    let t = x.as_table()?;
                    if t.keys().any(|k| !keys.contains(&k.as_str())) {
                        return None;
                    }
                    one(t)
                })
                .collect::<Option<Vec<_>>>()
        })
        .unwrap_or_default()
    }

    /// One of several templates picked by weight: a list of at least two `{ template, weight }`,
    /// each weight above 0 (none when left out).
    fn one_of(&mut self, name: &str) -> Vec<(u32, u32)> {
        self.typed(
            name,
            "a list of at least two { template, weight }, each weight above 0",
            |v| {
                let a = v.as_array().filter(|a| a.len() >= 2)?;
                a.iter()
                    .map(|x| {
                        let t = x.as_table()?;
                        if t.len() != 2 {
                            return None;
                        }
                        let template = as_u32(t.get("template")?)?;
                        let weight = as_u32(t.get("weight")?).filter(|w| *w > 0)?;
                        Some((template, weight))
                    })
                    .collect::<Option<Vec<_>>>()
            },
        )
        .unwrap_or_default()
    }

    fn weighted(&mut self, name: &str) -> Vec<(u32, u32)> {
        self.list_of(
            name,
            "a list of { template, weight }",
            &["template", "weight"],
            |t| Some((as_u32(t.get("template")?)?, as_u32(t.get("weight")?)?)),
        )
    }

    /// A list of pool entries, each naming `to` (a pool's name or a template), a weight, and the
    /// least and most item level it is drawn at, either left out.
    fn pool_entries(&mut self, name: &str, to: &str) -> Vec<PoolEntry> {
        let level = |t: &toml::Table, k: &str| match t.get(k) {
            None => Some(None),
            Some(v) => v.as_integer().and_then(|i| i32::try_from(i).ok()).map(Some),
        };
        let what =
            format!("a list of {{ {to}, weight, least, most }} (least and most may be left out)");
        self.list_of(name, &what, &[to, "weight", "least", "most"], |t| {
            let (pool, template) = if to == "pool" {
                (
                    Some(t.get("pool")?.as_str().filter(|s| is_ident(s))?.to_string()),
                    None,
                )
            } else {
                (None, Some(as_u32(t.get("template")?)?))
            };
            Some(PoolEntry {
                pool,
                template,
                weight: to_f32(as_number(t.get("weight")?)?),
                least: level(t, "least")?,
                most: level(t, "most")?,
            })
        })
    }

    /// `{ pool, chance }`.
    fn rare(&mut self, name: &str) -> Option<(String, f32)> {
        self.typed(name, "{ pool, chance }", |v| {
            let t = v.as_table()?;
            if t.len() != 2 {
                return None;
            }
            Some((
                t.get("pool")?.as_str().filter(|s| is_ident(s))?.to_string(),
                to_f32(as_number(t.get("chance")?)?),
            ))
        })
    }

    fn req_ident(&mut self, name: &str) -> Option<String> {
        match self.t.get(name) {
            Some(_) => self.ident(name),
            None => self.missing(name),
        }
    }

    fn chances(&mut self, name: &str) -> Vec<(u32, f32)> {
        self.list_of(
            name,
            "a list of { template, chance }",
            &["template", "chance"],
            |t| {
                Some((
                    as_u32(t.get("template")?)?,
                    to_f32(as_number(t.get("chance")?)?),
                ))
            },
        )
    }

    fn spots(&mut self, name: &str) -> Vec<(u32, At)> {
        self.list_of(name, "a list of { cell, at }", &["cell", "at"], |t| {
            Some((as_u32(t.get("cell")?)?, Self::at_of(t.get("at")?)?))
        })
    }

    fn waypoints(&mut self, name: &str) -> Vec<Waypoint> {
        self.list_of(
            name,
            "a list of { cell, at, rest }",
            &["cell", "at", "rest"],
            |t| {
                Some(Waypoint {
                    cell: as_u32(t.get("cell")?)?,
                    at: Self::at_of(t.get("at")?)?,
                    rest: match t.get("rest") {
                        Some(v) => Some(as_number(v)?),
                        None => None,
                    },
                })
            },
        )
    }

    /// A row's `route_direction`: whether it starts the way back (forward when left out); none
    /// when it is neither word.
    fn route_direction(&mut self) -> Option<bool> {
        match self.word(
            "route_direction",
            |w| match w {
                "forward" => Some(false),
                "backward" => Some(true),
                _ => None,
            },
            "forward or backward",
        ) {
            Some(None) => None,
            d => Some(d.flatten().unwrap_or(false)),
        }
    }

    /// A camp's movement settings over its templates': `None` (with a problem) when one is not
    /// of its kind.
    fn movement(&mut self) -> Option<MovementOverrides> {
        let kind = match self.word(
            "movement",
            MovementType::from_word,
            "stationary, turn-only, constrained-to-home, constrained-to-master or free-range",
        ) {
            Some(None) => return None,
            k => k.flatten(),
        };
        let meters = |f: &mut Self, name: &str| -> Option<Option<f32>> {
            match f.float(name).map(to_f32) {
                Some(v) if v < 0.0 => {
                    f.bad(name, "meters from 0");
                    None
                }
                v => Some(v),
            }
        };
        let wander_range = meters(self, "wander_range")?;
        let homesick = meters(self, "homesick")?;
        let perception = meters(self, "perception")?;
        let wander_chance = self.float("wander_chance").map(to_f32);
        if wander_chance.is_some_and(|c| !(0.0..=1.0).contains(&c)) {
            self.bad("wander_chance", "a chance is a number from 0 to 1");
            return None;
        }
        Some(MovementOverrides {
            kind,
            wanders: self.bool("wanders"),
            wander_range,
            wander_chance,
            homesick,
            perception,
            uses_doors: self.bool("uses_doors"),
        })
    }

    fn landings(&mut self, name: &str) -> Vec<Landing> {
        self.list_of(
            name,
            "a list of { cell, at, heading, seen }",
            &["cell", "at", "heading", "seen"],
            |t| {
                Some(Landing {
                    cell: as_u32(t.get("cell")?)?,
                    at: Self::at_of(t.get("at")?)?,
                    heading: match t.get("heading") {
                        Some(v) => Some(to_f32(as_number(v)?)),
                        None => None,
                    },
                    seen: match t.get("seen") {
                        Some(v) => Some(as_u32(v)?),
                        None => None,
                    },
                })
            },
        )
    }

    fn keyframes(&mut self, name: &str) -> Vec<Keyframe> {
        self.list_of(
            name,
            "a list of { seconds, picture, music, voice, subtitle, tracks }",
            &["seconds", "picture", "music", "voice", "subtitle", "tracks"],
            |t| {
                let id = |k: &str| -> Option<Option<u32>> {
                    match t.get(k) {
                        Some(v) => Some(Some(as_u32(v)?)),
                        None => Some(None),
                    }
                };
                let subtitle = match t.get("subtitle") {
                    Some(v) => {
                        let s = v.as_table()?;
                        Some(StrRef {
                            table: as_u32(s.get("table")?)?,
                            string: as_u32(s.get("string")?)?,
                        })
                    }
                    None => None,
                };
                Some(Keyframe {
                    seconds: as_number(t.get("seconds")?)?,
                    picture: id("picture")?,
                    music: id("music")?,
                    voice: id("voice")?,
                    subtitle,
                    tracks: as_u32(t.get("tracks")?)?,
                })
            },
        )
    }

    fn icon(&mut self, name: &str) -> Vec<(u32, u32, u32)> {
        self.list_of(
            name,
            "a list of { layer, picture, color }",
            &["layer", "picture", "color"],
            |t| {
                Some((
                    as_u32(t.get("layer")?)?,
                    as_u32(t.get("picture")?)?,
                    as_u32(t.get("color")?)?,
                ))
            },
        )
    }

    fn trigger(&mut self, name: &str) -> Option<Trigger> {
        if self.t.get(name).is_none() {
            return self.missing(name);
        }
        self.typed(
            name,
            "one of { use = T }, { use = ITEM, on = TARGET }, { give = ITEM, to = TARGET }, { give_class = CLASS, to = TARGET }, { arrive = T }, { arrive = T, from = OBJECT }, { take = T }, { kill = T }, { kill = T, camp = KEY }, { kill = T, within = METERS }, { done = Q }, { given = Q }",
            |v| {
                let t = v.as_table()?;
                if let (2, Some(item), Some(on)) = (t.len(), t.get("use"), t.get("on")) {
                    return Some(Trigger::UseOn(as_u32(item)?, as_u32(on)?));
                }
                if let (2, Some(tpl), Some(from)) = (t.len(), t.get("arrive"), t.get("from")) {
                    let from = from.as_integer().and_then(|i| u64::try_from(i).ok())?;
                    return Some(Trigger::ArriveFrom(as_u32(tpl)?, from));
                }
                if let (2, Some(item), Some(to)) = (t.len(), t.get("give"), t.get("to")) {
                    return Some(Trigger::Give(as_u32(item)?, as_u32(to)?));
                }
                if let (2, Some(class), Some(to)) = (t.len(), t.get("give_class"), t.get("to")) {
                    return Some(Trigger::GiveClass(as_u32(class)?, as_u32(to)?));
                }
                if let (2, Some(tpl), Some(camp)) = (t.len(), t.get("kill"), t.get("camp")) {
                    let camp = camp.as_str().filter(|c| !c.is_empty())?;
                    return Some(Trigger::KillIn(as_u32(tpl)?, camp.to_string()));
                }
                if let (2, Some(tpl), Some(m)) = (t.len(), t.get("kill"), t.get("within")) {
                    let m = as_u32(m).filter(|m| *m > 0)?;
                    return Some(Trigger::KillNear(as_u32(tpl)?, m));
                }
                if t.len() != 1 {
                    return None;
                }
                let (k, v) = t.iter().next()?;
                let tpl = as_u32(v)?;
                match k.as_str() {
                    "use" => Some(Trigger::Use(tpl)),
                    "arrive" => Some(Trigger::Arrive(tpl)),
                    "take" => Some(Trigger::Take(tpl)),
                    "kill" => Some(Trigger::Kill(tpl)),
                    "done" => Some(Trigger::Done(tpl)),
                    "given" => Some(Trigger::Given(tpl)),
                    _ => None,
                }
            },
        )
    }

    /// `{ quest = Q, done = true }`, `{ quest = Q, underway = true }` or `{ quest = Q, phase = N }`.
    fn gate(&mut self, name: &str) -> Option<(u32, Gate)> {
        self.typed(
            name,
            "{ quest, done = true }, { quest, underway = true }, { quest, phase } or { quest, had = true }",
            |v| {
                let t = v.as_table()?;
                if t.keys()
                    .any(|k| !["quest", "done", "underway", "phase", "had"].contains(&k.as_str()))
                {
                    return None;
                }
                let quest = as_u32(t.get("quest")?)?;
                let gate = match (t.get("done"), t.get("underway"), t.get("phase"), t.get("had")) {
                    (Some(d), None, None, None) if d.as_bool() == Some(true) => Gate::Done,
                    (None, Some(u), None, None) if u.as_bool() == Some(true) => Gate::UnderWay,
                    (None, None, Some(p), None) => Gate::Phase(as_u32(p).filter(|p| *p > 0)?),
                    (None, None, None, Some(h)) if h.as_bool() == Some(true) => Gate::Had,
                    _ => return None,
                };
                Some((quest, gate))
            },
        )
    }

    /// A step's or a give's `requires`: other quests (`{ quest, phase }`, `{ quest, done = true }`,
    /// `{ quest, free = true }` or `{ quest, failed = true }`); `{ kingdom = "word" }`, the one kingdom it is for, a word of
    /// [`yalain_content::kingdoms::kingdom::WORDS`]; `{ tally = "key", full = true|false }`, a
    /// `[[tally]]` that must be full or not yet full; `{ craft = S, at_least = N }`, a craft skill
    /// (or `craft = [S, ...]`, any one of several) at level N or above; `{ untraining = true }`, an
    /// untraining under way; `{ hero = true }`, a hero; and, for a step (`events`),
    /// `{ event = "key" }`, an `[[event]]` that must be on, and `{ act = A }`, an act that must be
    /// whole.
    fn requires_and_events(&mut self, name: &str, events: bool) -> Requires {
        use yalain_content::kingdoms::kingdom;
        enum Req {
            Quest(u32, Need),
            Event(String),
            Tally(Vec<yalain_content::tallies::Gate>),
            Kingdom(u32),
            Act(u32),
            Craft(yalain_content::crafts::CraftGate),
            Untraining,
            Hero,
        }
        let (what, keys): (&str, &[&str]) = if events {
            (
                "a list of { quest, phase }, { quest, done = true }, { quest, free = true }, { quest, failed = true }, { event }, { tally, full }, { tally, at_least, below }, { craft, at_least }, { untraining = true }, { hero = true }, { act } or { kingdom } (neutral, shadow, order or dominion)",
                &["quest", "phase", "done", "free", "failed", "event", "tally", "full", "at_least", "below", "craft", "untraining", "hero", "act", "kingdom"],
            )
        } else {
            (
                "a list of { quest, phase }, { quest, done = true }, { quest, free = true }, { quest, failed = true }, { tally, full }, { tally, at_least, below }, { craft, at_least }, { untraining = true }, { hero = true } or { kingdom } (neutral, shadow, order or dominion)",
                &["quest", "phase", "done", "free", "failed", "tally", "full", "at_least", "below", "craft", "untraining", "hero", "kingdom"],
            )
        };
        let all = self.list_of(name, what, keys, |t| {
            if t.contains_key("tally") {
                return tally_gate(t).map(Req::Tally);
            }
            if t.contains_key("craft") {
                return craft_gate(t).map(Req::Craft);
            }
            if t.contains_key("at_least") || t.contains_key("below") {
                return None;
            }
            if let Some(u) = t.get("untraining") {
                return (t.len() == 1 && u.as_bool() == Some(true)).then_some(Req::Untraining);
            }
            if let Some(h) = t.get("hero") {
                return (t.len() == 1 && h.as_bool() == Some(true)).then_some(Req::Hero);
            }
            if let Some(e) = t.get("event") {
                return (t.len() == 1)
                    .then(|| e.as_str().filter(|s| is_symbol(s)).map(str::to_string))
                    .flatten()
                    .map(Req::Event);
            }
            if let Some(k) = t.get("kingdom") {
                return (t.len() == 1)
                    .then(|| k.as_str().and_then(kingdom::from_word))
                    .flatten()
                    .map(Req::Kingdom);
            }
            if let Some(a) = t.get("act") {
                return (t.len() == 1).then(|| as_u32(a)).flatten().map(Req::Act);
            }
            let quest = as_u32(t.get("quest")?)?;
            let true_ = |v: &toml::Value| v.as_bool() == Some(true);
            let need = match (
                t.get("phase"),
                t.get("done"),
                t.get("free"),
                t.get("failed"),
            ) {
                (Some(p), None, None, None) => Need::Phase(as_u32(p)?),
                (None, Some(d), None, None) if true_(d) => Need::Done,
                (None, None, Some(f), None) if true_(f) => Need::Free,
                (None, None, None, Some(f)) if true_(f) => Need::Failed,
                _ => return None,
            };
            Some(Req::Quest(quest, need))
        });
        let mut quests = Vec::new();
        let mut named = Vec::new();
        let mut tallies = Vec::new();
        let mut kingdoms = Vec::new();
        let mut acts = Vec::new();
        let mut crafts = Vec::new();
        let mut untraining = false;
        let mut heroes = false;
        for r in all {
            match r {
                Req::Quest(q, n) => quests.push((q, n)),
                Req::Event(e) => named.push(e),
                Req::Tally(g) => tallies.extend(g),
                Req::Kingdom(k) => kingdoms.push(k),
                Req::Act(a) => acts.push(a),
                Req::Craft(c) => crafts.push(c),
                Req::Untraining => untraining = true,
                Req::Hero => heroes = true,
            }
        }
        if kingdoms.len() > 1 {
            self.bad(name, "names at most one kingdom");
        }
        Requires {
            quests,
            events: named,
            tallies,
            kingdom: kingdoms.first().copied(),
            acts,
            crafts,
            untraining,
            heroes,
        }
    }

    /// A line's `requires`: `[[tally]]`s that must be full or not yet full, each
    /// `{ tally = "key", full = true|false }`.
    fn tally_gates(&mut self, name: &str) -> Vec<yalain_content::tallies::Gate> {
        self.list_of(
            name,
            "a list of { tally, full } (true for while it is full, false for while it is not) or { tally, at_least, below } (its count at least, and below, so many; either or both)",
            &["tally", "full", "at_least", "below"],
            tally_gate,
        )
        .into_iter()
        .flatten()
        .collect()
    }

    /// A step's or a give's `adds`: `{ tally = "key", by = N }`, N above 0.
    fn adds(&mut self) -> Option<yalain_content::tallies::Adds> {
        let v = self.get("adds")?;
        let read = v.as_table().and_then(|t| {
            if t.keys().any(|k| !["tally", "by"].contains(&k.as_str())) {
                return None;
            }
            let tally = t.get("tally")?.as_str().filter(|s| is_symbol(s))?;
            let by = as_u32(t.get("by")?).filter(|n| *n > 0)?;
            Some(yalain_content::tallies::Adds {
                tally: tally.to_string(),
                by,
            })
        });
        if read.is_none() {
            self.bad(
                "adds",
                "{ tally, by }: a tally's key, and how much it adds, above 0",
            );
        }
        read
    }

    /// A step's `escort`: `{ template, icon, until, far, portal_fails }`, the creature that follows
    /// its player, the icon its list shows it with, the phase it goes at, the meters from it its
    /// player fails the quest past, and whether its player's trip through portal space fails it.
    fn escort(&mut self) -> Option<yalain_content::escorts::Escort> {
        let v = self.get("escort")?;
        let read = v.as_table().and_then(|t| {
            let known = ["template", "icon", "until", "far", "portal_fails"];
            if t.keys().any(|k| !known.contains(&k.as_str())) {
                return None;
            }
            let template = as_u32(t.get("template")?)?;
            let icon = match t.get("icon") {
                Some(i) => as_u32(i)?,
                None => 0,
            };
            let until = match t.get("until") {
                Some(u) => Some(as_u32(u).filter(|n| *n > 0)?),
                None => None,
            };
            let far = match t.get("far") {
                #[allow(clippy::cast_possible_truncation)]
                Some(m) => Some(as_number(m).filter(|m| *m > 0.0)? as f32),
                None => None,
            };
            let portal_fails = match t.get("portal_fails") {
                Some(b) => b.as_bool()?,
                None => false,
            };
            Some(yalain_content::escorts::Escort {
                template,
                icon,
                until,
                far,
                portal_fails,
            })
        });
        if read.is_none() {
            self.bad(
                "escort",
                "{ template, icon, until, far, portal_fails }: the template that follows, and optionally its icon, the phase it goes at (above 0), the meters past which the quest fails (above 0) and true where a trip through portal space fails it",
            );
        }
        read
    }

    /// A `[[tally]]`'s `starts`: `{ event, after, lasts }`, the event it starts as it fills, the
    /// seconds from its filling to the event (0 when left out), and how long the event lasts,
    /// above 0.
    fn tally_start(&mut self) -> Option<Option<yalain_content::tallies::Start>> {
        let Some(v) = self.get("starts") else {
            return Some(None);
        };
        let read = v.as_table().and_then(|t| {
            if t.keys()
                .any(|k| !["event", "after", "lasts"].contains(&k.as_str()))
            {
                return None;
            }
            let event = t.get("event")?.as_str().filter(|s| is_symbol(s))?;
            let after = match t.get("after") {
                Some(a) => as_u32(a)?,
                None => 0,
            };
            let lasts = as_u32(t.get("lasts")?).filter(|n| *n > 0)?;
            Some(yalain_content::tallies::Start {
                event: event.to_string(),
                after,
                lasts,
            })
        });
        if read.is_none() {
            self.bad(
                "starts",
                "{ event, after, lasts }: an event's key, the seconds from the tally's filling to it (0 when left out), and how long it lasts in seconds, above 0",
            );
            return None;
        }
        Some(read)
    }

    /// A `[[tally]]`'s `resets`: `{ event = "key" }`, as that event ends, or `{ every = S }`, every
    /// S seconds by the calendar (above 0); left out, never.
    fn tally_reset(&mut self) -> Option<yalain_content::tallies::Reset> {
        use yalain_content::tallies::Reset;
        let Some(v) = self.get("resets") else {
            return Some(Reset::Never);
        };
        let read = v
            .as_table()
            .and_then(|t| match (t.len(), t.get("event"), t.get("every")) {
                (1, Some(e), None) => e
                    .as_str()
                    .filter(|s| is_symbol(s))
                    .map(|e| Reset::Ends(e.to_string())),
                (1, None, Some(n)) => as_u32(n).filter(|n| *n > 0).map(Reset::Every),
                _ => None,
            });
        if read.is_none() {
            self.bad(
                "resets",
                "{ event = \"key\" } (as that event ends) or { every = S } (every S seconds by the calendar, above 0)",
            );
        }
        read
    }

    /// An `[[event]]`'s `when`: at least one window, each `{ from, until }` (UTC times,
    /// `YYYY-MM-DDTHH:MMZ`, the end after the start) or `{ days, from, until }` (days of the week,
    /// `mon` to `sun`, and UTC times of day, `HH:MM`).
    fn windows(&mut self, name: &str) -> Option<Vec<yalain_content::events::Window>> {
        use yalain_content::events::{time_of_day, utc, Window, DAYS};
        let Some(v) = self.get(name) else {
            return self.missing(name);
        };
        let empty = v.as_array().is_some_and(Vec::is_empty);
        let w = self.list_of(
            name,
            "a list of { from, until } (UTC: \"2026-12-24T09:00Z\", the end after the start), { days, from, until } (days \"mon\" to \"sun\", UTC times of day \"HH:MM\"), { game = \"hours\", from, until } (the game's hours \"HH:MM\") or { game = \"day\" } or { game = \"night\" }",
            &["days", "from", "until", "game"],
            |t| {
                match t.get("game").map(toml::Value::as_str) {
                    None => {}
                    Some(Some(day @ ("day" | "night"))) => {
                        let alone = t.len() == 1;
                        return alone.then_some(Window::GameDay {
                            night: day == "night",
                        });
                    }
                    Some(Some("hours")) => {
                        let (from, until) = (t.get("from")?.as_str()?, t.get("until")?.as_str()?);
                        if t.contains_key("days") {
                            return None;
                        }
                        return Some(Window::GameHours {
                            from: time_of_day(from)?,
                            until: time_of_day(until)?,
                        });
                    }
                    Some(_) => return None,
                }
                let (from, until) = (t.get("from")?.as_str()?, t.get("until")?.as_str()?);
                match t.get("days") {
                    None => {
                        let (from, until) = (utc(from)?, utc(until)?);
                        (until > from).then_some(Window::Dated { from, until })
                    }
                    Some(d) => {
                        let mut days = 0u8;
                        for day in d.as_array()? {
                            let i = DAYS.iter().position(|n| Some(*n) == day.as_str())?;
                            days |= 1 << i;
                        }
                        (days != 0).then_some(Window::Weekly {
                            days,
                            from: time_of_day(from)?,
                            until: time_of_day(until)?,
                        })
                    }
                }
            },
        );
        if w.is_empty() {
            if empty {
                self.bad(name, "an event needs at least one window");
            }
            return None;
        }
        Some(w)
    }

    /// A word of a closed set: `None` when absent, `Some(None)` (and a problem) when it is not one
    /// of the set.
    fn word<T>(
        &mut self,
        name: &str,
        of: impl Fn(&str) -> Option<T>,
        what: &str,
    ) -> Option<Option<T>> {
        let v = self.get(name)?;
        let r = v.as_str().and_then(&of);
        if r.is_none() {
            self.bad(name, &format!("{v} is not {what}"));
        }
        Some(r)
    }

    /// The row's provenance, read in the shared format's grammar.
    fn pet_tends(&mut self) -> Option<PetTendsRow> {
        let v = self.get("tends")?;
        let t = v.as_table();
        let read = t.and_then(|t| {
            let whom = t.get("whom")?.as_str()?;
            Some(PetTendsRow {
                pets: match whom {
                    "owner" => false,
                    "owners-pets" => true,
                    _ => return None,
                },
                effect: as_u32(t.get("effect")?)?,
                every: to_f32(as_number(t.get("every")?)?),
                below: to_f32(as_number(t.get("below")?)?),
            })
        });
        if read.is_none() {
            self.bad(
                "tends",
                "{ whom = \"owner\" or \"owners-pets\", effect, every, below }",
            );
        }
        read
    }

    /// `{ table, maturity_in, mature, harvest, not_mature, not_yours, withers, stages, conditions }`:
    /// where a plant's lines are (`table`, `mature` and `harvest` required).
    fn plant_texts(&mut self) -> Option<yalain_content::plants::Texts> {
        const KEYS: [&str; 9] = [
            "table",
            "maturity_in",
            "mature",
            "harvest",
            "not_mature",
            "not_yours",
            "withers",
            "stages",
            "conditions",
        ];
        const CONDITIONS: [&str; 5] = ["healthy", "dry", "overwatered", "hungry", "overfed"];
        let v = self.get("texts")?;
        let read = v.as_table().and_then(|t| {
            let id = |k: &str| t.get(k).and_then(as_u32);
            // A key given must read; one left out is none.
            let opt = |k: &str| match t.get(k) {
                Some(v) => as_u32(v).map(Some),
                None => Some(None),
            };
            t.keys().all(|k| KEYS.contains(&k.as_str())).then_some(())?;
            let stages = match t.get("stages") {
                Some(v) => {
                    let a = v.as_array()?;
                    let ids: Vec<u32> = a.iter().map(as_u32).collect::<Option<_>>()?;
                    Some(<[u32; 3]>::try_from(ids).ok()?)
                }
                None => None,
            };
            let conditions = match t.get("conditions") {
                Some(v) => {
                    let c = v.as_table()?;
                    (c.len() == CONDITIONS.len()).then_some(())?;
                    let line = |k: &str| c.get(k).and_then(as_u32);
                    Some(yalain_content::plants::Conditions {
                        healthy: line("healthy")?,
                        dry: line("dry")?,
                        overwatered: line("overwatered")?,
                        hungry: line("hungry")?,
                        overfed: line("overfed")?,
                    })
                }
                None => None,
            };
            Some(yalain_content::plants::Texts {
                table: id("table")?,
                maturity_in: opt("maturity_in")?,
                mature: id("mature")?,
                harvest: id("harvest")?,
                not_mature: opt("not_mature")?,
                not_yours: opt("not_yours")?,
                withers: opt("withers")?,
                stages,
                conditions,
            })
        });
        if read.is_none() {
            self.bad(
                "texts",
                "{ table, mature, harvest } and any of maturity_in, not_mature, not_yours, withers \
                 (string ids), stages (three string ids) and conditions \
                 ({ healthy, dry, overwatered, hungry, overfed }, string ids)",
            );
        }
        read
    }

    /// `{ template, planted, busy }`: the garden a plant's seed is given to, and its lines (string
    /// references, each left out or given).
    fn plant_garden(&mut self) -> Option<yalain_content::plants::Garden> {
        let v = self.get("garden")?;
        let line = |v: Option<&toml::Value>| match v {
            None => Some(None),
            Some(v) => {
                let t = v.as_table()?;
                (t.len() == 2).then_some(())?;
                Some(Some((as_u32(t.get("string")?)?, as_u32(t.get("table")?)?)))
            }
        };
        let read = v.as_table().and_then(|t| {
            t.keys()
                .all(|k| ["template", "planted", "busy"].contains(&k.as_str()))
                .then_some(())?;
            Some(yalain_content::plants::Garden {
                template: as_u32(t.get("template")?)?,
                planted: line(t.get("planted"))?,
                busy: line(t.get("busy"))?,
            })
        });
        if read.is_none() {
            self.bad(
                "garden",
                "{ template, planted, busy }: the template given to, and its lines \
                 (string references { table, string })",
            );
        }
        read
    }

    fn pet_hire(&mut self) -> Option<PetHireRow> {
        let v = self.get("hire")?;
        let read = v.as_table().and_then(|t| {
            Some(PetHireRow {
                class: as_u32(t.get("class")?)?,
                seconds: to_f32(as_number(t.get("seconds")?)?),
                level: to_f32(as_number(t.get("level")?)?),
            })
        });
        if read.is_none() {
            self.bad("hire", "{ class, seconds, level }");
        }
        read
    }

    /// `{ classes, effects, chance }`: the classes and the effects (saved states) it keeps off, at
    /// least one of either, and its chance, above 0 and at most 1.
    /// A field that must be there, converted; what it should be is said when it is not.
    fn req<T>(
        &mut self,
        name: &str,
        what: &str,
        conv: impl FnOnce(&toml::Value) -> Option<T>,
    ) -> Option<T> {
        match self.t.get(name) {
            Some(_) => self.typed(name, what, conv),
            None => self.missing(name),
        }
    }

    /// A field's value held to a rule: none, and the rule said, when it breaks it.
    fn held<T>(&mut self, name: &str, value: T, ok: bool, rule: &str) -> Option<T> {
        if ok {
            Some(value)
        } else {
            self.bad(name, rule);
            None
        }
    }

    fn req_strref(&mut self, name: &str) -> Option<StrRef> {
        match self.t.get(name) {
            Some(_) => self.strref(name),
            None => self.missing(name),
        }
    }

    /// `[[mine]]`'s `delving`: `{ effect, node }`, the name of a delving stone's effect and the node
    /// it conjures; none when left out.
    fn delving(&mut self) -> Option<(String, u32)> {
        self.typed(
            "delving",
            "{ effect, node }: a delving stone's effect's name and the node it conjures",
            |v| {
                let t = v.as_table()?;
                if t.len() != 2 {
                    return None;
                }
                Some((
                    t.get("effect")?
                        .as_str()
                        .filter(|e| is_ident(e))?
                        .to_string(),
                    as_u32(t.get("node")?)?,
                ))
            },
        )
    }

    /// `[[mine]]`'s `lodestones`: `{ from, template }`, levels rising from 0.
    fn lodestones(&mut self) -> Vec<(i32, u32)> {
        let read = self.list_of(
            "lodestones",
            "a list of { from, template }: the finder's least level and the lodestone",
            &["from", "template"],
            |t| {
                Some((
                    i32::try_from(t.get("from")?.as_integer()?).ok()?,
                    as_u32(t.get("template")?)?,
                ))
            },
        );
        let rising =
            read.first().is_none_or(|(f, _)| *f == 0) && read.windows(2).all(|w| w[0].0 < w[1].0);
        if rising {
            read
        } else {
            self.bad("lodestones", "levels rising from 0");
            Vec::new()
        }
    }

    /// `[[mine]]`'s `finds`: `{ template, share }`, shares above 0 and at most 1 in all.
    fn finds(&mut self) -> Vec<(u32, f32)> {
        let read = self.list_of(
            "finds",
            "a list of { template, share }",
            &["template", "share"],
            |t| {
                Some((
                    as_u32(t.get("template")?)?,
                    to_f32(as_number(t.get("share")?)?),
                ))
            },
        );
        let sum: f32 = read.iter().map(|(_, s)| s).sum();
        if read.iter().all(|(_, s)| *s > 0.0) && sum <= 1.0 + 1e-6 {
            read
        } else {
            self.bad("finds", "shares above 0, at most 1 in all");
            Vec::new()
        }
    }

    /// `[[treasure_rules]]`'s fields.
    fn treasure_rules(&mut self) -> Option<yalain_content::treasure_tables::Rules> {
        use yalain_content::treasure_tables::{LookRule, LoreRule, Rules};
        let above_base = self.req_floats("above_base")?;
        let ok = weights_ok(&above_base);
        let above_base = self.held("above_base", above_base, ok, WEIGHTS)?;
        let above_base_override = if self.t.contains_key("above_base_override") {
            let w = self.req_floats("above_base_override")?;
            let ok = weights_ok(&w);
            Some(self.held("above_base_override", w, ok, WEIGHTS)?)
        } else {
            None
        };
        let most_level = self.req("most_level", "a whole number", toml::Value::as_integer)?;
        let most_level = i32::try_from(most_level).ok().filter(|l| *l >= 1);
        let most_level =
            self.held("most_level", most_level, most_level.is_some(), "1 or more")??;
        let lore = self.req("lore", "{ above, from, to }: whole numbers", |v| {
            let t = exactly(v, &["above", "from", "to"])?;
            Some(LoreRule {
                above: int_of(t.get("above")?)?,
                from: int_of(t.get("from")?)?,
                to: int_of(t.get("to")?)?,
            })
        })?;
        let lore = self.held("lore", lore, lore.from <= lore.to, "from at most to")?;
        let potion_tiers = self.req("potion_tiers", "a list of whole numbers", ints)?;
        let ok = edges_ok(&potion_tiers);
        let potion_tiers = self.held(
            "potion_tiers",
            potion_tiers,
            ok,
            "the least dropper level of each tier of the potions, from 0, rising",
        )?;
        let spellcraft = self.req("potion_spellcraft", "[least, most]", pair_of)?;
        let ok = spellcraft.0 <= spellcraft.1;
        let potion_spellcraft =
            self.held("potion_spellcraft", spellcraft, ok, "least at most most")?;
        let look = self.req(
            "look",
            "{ glow_most, round_chance, round_from, round_tenths }",
            |v| {
                let t = exactly(
                    v,
                    &["glow_most", "round_chance", "round_from", "round_tenths"],
                )?;
                Some(LookRule {
                    glow_most: number_of(t.get("glow_most")?)?,
                    round_chance: number_of(t.get("round_chance")?)?,
                    round_from: number_of(t.get("round_from")?)?,
                    round_tenths: as_u32(t.get("round_tenths")?)?,
                })
            },
        )?;
        let ok = [look.glow_most, look.round_chance, look.round_from]
            .into_iter()
            .all(share_ok)
            && look.round_tenths >= 1;
        let look = self.held(
            "look",
            look,
            ok,
            "glow_most, round_chance and round_from from 0 to 1, round_tenths 1 or more",
        )?;
        Some(Rules {
            above_base,
            above_base_override,
            most_level,
            lore,
            potion_tiers,
            potion_spellcraft,
            look,
        })
    }

    /// `[[treasure_sort]]`'s fields: its sort, and the numbers it draws by.
    #[allow(clippy::too_many_lines)]
    fn treasure_sort(
        &mut self,
    ) -> Option<(
        yalain_content::treasure_tables::Sort,
        yalain_content::treasure_tables::SortTables,
    )> {
        use yalain_content::treasure_tables::{CountBand, Delays, Sort, SortTables, VigorBand};
        let name = self.req_str("sort")?;
        let sort = Sort::of_name(&name);
        let sort = self.held(
            "sort",
            sort,
            sort.is_some(),
            "weapon, armor, shield or jewelry",
        )??;
        let own = TREASURE_SORT_FIELDS
            .iter()
            .find(|(s, _)| *s == name)
            .map_or(&[][..], |(_, f)| *f);
        for (_, fields) in &TREASURE_SORT_FIELDS {
            for field in *fields {
                let has = self.t.contains_key(*field);
                if has && !own.contains(field) {
                    self.bad(field, &format!("not a field of a {name} row"));
                } else if !has && own.contains(field) {
                    self.bad(field, &format!("missing: a {name} row has it"));
                }
            }
        }
        if self.bad {
            return None;
        }
        let counts = self.req("counts", "a list of { from, weights }", |v| {
            v.as_array()?
                .iter()
                .map(|b| {
                    let t = exactly(b, &["from", "weights"])?;
                    Some(CountBand {
                        from: int_of(t.get("from")?)?,
                        weights: numbers(t.get("weights")?)?,
                    })
                })
                .collect::<Option<Vec<_>>>()
        })?;
        let ok = edges_ok(&counts.iter().map(|b| b.from).collect::<Vec<_>>())
            && counts.iter().all(|b| weights_ok(&b.weights));
        let counts = self.held(
            "counts",
            counts,
            ok,
            &format!("bands from 0, rising, each with {WEIGHTS}"),
        )?;
        let mut tables = SortTables {
            counts,
            ..SortTables::default()
        };
        if own.contains(&"damage_share") {
            let p = self.req("damage_share", "[least, most]", pair_of)?;
            let ok = share_ok(p.0) && share_ok(p.1) && p.0 <= p.1;
            tables.damage_share = Some(self.held("damage_share", p, ok, SHARES)?);
        }
        if own.contains(&"vigor") {
            let bands = self.req(
                "vigor",
                "a list of { from, least, most, every }, every may be left out",
                |v| {
                    v.as_array()?
                        .iter()
                        .map(|b| {
                            let t = b.as_table()?;
                            if t.keys()
                                .any(|k| !["from", "least", "most", "every"].contains(&k.as_str()))
                            {
                                return None;
                            }
                            Some(VigorBand {
                                from: int_of(t.get("from")?)?,
                                least: int_of(t.get("least")?)?,
                                most: int_of(t.get("most")?)?,
                                every: match t.get("every") {
                                    Some(e) => Some(int_of(e)?),
                                    None => None,
                                },
                            })
                        })
                        .collect::<Option<Vec<_>>>()
                },
            )?;
            let last = bands.len().saturating_sub(1);
            let ok = edges_ok(&bands.iter().map(|b| b.from).collect::<Vec<_>>())
                && bands.iter().enumerate().all(|(i, b)| {
                    b.least >= 1 && b.least <= b.most && b.every.is_none_or(|e| i == last && e >= 1)
                });
            tables.vigor = self.held(
                "vigor",
                bands,
                ok,
                "bands from 0, rising, least 1 or more and at most most; every (1 or more) only on the last",
            )?;
        }
        if own.contains(&"span") {
            let p = self.req("span", "[least, most]", pair_of)?;
            let ok = share_ok(p.0) && share_ok(p.1) && p.0 <= p.1;
            tables.span = Some(self.held("span", p, ok, SHARES)?);
        }
        if own.contains(&"delays") {
            let d = self.req("delays", "{ from, weights }", |v| {
                let t = exactly(v, &["from", "weights"])?;
                Some(Delays {
                    from: int_of(t.get("from")?)?,
                    weights: numbers(t.get("weights")?)?,
                })
            })?;
            let ok = d.from >= 0 && weights_ok(&d.weights);
            let rule = format!("from 0 or more, and {WEIGHTS}");
            tables.delays = Some(self.held("delays", d, ok, &rule)?);
        }
        if own.contains(&"rating_share") {
            let points = self.req("rating_share", "a list of [level, share]", |v| {
                v.as_array()?
                    .iter()
                    .map(|p| {
                        let p = p.as_array().filter(|p| p.len() == 2)?;
                        Some((int_of(&p[0])?, number_of(&p[1])?))
                    })
                    .collect::<Option<Vec<_>>>()
            })?;
            let ok = !points.is_empty()
                && points.windows(2).all(|w| w[0].0 < w[1].0)
                && points.iter().all(|(l, s)| *l >= 0 && share_ok(*s));
            tables.rating_share = self.held(
                "rating_share",
                points,
                ok,
                "at least one point, levels from 0 and rising, each share from 0 to 1",
            )?;
            for name in ["rating_spread", "slot_chance"] {
                let x = self.req(name, "a number", number_of)?;
                let x = self.held(name, x, share_ok(x), "from 0 to 1")?;
                if name == "rating_spread" {
                    tables.rating_spread = x;
                } else {
                    tables.slot_chance = x;
                }
            }
        }
        Some((sort, tables))
    }

    fn ward(&mut self) -> Option<yalain_content::effect_rules::Ward> {
        let Some(v) = self.get("ward") else {
            return self.missing("ward");
        };
        let list = |t: &toml::Table, k: &str| match t.get(k) {
            Some(v) => v.as_array()?.iter().map(as_u32).collect::<Option<Vec<_>>>(),
            None => Some(Vec::new()),
        };
        let read = v.as_table().and_then(|t| {
            if t.keys()
                .any(|k| !["classes", "effects", "chance"].contains(&k.as_str()))
            {
                return None;
            }
            let ward = yalain_content::effect_rules::Ward {
                classes: list(t, "classes")?,
                effects: list(t, "effects")?,
                chance: to_f32(as_number(t.get("chance")?)?),
            };
            (!(ward.classes.is_empty() && ward.effects.is_empty())
                && ward.chance > 0.0
                && ward.chance <= 1.0)
                .then_some(ward)
        });
        if read.is_none() {
            self.bad(
                "ward",
                "{ classes, effects, chance }: the classes and the effects it keeps off, at least one, and a chance above 0, at most 1",
            );
        }
        read
    }

    fn vitae_restore(&mut self) -> Option<yalain_content::effect_rules::VitaeRestore> {
        let Some(v) = self.get("vitae_restore") else {
            return self.missing("vitae_restore");
        };
        let read = v.as_table().and_then(|t| {
            if t.keys()
                .any(|k| !["points", "fellows"].contains(&k.as_str()))
            {
                return None;
            }
            let points = to_f32(as_number(t.get("points")?)?);
            let fellows = match t.get("fellows") {
                Some(b) => b.as_bool()?,
                None => false,
            };
            (points > 0.0 && points <= 100.0)
                .then_some(yalain_content::effect_rules::VitaeRestore { points, fellows })
        });
        if read.is_none() {
            self.bad(
                "vitae_restore",
                "{ points, fellows }: the points of vitae it gives back, above 0, at most 100, and whether its player's fellows have them too",
            );
        }
        read
    }

    /// A list of what a use makes standing in the world, each `{ template, offset, lasts }`: the
    /// template, its offset (`[across, ahead]` in meters, `[0, 0]` when left out) and its time
    /// (seconds above 0; left out, until cleared or killed). At least one.
    fn placings(&mut self, name: &str) -> Option<Vec<yalain_content::effect_rules::Placing>> {
        let list = self.typed(
            name,
            "a list of at least one { template, offset = [across, ahead], lasts }, offset and lasts \
             left out or two numbers of meters and seconds above 0",
            |v| {
                let a = v.as_array().filter(|a| !a.is_empty())?;
                a.iter()
                    .map(|x| {
                        let t = x.as_table()?;
                        if t.keys()
                            .any(|k| !["template", "offset", "lasts"].contains(&k.as_str()))
                        {
                            return None;
                        }
                        let template = as_u32(t.get("template")?)?;
                        let offset = match t.get("offset") {
                            None => [0.0, 0.0],
                            Some(o) => match o.as_array()?.as_slice() {
                                [x, y] => {
                                    let (x, y) = (to_f32(as_number(x)?), to_f32(as_number(y)?));
                                    (x.is_finite() && y.is_finite()).then_some([x, y])?
                                }
                                _ => return None,
                            },
                        };
                        let lasts = match t.get("lasts") {
                            None => None,
                            Some(l) => Some(as_u32(l).filter(|s| *s > 0)?),
                        };
                        Some(yalain_content::effect_rules::Placing {
                            template,
                            offset,
                            lasts,
                        })
                    })
                    .collect::<Option<Vec<_>>>()
            },
        )?;
        Some(list)
    }

    /// An `[[effect_rule]]`'s `places`, a template, with its `offset` (`[across, ahead]` in
    /// meters, `[0, 0]` when left out) and its `lasts` (seconds, above 0; left out, until cleared);
    /// neither of those without `places`.
    fn placing(&mut self) -> Option<Option<yalain_content::effect_rules::Placing>> {
        if self.get("places").is_none() {
            for k in ["offset", "lasts"] {
                if self.get(k).is_some() {
                    self.bad(k, "an offset and a time are of what a rule places");
                    return None;
                }
            }
            return Some(None);
        }
        let template = self.id("places")?;
        let offset = match self.get("offset") {
            None => [0.0, 0.0],
            Some(_) => match self.req_floats("offset")?.as_slice() {
                [x, y] if x.is_finite() && y.is_finite() => [*x, *y],
                _ => {
                    self.bad("offset", "[across, ahead]: two numbers of meters");
                    return None;
                }
            },
        };
        let lasts = self.u32("lasts");
        if self.get("lasts").is_some() && lasts.is_none_or(|t| t == 0) {
            self.bad("lasts", "a number of seconds above 0");
            return None;
        }
        Some(Some(yalain_content::effect_rules::Placing {
            template,
            offset,
            lasts,
        }))
    }

    /// A share, above 0 and at most 1.
    fn share(&mut self, name: &str) -> Option<f64> {
        self.typed(name, "a share above 0, at most 1", |v| {
            as_number(v).filter(|s| *s > 0.0 && *s <= 1.0)
        })
    }

    /// A number of seconds, from 0.
    fn seconds(&mut self, name: &str) -> Option<f64> {
        self.typed(name, "a number of seconds from 0", |v| {
            as_number(v).filter(|s| *s >= 0.0)
        })
    }

    /// `{ seconds, fx, behavior, text, name }`: how long after the give's answer, from 0; the
    /// appearances and the behavior played on the player meanwhile; what the new hero is told (a
    /// `{ table, string }`), and the variable its name fills.
    fn transformation(&mut self) -> Option<yalain_content::quests::Transformation> {
        let v = self.get("transforms")?;
        let read = v.as_table().and_then(|t| {
            if t.keys()
                .any(|k| !["seconds", "fx", "behavior", "text", "name"].contains(&k.as_str()))
            {
                return None;
            }
            let fx = match t.get("fx") {
                Some(v) => v
                    .as_array()?
                    .iter()
                    .map(as_u32)
                    .collect::<Option<Vec<_>>>()?,
                None => Vec::new(),
            };
            let text = match t.get("text") {
                Some(v) => {
                    let s = v.as_table().filter(|s| s.len() == 2)?;
                    Some((as_u32(s.get("string")?)?, as_u32(s.get("table")?)?))
                }
                None => None,
            };
            let name = match t.get("name") {
                Some(v) => as_u32(v).filter(|_| text.is_some())?,
                None => 0,
            };
            Some(yalain_content::quests::Transformation {
                seconds: as_number(t.get("seconds")?).filter(|s| *s >= 0.0)?,
                fx,
                behavior: t
                    .get("behavior")
                    .map(as_u32)
                    .map_or(Some(None), |b| b.map(Some))?,
                text,
                name,
            })
        });
        if read.is_none() {
            self.bad(
                "transforms",
                "{ seconds, fx, behavior, text, name }: its seconds from 0, the appearances and the behavior played meanwhile, the text the hero is told ({ table, string }) and the variable its name fills, which needs the text",
            );
        }
        read
    }

    /// A list of lines said at times: `{ at, line }`, `at` seconds (0 or above), `line` a string
    /// reference `{ table, string }`, in order of their times; none when left out. `None` when one
    /// is wrong.
    fn timed_lines(&mut self, name: &str) -> Option<Vec<(f64, (u32, u32))>> {
        let Some(v) = self.get(name) else {
            return Some(Vec::new());
        };
        let read = v.as_array().and_then(|a| {
            a.iter()
                .map(|e| {
                    let t = e.as_table().filter(|t| t.len() == 2)?;
                    let at = t
                        .get("at")
                        .and_then(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64)))?;
                    let l = t.get("line")?.as_table().filter(|l| l.len() == 2)?;
                    let line = (as_u32(l.get("string")?)?, as_u32(l.get("table")?)?);
                    (at >= 0.0).then_some((at, line))
                })
                .collect::<Option<Vec<_>>>()
        });
        let ordered = read
            .as_ref()
            .is_some_and(|l| l.windows(2).all(|w| w[0].0 <= w[1].0));
        if !ordered {
            self.bad(
                name,
                "{ at, line }: seconds (0 or above) and a string reference { table, string }, in order of their times",
            );
            return None;
        }
        read
    }

    /// `[[creature]]`'s `mark_lines`: `{ race, line }`, a line said of a player of one race marked
    /// (or, with no race, of any other), `line` a string reference `{ table, string }`; none when
    /// left out. `None` when one is wrong.
    fn mark_lines(&mut self) -> Option<Vec<yalain_content::creature_rows::MarkLine>> {
        let Some(v) = self.get("mark_lines") else {
            return Some(Vec::new());
        };
        let races = [1, 2, 4, 8, 16];
        let read = v.as_array().and_then(|a| {
            a.iter()
                .map(|e| {
                    let t = e.as_table()?;
                    if t.keys().any(|k| !["race", "line"].contains(&k.as_str())) {
                        return None;
                    }
                    let race = match t.get("race") {
                        Some(r) => Some(as_u32(r).filter(|r| races.contains(r))?),
                        None => None,
                    };
                    let l = t.get("line")?.as_table().filter(|l| l.len() == 2)?;
                    Some((race, (as_u32(l.get("string")?)?, as_u32(l.get("table")?)?)))
                })
                .collect::<Option<Vec<_>>>()
        });
        let one_each = read.as_ref().is_some_and(|lines| {
            lines
                .iter()
                .enumerate()
                .all(|(i, (r, _))| lines[..i].iter().all(|(q, _)| q != r))
        });
        if !one_each {
            self.bad(
                "mark_lines",
                "{ race, line }: a race (1, 2, 4, 8 or 16, or none for any other) and a string reference { table, string }, one line to a race",
            );
            return None;
        }
        read
    }

    fn req_prov(&mut self) -> Option<Prov> {
        let Some(v) = self.get("prov") else {
            return self.missing("prov");
        };
        match v.clone().try_into::<Prov>() {
            Ok(p) => Some(p),
            Err(e) => {
                self.bad("prov", e.message());
                None
            }
        }
    }
}

/// A reference to a row by key: `key`, `key@XXYY` (another landblock), `pack:key[@XXYY]`
/// (another pack).
#[must_use]
pub fn is_reference(s: &str) -> bool {
    let (pack, rest) = match s.split_once(':') {
        Some((p, r)) => (Some(p), r),
        None => (None, s),
    };
    let (key, lb) = match rest.split_once('@') {
        Some((k, l)) => (k, Some(l)),
        None => (rest, None),
    };
    pack.is_none_or(is_symbol)
        && is_symbol(key)
        && lb.is_none_or(|l| l.len() == 4 && l.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// What a list of weights must be.
const WEIGHTS: &str = "weights: numbers from 0, at least one above 0";

/// What a pair of shares must be.
const SHARES: &str = "[least, most], each from 0 to 1, least at most most";

/// Weights to draw by: none below 0, and some above.
fn weights_ok(w: &[f32]) -> bool {
    w.iter().all(|x| x.is_finite() && *x >= 0.0) && w.iter().any(|x| *x > 0.0)
}

/// The least levels of bands: from 0, each above the one before.
fn edges_ok(e: &[i32]) -> bool {
    e.first() == Some(&0) && e.windows(2).all(|w| w[0] < w[1])
}

/// A share or a chance: from 0 to 1.
fn share_ok(x: f32) -> bool {
    (0.0..=1.0).contains(&x)
}

/// A table with exactly these keys.
fn exactly<'v>(v: &'v toml::Value, keys: &[&str]) -> Option<&'v toml::Table> {
    v.as_table()
        .filter(|t| t.len() == keys.len() && keys.iter().all(|k| t.contains_key(*k)))
}

fn int_of(v: &toml::Value) -> Option<i32> {
    v.as_integer().and_then(|i| i32::try_from(i).ok())
}

fn ints(v: &toml::Value) -> Option<Vec<i32>> {
    v.as_array()?.iter().map(int_of).collect()
}

fn number_of(v: &toml::Value) -> Option<f32> {
    as_number(v).map(to_f32).filter(|x| x.is_finite())
}

fn numbers(v: &toml::Value) -> Option<Vec<f32>> {
    v.as_array()?.iter().map(number_of).collect()
}

fn pair_of(v: &toml::Value) -> Option<(f32, f32)> {
    let a = v.as_array().filter(|a| a.len() == 2)?;
    Some((number_of(&a[0])?, number_of(&a[1])?))
}

fn as_u32(v: &toml::Value) -> Option<u32> {
    v.as_integer().and_then(|i| u32::try_from(i).ok())
}

/// `{ craft = S, at_least = N }`: a craft skill (its saved state) at level N (above 0) or above;
/// `craft = [S, ...]`, any one of several.
fn craft_gate(t: &toml::Table) -> Option<yalain_content::crafts::CraftGate> {
    if t.len() != 2 {
        return None;
    }
    let skills = match t.get("craft")? {
        toml::Value::Array(a) => a.iter().map(as_u32).collect::<Option<Vec<_>>>()?,
        v => vec![as_u32(v)?],
    };
    let at_least = as_u32(t.get("at_least")?).filter(|n| *n > 0)?;
    (!skills.is_empty()).then_some(yalain_content::crafts::CraftGate { skills, at_least })
}

/// `{ tally = "key", full = true|false }`: a tally that must be full, or not yet full; or
/// `{ tally = "key", at_least = N, below = M }`, either or both: its count at least N (above 0),
/// and below M (above N).
fn tally_gate(t: &toml::Table) -> Option<Vec<yalain_content::tallies::Gate>> {
    use yalain_content::tallies::{Gate, Holds};
    let tally = t.get("tally")?.as_str().filter(|s| is_symbol(s))?;
    let gate = |holds| Gate {
        tally: tally.to_string(),
        holds,
    };
    if let Some(full) = t.get("full") {
        let full = full.as_bool()?;
        return (t.len() == 2).then(|| vec![gate(if full { Holds::Full } else { Holds::NotFull })]);
    }
    let at_least = t.get("at_least").map(as_u32);
    let below = t.get("below").map(as_u32);
    let (at_least, below) = match (at_least, below) {
        (Some(None), _) | (_, Some(None)) | (None, None) => return None,
        (a, b) => (a.flatten(), b.flatten()),
    };
    if at_least == Some(0) || below.is_some_and(|b| b <= at_least.unwrap_or(0)) {
        return None;
    }
    Some(
        at_least
            .map(Holds::AtLeast)
            .into_iter()
            .chain(below.map(Holds::Below))
            .map(gate)
            .collect(),
    )
}

#[allow(clippy::cast_precision_loss)]
fn as_number(v: &toml::Value) -> Option<f64> {
    match v {
        toml::Value::Float(f) => Some(*f),
        toml::Value::Integer(i) => Some(*i as f64),
        _ => None,
    }
}
