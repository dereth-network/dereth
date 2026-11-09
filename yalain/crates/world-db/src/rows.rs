//! A database's files read into rows: the world file, each pack's header, the lockfile, and every
//! server row with the file and line it came from. The world file, the pack headers, the
//! lockfile, every row's provenance and the items a removal names are read in the shared pack
//! format's grammar, in this game's dialect (`dereth-pack`, `auberean-pack`); the rows are this
//! database's own. Reading checks the schema: every table and field is one the format has, of
//! its type, with the fields its kind needs; a row that breaks it is a [`Problem`] and is left
//! out.

use std::collections::BTreeMap;
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
    pub greeting: Option<StrRef>,
    pub greeting_text: Option<String>,
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
    /// Whether it fails when its player dies.
    pub fails_on_death: Option<bool>,
    /// Whether it is given only once.
    pub once: Option<bool>,
    /// Whether a kill's move on it reaches the killer's fellows; left out, as the quest's setup in
    /// the install says.
    pub fellows: Option<bool>,
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
    Arrive(u32),
    /// Arriving through one placed portal of the template: the object's id.
    ArriveFrom(u32, u64),
    Take(u32),
    Kill(u32),
    /// Killing a creature of the template that one camp made: the camp's `key`, or `key@XXYY` for
    /// the one in that landblock where several camps share the key.
    KillIn(u32, String),
    Done(u32),
    Given(u32),
}

/// What a step requires of another quest: under way at a phase, done, or free to be given.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Need {
    Phase(u32),
    Done,
    Free,
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
    /// The effects it puts on the player (their saved states), and a text shown after them.
    pub effects: Vec<u32>,
    pub text: Option<StrRef>,
    /// Other quests it requires.
    pub requires: Vec<(u32, Need)>,
    /// The `[[event]]`s it runs only while they run.
    pub events: Vec<String>,
    /// The kingdom it runs for alone (a player in none counting as of the neutral one).
    pub kingdom: Option<u32>,
    /// The items it takes from the player's pack, which the player must hold.
    pub takes: Vec<u32>,
    /// The place it takes the player to, its update made on arrival.
    pub to: Option<String>,
    /// The races it is for (none: every race).
    pub races: Vec<u32>,
    /// Its effects come after its update, not before it.
    pub effects_last: bool,
    /// Its update is made for each of the player's fellows as well.
    pub fellows: bool,
    /// For a give, exactly how many of the item it wants (none: whatever is given).
    pub wants: Option<u32>,
    /// The least and the most level it is for, either left out for no such limit.
    pub least_level: Option<i32>,
    pub most_level: Option<i32>,
    /// It pays this share of what the player's level takes to reach the next, whole.
    pub xp_share: Option<f64>,
    /// For a give, it makes its player a hero, after this transformation.
    pub transforms: Option<yalain_content::quests::Transformation>,
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
    /// Exactly how many of the item it wants (none: whatever is given).
    pub wants: Option<u32>,
    /// The items it gives back in its place.
    pub gives: Vec<u32>,
    /// How many of each item given, in order (1 when left out).
    pub counts: Vec<u32>,
    /// The effects it puts on the player (their saved states), and a text shown after them.
    pub effects: Vec<u32>,
    pub text: Option<StrRef>,
    /// What the person or the object plays as it takes the item.
    pub fx: Option<u32>,
    /// The quests it requires.
    pub requires: Vec<(u32, Need)>,
    /// The kingdom it takes the item from alone (a player in none counting as of the neutral one).
    pub kingdom: Option<u32>,
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
    Sale(Sale),
    Quest(Quest),
    Step(Step),
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
}

/// `[[event]]`: a named state of the world, on for its times: what it gates (camps, objects and
/// steps that name it) stands or runs only while it is on, and its effects are on every player in
/// the world while it is.
#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub key: String,
    /// The install's game event it stands for (a value of its game-event enumeration).
    pub game_event: Option<u32>,
    pub when: Vec<yalain_content::events::Window>,
    pub on_players: Vec<u32>,
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

