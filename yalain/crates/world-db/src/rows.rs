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
    /// A portal a player who used it may recall to, and summon a portal to.
    pub recall: bool,
    pub summon: bool,
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
    pub prov: Prov,
}

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
    pub xp: Option<u64>,
    /// In place of an amount, the share of what the player's level takes to reach the next that its
    /// completion pays; left out, as the quest's setup in the install says.
    pub xp_share: Option<f64>,
    /// The level at and above which that share pays no more.
    pub xp_share_level: Option<i32>,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    Use(u32),
    UseOn(u32, u32),
    Give(u32, u32),
    Arrive(u32),
    Take(u32),
    Kill(u32),
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
    Scene(Scene),
    Shard(Shard),
    Weapon(Weapon),
    Route(Route),
    Area(Area),
}

/// `[[area]]`: the kind of player conflict a landblock allows (a landblock no row names is a
/// peace area).
#[derive(Debug, Clone, PartialEq)]
pub struct Area {
    pub landblock: u16,
    pub conflict: yalain_content::Conflict,
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
            Self::Scene(_) => "scene",
            Self::Shard(_) => "shard",
            Self::Weapon(_) => "weapon",
            Self::Route(_) => "route",
            Self::Area(_) => "area",
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
            Self::Scene(r) => Some(&r.prov),
            Self::Shard(r) => Some(&r.prov),
            Self::Weapon(r) => Some(&r.prov),
            Self::Route(r) => Some(&r.prov),
            Self::Area(r) => Some(&r.prov),
            Self::Remove(_) => None,
        }
    }
}

/// The record kinds, as their tables are written.
pub const KINDS: [&str; 18] = [
    "object", "camp", "loot", "keeper", "sale", "quest", "step", "give", "place", "talk", "remove",
    "craft", "butchery", "scene", "shard", "weapon", "route", "area",
];

/// Each record kind's fields.
pub const FIELDS: [(&str, &[&str]); 18] = [
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
            "recall",
            "summon",
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
            "xp",
            "xp_share",
            "xp_share_level",
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
        if matches!(kind.as_str(), "object" | "camp") && landblock.is_none() {
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

fn read_row(kind: &str, f: &mut Fields<'_>) -> Option<Row> {
    Some(match kind {
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
                recall: f.bool("recall").unwrap_or(false),
                summon: f.bool("summon").unwrap_or(false),
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
                prov: f.req_prov()?,
            })
        }
        "loot" => Row::Loot(Loot {
            profile: f.req_u32("profile")?,
            profile_name: f.ident("profile_name"),
            counts: f.req_floats("counts")?,
            items: f.weighted("items"),
            extras: f.chances("extras"),
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
            xp: f.int("xp").and_then(|v| u64::try_from(v).ok()),
            xp_share: f.float("xp_share"),
            xp_share_level: f.int("xp_share_level").and_then(|v| i32::try_from(v).ok()),
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
                other => {
                    f.bad(
                        "update",
                        &format!("{other:?} is not bestow, advance or complete"),
                    );
                    return None;
                }
            };
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
                requires: f.requires("requires"),
                takes: f.ids("takes"),
                to: f.symbol("to"),
                races: f.ids("races"),
                effects_last: f.bool("effects_last").unwrap_or(false),
                fellows: f.bool("fellows").unwrap_or(false),
                wants: f.u32("wants"),
                prov: f.req_prov()?,
            })
        }
        "give" => Row::Give(Give {
            key: f.req_symbol("key")?,
            to: f.req_id("to")?,
            item: f.id("item"),
            wants: f.u32("wants"),
            gives: f.ids("gives"),
            counts: f.ids("counts"),
            effects: f.ids("effects"),
            text: f.strref("text"),
            fx: f.id("fx"),
            requires: f.requires("requires"),
            prov: f.req_prov()?,
        }),
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
            "one of { use = T }, { use = ITEM, on = TARGET }, { give = ITEM, to = TARGET }, { arrive = T }, { take = T }, { kill = T }, { done = Q }, { given = Q }",
            |v| {
                let t = v.as_table()?;
                if let (2, Some(item), Some(on)) = (t.len(), t.get("use"), t.get("on")) {
                    return Some(Trigger::UseOn(as_u32(item)?, as_u32(on)?));
                }
                if let (2, Some(item), Some(to)) = (t.len(), t.get("give"), t.get("to")) {
                    return Some(Trigger::Give(as_u32(item)?, as_u32(to)?));
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
            "{ quest, done = true }, { quest, underway = true } or { quest, phase }",
            |v| {
                let t = v.as_table()?;
                if t.keys()
                    .any(|k| !["quest", "done", "underway", "phase"].contains(&k.as_str()))
                {
                    return None;
                }
                let quest = as_u32(t.get("quest")?)?;
                let gate = match (t.get("done"), t.get("underway"), t.get("phase")) {
                    (Some(d), None, None) if d.as_bool() == Some(true) => Gate::Done,
                    (None, Some(u), None) if u.as_bool() == Some(true) => Gate::UnderWay,
                    (None, None, Some(p)) => Gate::Phase(as_u32(p).filter(|p| *p > 0)?),
                    _ => return None,
                };
                Some((quest, gate))
            },
        )
    }

    /// `[{ quest = Q, phase = N }, { quest = Q, done = true }, ...]`.
    fn requires(&mut self, name: &str) -> Vec<(u32, Need)> {
        self.list_of(
            name,
            "a list of { quest, phase }, { quest, done = true } or { quest, free = true }",
            &["quest", "phase", "done", "free"],
            |t| {
                let quest = as_u32(t.get("quest")?)?;
                let need = match (t.get("phase"), t.get("done"), t.get("free")) {
                    (Some(p), None, None) => Need::Phase(as_u32(p)?),
                    (None, Some(d), None) if d.as_bool() == Some(true) => Need::Done,
                    (None, None, Some(f)) if f.as_bool() == Some(true) => Need::Free,
                    _ => return None,
                };
                Some((quest, need))
            },
        )
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