/// `[[effect_rule]]`: what the world says of an effect where its own data says nothing: what a ward
/// keeps off, the share of its player's vitae it takes.
#[derive(Debug, Clone, PartialEq)]
pub struct EffectRule {
    /// The effect, by its saved state.
    pub effect: u32,
    pub ward: Option<yalain_content::effect_rules::Ward>,
    pub vitae_share: Option<f32>,
    pub prov: Prov,
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

/// `[[death_spawn]]`: what a creature's death effect makes around its killer.
#[derive(Debug, Clone, PartialEq)]
pub struct DeathSpawn {
    /// The death effect, as a creature's template names it.
    pub effect: String,
    pub template: u32,
    pub count: u32,
    pub chance: f32,
    pub offset: At,
    /// How long each stands, in seconds; it stays until killed when left out.
    pub lasts: Option<f32>,
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

/// A `[[pet]]`'s `hire`: a contract's terms.
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
            Self::Remove(_) => None,
        }
    }
}

/// The record kinds, as their tables are written.
pub const KINDS: [&str; 39] = [
    "object",
    "camp",
    "loot",
    "keeper",
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
];

/// Each record kind's fields.
pub const FIELDS: [(&str, &[&str]); 39] = [
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
        &["template", "store", "greeting", "greeting_text", "prov"],
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
            "once",
            "fellows",
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
            "effects",
            "text",
            "requires",
            "takes",
            "to",
            "races",
            "effects_last",
            "fellows",
            "wants",
            "least_level",
            "most_level",
            "xp_share",
            "transforms",
            "prov",
        ],
    ),
    (
        "give",
        &[
            "key", "to", "item", "wants", "gives", "counts", "effects", "text", "fx", "requires",
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
            "effect", "template", "count", "chance", "offset", "lasts", "prov",
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
        &["key", "template", "camp", "item", "chance", "prov"],
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
    ("points_bonus", &["effect", "add", "up_to", "prov"]),
    ("popup", &["on", "effect", "prov"]),
    ("effect_rule", &["effect", "ward", "vitae_share", "prov"]),
    ("treasure_value", &["level", "values", "prov"]),
    (
        "treasure_rules",
        &[
            "above_base",
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
    if let Some(lock) = files.get("world.lock") {
        if let Err(e) = dereth_pack::Lock::<Ac2>::parse(Path::new("world.lock"), lock) {
            problems.push(problem(&e));
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
                Ok(t) => read_rows(&mut pack, path, text, &t, &mut problems),
                Err(p) => problems.push(p),
            }
        }
        packs.push(pack);
    }
    problems.sort();
    (Some(Database { world, packs }), problems)
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
            greeting: f.strref("greeting"),
            greeting_text: f.text("greeting_text"),
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
            once: f.bool("once"),
            fellows: f.bool("fellows"),
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
            let (requires, events, kingdom) = f.requires_and_events("requires", true);
            Row::Step(Step {
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
                effects: f.ids("effects"),
                text: f.strref("text"),
                requires,
                events,
                kingdom,
                takes: f.ids("takes"),
                to: f.symbol("to"),
                races: f.ids("races"),
                effects_last: f.bool("effects_last").unwrap_or(false),
                fellows: f.bool("fellows").unwrap_or(false),
                wants: f.u32("wants"),
                least_level: f.int("least_level").and_then(|v| i32::try_from(v).ok()),
                most_level: f.int("most_level").and_then(|v| i32::try_from(v).ok()),
                xp_share: f.float("xp_share"),
                transforms: match f.get("transforms") {
                    Some(_) => Some(f.transformation()?),
                    None => None,
                },
                prov: f.req_prov()?,
            })
        }
        "give" => {
            let (requires, _, kingdom) = f.requires_and_events("requires", false);
            Row::Give(Give {
                key: f.req_symbol("key")?,
                to: f.req_id("to")?,
                item: f.id("item"),
                wants: f.u32("wants"),
                gives: f.ids("gives"),
                counts: f.ids("counts"),
                effects: f.ids("effects"),
                text: f.strref("text"),
                fx: f.id("fx"),
                requires,
                kingdom,
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
        "event" => {
            let when = f.windows("when")?;
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
            let effect = f.req_id("effect")?;
            let ward = match f.get("ward") {
                Some(_) => Some(f.ward()?),
                None => None,
            };
            let vitae_share = f.share("vitae_share").map(to_f32);
            if ward.is_none() && vitae_share.is_none() {
                f.bad(
                    "ward, vitae_share",
                    "a rule names a ward or a share of vitae",
                );
                return None;
            }
            Row::EffectRule(EffectRule {
                effect,
                ward,
                vitae_share,
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
            Row::DeathTreasure(DeathTreasure {
                key,
                template,
                camp,
                item: f.req_id("item")?,
                chance,
                prov: f.req_prov()?,
            })
        }
        "death_spawn" => {
            if f.get("effect").is_none() {
                return f.missing("effect");
            }
            let effect = f.ident("effect")?;
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
            Row::DeathSpawn(DeathSpawn {
                effect,
                template: f.req_id("template")?,
                count: f
                    .int("count")
                    .and_then(|v| u32::try_from(v).ok())
                    .unwrap_or(1),
                chance,
                offset: f.req_at("offset")?,
                lasts,
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
            "one of { use = T }, { use = ITEM, on = TARGET }, { give = ITEM, to = TARGET }, { arrive = T }, { arrive = T, from = OBJECT }, { take = T }, { kill = T }, { kill = T, camp = KEY }, { done = Q }, { given = Q }",
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
                if let (2, Some(tpl), Some(camp)) = (t.len(), t.get("kill"), t.get("camp")) {
                    let camp = camp.as_str().filter(|c| !c.is_empty())?;
                    return Some(Trigger::KillIn(as_u32(tpl)?, camp.to_string()));
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

    /// A step's or a give's `requires`: other quests (`{ quest, phase }`, `{ quest, done = true }`
    /// or `{ quest, free = true }`); `{ kingdom = "word" }`, the one kingdom it is for, a word of
    /// [`yalain_content::kingdoms::kingdom::WORDS`]; and, where `events` allows them,
    /// `{ event = "key" }`, an `[[event]]` that must be on.
    fn requires_and_events(
        &mut self,
        name: &str,
        events: bool,
    ) -> (Vec<(u32, Need)>, Vec<String>, Option<u32>) {
        use yalain_content::kingdoms::kingdom;
        enum Req {
            Quest(u32, Need),
            Event(String),
            Kingdom(u32),
        }
        let (what, keys): (&str, &[&str]) = if events {
            (
                "a list of { quest, phase }, { quest, done = true }, { quest, free = true }, { event } or { kingdom } (neutral, shadow, order or dominion)",
                &["quest", "phase", "done", "free", "event", "kingdom"],
            )
        } else {
            (
                "a list of { quest, phase }, { quest, done = true }, { quest, free = true } or { kingdom } (neutral, shadow, order or dominion)",
                &["quest", "phase", "done", "free", "kingdom"],
            )
        };
        let all = self.list_of(name, what, keys, |t| {
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
            let quest = as_u32(t.get("quest")?)?;
            let need = match (t.get("phase"), t.get("done"), t.get("free")) {
                (Some(p), None, None) => Need::Phase(as_u32(p)?),
                (None, Some(d), None) if d.as_bool() == Some(true) => Need::Done,
                (None, None, Some(f)) if f.as_bool() == Some(true) => Need::Free,
                _ => return None,
            };
            Some(Req::Quest(quest, need))
        });
        let mut quests = Vec::new();
        let mut named = Vec::new();
        let mut kingdoms = Vec::new();
        for r in all {
            match r {
                Req::Quest(q, n) => quests.push((q, n)),
                Req::Event(e) => named.push(e),
                Req::Kingdom(k) => kingdoms.push(k),
            }
        }
        if kingdoms.len() > 1 {
            self.bad(name, "names at most one kingdom");
        }
        (quests, named, kingdoms.first().copied())
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
            "a list of { from, until } (UTC: \"2026-12-24T09:00Z\", the end after the start) or { days, from, until } (days \"mon\" to \"sun\", UTC times of day \"HH:MM\")",
            &["days", "from", "until"],
            |t| {
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

#[allow(clippy::cast_precision_loss)]
fn as_number(v: &toml::Value) -> Option<f64> {
    match v {
        toml::Value::Float(f) => Some(*f),
        toml::Value::Integer(i) => Some(*i as f64),
        _ => None,
    }
}
