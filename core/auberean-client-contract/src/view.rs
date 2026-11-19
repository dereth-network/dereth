//! The game as a front end reads it: one owned [`View`], which the runtime keeps current, a part
//! at a time, from the model and the player's scripts.
//!
//! Every part is plain data: objects by id, text written out (and, where an interface may write it
//! again once the string tables it reads come, its reference as a [`TextRef`]), numbers as the
//! scripts answered them. A part the runtime asks of the scripts carries a `version` that changes
//! whenever it is asked again, so a reader rebuilds what it draws from it only then.
//!
//! The view is refreshed when the host asks (once a frame, before its interface reads it). What the
//! player's scripts tell the panels marks the parts they change, and those are asked again at the
//! next refresh after the one that heard them, with the parts any action may have changed; two
//! parts are asked only when a front end asks for them ([`crate::Action::AskTargets`],
//! [`crate::Action::AskSavedPlaces`]).

use std::collections::BTreeMap;

use crate::notice::{Allegiance, Fellowship, FriendState};
use crate::question::Question;
use crate::session::{CharacterList, Heartbeat, LoginProgress, SessionEnd};
use crate::text::TextRef;
use crate::UiLocation;

/// The game as the runtime last refreshed it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct View {
    /// How many times it was refreshed.
    pub refreshes: u64,
    pub pregame: Pregame,
    /// The connection as last measured.
    pub link: Option<Heartbeat>,
    /// Seconds from when the world's server was last heard to the refresh, on the client's clock;
    /// none with no connection to it.
    pub since_heard: Option<f64>,
    /// The machine's time of day at the refresh: the hour (0 to 23) and the minute; none when the
    /// host has no clock.
    pub local_clock: Option<(u8, u8)>,
    /// The player's id, once the server named it (its object may not be here yet).
    pub player_id: Option<u64>,
    /// The player, once its object is here.
    pub player: Option<Player>,
    /// The game's clocks.
    pub clock: Clock,
    /// What the player carries and wears, as the scripts answer for it.
    pub inventory: Option<Inventory>,
    /// The shortcut bar, as the scripts answer for it.
    pub toolbar: Option<Toolbar>,
    /// The effects on the player.
    pub effects: Option<Effects>,
    /// The skill window's panels and skills.
    pub skills: Option<Skills>,
    /// The crafting page's skills and recipes, and the recipe opened.
    pub craft: Option<Craft>,
    /// What the shop windows read.
    pub store: Option<Store>,
    /// The story's acts and their scenes; none until the scripts list them.
    pub acts: Option<Acts>,
    /// The objects around the player and what the panels read of them; the selection.
    pub world: Option<Surroundings>,
    /// The player's options.
    pub options: Option<GameOptions>,
    /// The panels' places the player's interface keeps, when asked for
    /// ([`crate::Action::AskSavedPlaces`]): at the refresh after the ask, and none at the next.
    pub saved_places: Option<Vec<UiLocation>>,
    /// The chat service refused the login's ticket: the account is banned (or the ticket could
    /// not be had).
    pub chat_service_refused: Option<ChatServiceRefusal>,
    /// How the server's calls to the player's scripts went so far this session.
    pub calls: ServerCalls,
    /// The game's questions to the player still open, in the order they were asked.
    pub questions: Vec<Question>,
    /// The player's quests as the journal lists them; none until the player is here.
    pub journal: Option<Journal>,
    /// The markers the player's scripts put on the map while the player is in the world.
    pub markers: MapMarkers,
    /// The player's fellowship, allegiance, friends and kingdom, as the scripts last told them.
    pub social: Social,
    /// The player's pets, in the order they came.
    pub pets: Vec<Pet>,
}

/// One of the player's pets, as the player's scripts last told of it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pet {
    pub id: u64,
    /// Its name as given (with its owner's part: [`crate::pet::shown_name`] shows it without).
    pub name: Option<TextRef>,
    /// Its icon, a picture of the game's records (0 for none).
    pub icon: u32,
    /// Its mode ([`crate::pet::mode`]).
    pub mode: u32,
}

/// The player's quests as the journal reads them from the player's scripts: once the player is
/// here, and again whenever the scripts say the journal changed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Journal {
    /// Changed each time the quests are read again.
    pub version: u64,
    /// Every quest the player has, under way or finished, newest given first.
    pub quests: Vec<crate::notice::Quest>,
}

/// A marker on the map: its kind, the object it stands for (0 for none), the land cell it marks,
/// its picture, its words.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MapMarker {
    pub kind: u32,
    pub object: u64,
    pub cell: u32,
    pub picture: u32,
    pub label: Option<TextRef>,
}

/// The markers on the map, as the map keeps them from what the player's scripts say: each kind's
/// in the order they were put, at most [`MapMarkers::MOST_OF_A_KIND`] of a kind.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MapMarkers {
    /// Changed each time a marker is put, taken off or cleared.
    pub version: u64,
    pub markers: Vec<MapMarker>,
}

impl MapMarkers {
    /// How many markers of a kind the map keeps: the oldest goes when another comes.
    pub const MOST_OF_A_KIND: usize = 10;

    /// The kind of marker the scripts never put: the map's own labels.
    pub const LABELS: u32 = 3;

    /// A marker put, on a land of `land` cells across and up: refused when it is of the labels'
    /// kind, or when one of its kind already stands within two and a half cells of it; else the
    /// kind's oldest go beyond the tenth, a marker for the same object is replaced, and it is put.
    /// Whether it was put.
    // D2-02582, D2-02579
    pub fn add(&mut self, marker: MapMarker, land: (i32, i32)) -> bool {
        if marker.kind == Self::LABELS {
            return false;
        }
        let at = |cell: u32| auberean_primitives::land::land_coords(cell, land);
        if let Some((x, y)) = at(marker.cell) {
            #[allow(clippy::cast_precision_loss)]
            let near = self
                .markers
                .iter()
                .filter(|m| m.kind == marker.kind)
                .filter_map(|m| at(m.cell))
                .any(|(mx, my)| {
                    let (dx, dy) = ((x - mx) as f32, (y - my) as f32);
                    dx * dx + dy * dy < 6.0
                });
            if near {
                return false;
            }
        }
        while self
            .markers
            .iter()
            .filter(|m| m.kind == marker.kind)
            .count()
            >= Self::MOST_OF_A_KIND
        {
            let Some(i) = self.markers.iter().position(|m| m.kind == marker.kind) else {
                break;
            };
            self.markers.remove(i);
        }
        if marker.object != 0 {
            self.markers
                .retain(|m| !(m.kind == marker.kind && m.object == marker.object));
        }
        self.markers.push(marker);
        self.version += 1;
        true
    }

    /// The marker of a kind for an object taken off; whether there was one.
    // D2-02580
    pub fn remove(&mut self, kind: u32, object: u64) -> bool {
        let Some(i) = self
            .markers
            .iter()
            .position(|m| m.kind == kind && m.object == object)
        else {
            return false;
        };
        self.markers.remove(i);
        self.version += 1;
        true
    }

    /// Every marker of a kind taken off.
    // D2-02578
    pub fn clear(&mut self, kind: u32) {
        let before = self.markers.len();
        self.markers.retain(|m| m.kind != kind);
        if self.markers.len() != before {
            self.version += 1;
        }
    }

    /// Every marker taken off, as the player leaves the world.
    pub fn clear_all(&mut self) {
        if !self.markers.is_empty() {
            self.markers.clear();
            self.version += 1;
        }
    }
}

/// How many of the server's calls the player's scripts ran, and how many of those ran to their
/// end.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ServerCalls {
    pub run: u64,
    pub completed: u64,
}

/// Why the chat service refused the login's ticket.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChatServiceRefusal {
    /// The ticket could not be had, or was refused.
    Failed,
    /// The account is banned.
    Banned,
}

/// The screens before the world: the account's characters, the server's answers about one, how
/// the login goes, the world's name.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pregame {
    /// The account's characters, as the server last listed them.
    pub characters: Option<CharacterList>,
    /// The server's last answer to a character made: counted, so each answer is told once.
    pub creation: Option<CreationAnswer>,
    /// The server's last refusal of an entry into the world (or end of the session in it):
    /// counted.
    pub refused: Option<Refusal>,
    /// How the login stands, while connecting.
    pub login: Option<LoginProgress>,
    /// How the session stands under the player: a connection failed or let go, the login failed,
    /// the server gone.
    pub state: SessionEnd,
    /// The data download's events so far this session, in order: each event and its value.
    pub download: Vec<(u32, u32)>,
    /// The world's name, as the login named it (empty before it does).
    pub world_name: String,
}

/// The server's answer to a character made.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CreationAnswer {
    /// The answer (1: made).
    pub response: u32,
    /// The character it is about.
    pub character: u64,
    /// How many answers have come.
    pub count: u32,
    /// The error, when it was not made.
    pub error: u32,
}

/// The server refused an entry into the world, or ended the session in it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Refusal {
    /// Why (11: the character is still in the world; 20: booted).
    pub reason: u32,
    /// How many refusals have come.
    pub count: u32,
    /// The server's own line, when it gave one.
    pub text: Option<TextRef>,
}

/// The player.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Player {
    pub id: u64,
    /// The cell the player's object stands in, when it has a place.
    pub cell: Option<u32>,
    /// Health, greatest health, vigor, greatest vigor, where the qualities have them.
    pub vitals: Option<[u32; 4]>,
    /// The player's race.
    pub race: Option<i32>,
    /// The player's name, as its repository answers it.
    pub name: Option<String>,
    /// The world the player plays on, by its name (empty when the client has none).
    pub world_name: String,
    /// The player's body: its cell, its place there and its heading (degrees).
    pub body: Body,
    /// The player rides a mount, as its scripts last said.
    pub mounted: bool,
    /// The player's level, its experience and its vitae.
    pub progress: Progress,
    /// The player is a hero, as its qualities hold it: only a hero has focus.
    pub hero: bool,
    /// The player's focus and its greatest, as its qualities hold them (the greatest a hundred
    /// and what the qualities add to it); none while the server has sent no focus.
    pub focus: Option<[u32; 2]>,
}

/// The player's level, its experience and its vitae, as its qualities hold them; each none while
/// the server has sent none.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Progress {
    pub level: Option<u32>,
    /// The experience earned in all, and the experience the player has to spend.
    pub total_experience: Option<u64>,
    pub available_experience: Option<u64>,
    /// The vitae, a percentage of the player's greatest health and vigor kept (none: full, 100),
    /// and the experience still to earn for its next point back.
    pub vitae: Option<f32>,
    pub vitae_experience: Option<u64>,
}

impl Progress {
    /// The vitae, 100 where the player has none recorded.
    #[must_use]
    pub fn vitae_or_full(&self) -> f32 {
        self.vitae.unwrap_or(100.0)
    }
}

/// Where a body stands: its cell, its place in the cell's block and its heading (degrees).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Body {
    pub cell: u32,
    pub origin: [f32; 3],
    pub heading: f32,
    /// It is moving.
    pub moving: bool,
}

/// The game's clocks, in seconds.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Clock {
    /// The world's time, which the game's date and time of day are read from.
    pub world_time: f64,
    /// The game's time, which waits and reset times are measured on.
    pub game_time: f64,
}

/// An object's icon as its look makes it: each layer's picture and the color it is tinted by.
pub type IconLayers = Vec<(u32, u32)>;

/// What the player carries and wears.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Inventory {
    pub version: u64,
    pub player: u64,
    /// The containers the player holds open, by id.
    pub containers: BTreeMap<u64, Container>,
    /// The player's pack's segments: each its first slot and how many.
    pub segments: Vec<(u32, u32)>,
    /// What is worn, by place.
    pub worn: Vec<(u32, u64)>,
    pub items: BTreeMap<u64, Item>,
    pub money: i32,
    /// The money bag's stack, when it is one.
    pub coin_stack: u32,
    /// Melting asks no question.
    pub no_melt_question: bool,
    pub selection: u64,
    /// What the scripts last named an amount of an item: the item, the amount, the name.
    pub amount_name: Option<(u64, u32, String)>,
}

/// An open container: its size, its segments and its slots.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Container {
    pub size: u32,
    pub segments: Vec<Contents>,
    pub slots: Vec<(u32, u32)>,
}

/// A segment of a container: its items, its first slot and how many slots.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Contents {
    pub items: Vec<u64>,
    pub first: u32,
    pub count: u32,
}

/// An item, as the inventory panels read it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Item {
    pub quantity: u32,
    pub icon: u32,
    pub icon_layers: IconLayers,
    pub stackable: bool,
    pub stack_size: u32,
    pub name: String,
    /// Where it is worn and how (place, priority).
    pub wear: Option<(u32, u32)>,
    pub preferred_location: u32,
    pub had: bool,
    pub worn: bool,
    pub melts_at_once: bool,
    /// Its name as a trade offers it.
    pub offered_name: Option<String>,
    /// The record it was made from.
    pub data_id: u32,
}

/// The shortcut bar.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Toolbar {
    pub version: u64,
    pub shortcuts: Vec<Shortcut>,
    /// Each slot's answer, tooltip and icon.
    pub slots: Vec<ToolbarSlot>,
    /// The set of ten shown (from 1).
    pub set: u32,
    pub show_numbers: bool,
    pub height: i32,
    pub locked: bool,
    pub insta_scroll: bool,
    pub skills: Vec<ToolbarSkill>,
    pub items: Vec<ToolbarItem>,
}

/// A shortcut: its kind and the datum the kind uses.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Shortcut {
    pub kind: u32,
    pub item: u64,
    pub data_enum: u32,
    pub data_id: u32,
    pub text: String,
}

/// A toolbar slot's answer, tooltip and icon.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ToolbarSlot {
    pub answer: u32,
    pub tooltip: String,
    pub icon: u32,
}

/// A skill a shortcut uses: trained, usable now, and how long it waits (all and left).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ToolbarSkill {
    pub skill: u32,
    pub trained: bool,
    pub usable: bool,
    pub waiting: Option<(f32, f32)>,
}

/// An item a shortcut uses: whether the player has it, and its name and icon when it is here.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ToolbarItem {
    pub item: u64,
    pub have: bool,
    pub here: Option<(String, u32)>,
    pub icon_layers: IconLayers,
}

/// The effects on the player.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Effects {
    pub version: u64,
    /// The game's time when they were listed.
    pub now: f64,
    pub effects: Vec<Effect>,
}

/// An effect on the player.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Effect {
    pub id: u32,
    pub name: String,
    pub icon: u32,
    /// When it was cast and how long it lasts, game seconds (negative: for good).
    pub time_cast: f64,
    pub duration: f64,
    pub pulsed: bool,
}

/// The skill window's panels and skills.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Skills {
    pub version: u64,
    /// The panel shown and its skills.
    pub panel: u32,
    pub nodes: Vec<SkillNode>,
    /// Each panel's name and icon.
    pub panels: BTreeMap<u32, (TextRef, u32)>,
    pub states: BTreeMap<u32, SkillState>,
    /// The race's root panels.
    pub roots: [Option<u32>; 4],
    pub experience: u64,
    pub credits: u32,
    pub hero_credits: u32,
}

/// The kinds of a skill panel's nodes ([`SkillNode::kind`]): a skill, or a node that opens
/// another panel; and the node that goes back to the panel visited before.
pub mod skill_node {
    pub const SKILL: u32 = 0x4000_0001;
    pub const PLUS: u32 = 0x4000_0002;
    pub const MINUS: u32 = 0x4000_0003;
    /// The node that goes back (its `skill`).
    pub const BACK: u32 = 0x8000_004B;
}

/// A skill on a panel: the skill or the panel it stands for, its kind ([`skill_node`]), its
/// place in the tree (column and row), the nodes it hangs from.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SkillNode {
    pub skill: u32,
    pub kind: u32,
    pub name: TextRef,
    pub description: TextRef,
    pub icon: u32,
    pub x: i32,
    pub y: i32,
    pub parents: Vec<u32>,
    pub dependencies: Vec<u32>,
    pub passive: bool,
    pub untrainable: bool,
    pub perk: bool,
}

/// What the player has of a skill.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SkillState {
    pub trained: bool,
    pub being_untrained: bool,
    pub hero: bool,
    pub can_train: u32,
    pub can_untrain: u32,
    pub can_raise: u32,
    pub cost_to_train: u32,
    pub experience_to_raise: u64,
    pub experience: u64,
    pub base_level: u32,
    pub level: u32,
}

/// The crafting page's skills and recipes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Craft {
    pub version: u64,
    pub skills: Vec<CraftSkillFacts>,
    pub experience: u64,
    /// The categories' names.
    pub categories: BTreeMap<u32, TextRef>,
    /// The recipe the ingredient panel has open, at its spinner value.
    pub opened: Option<RecipeAt>,
}

/// A craft skill as listed and as the player has it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CraftSkillFacts {
    pub listed: CraftSkill,
    pub own: Option<CraftSkill>,
    pub raw_level: u32,
    pub level: u32,
    pub raiseable: bool,
    pub can_raise: bool,
    pub recipes: Vec<Recipe>,
}

/// A craft skill.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CraftSkill {
    pub did: u32,
    pub name: TextRef,
    pub level: u32,
    pub raw_level: u32,
    pub cost_to_raise: u64,
    pub icon: u32,
    pub usage_xp_mod: f32,
}

/// A recipe as the crafting page lists it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Recipe {
    pub did: u32,
    pub name: TextRef,
    pub icon: u32,
    pub shown: bool,
    pub craft_skill: u32,
    pub category: u32,
    pub time_use_again: f64,
}

/// A recipe at a spinner value, as the ingredient panel reads it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RecipeAt {
    pub recipe: u32,
    pub spinner: i32,
    pub name: TextRef,
    pub charges: i32,
    pub min_spinner: i32,
    pub max_spinner: i32,
    pub last_spinner: i32,
    pub auto_populating: bool,
    pub max_ingredients: u32,
    pub name_coloring: bool,
    pub name_color: u32,
    pub difficulty: u32,
    pub dynamic_difficulty: bool,
    pub cutoff: Option<u32>,
    pub cost: i32,
    pub dynamic_cost: bool,
    pub craft_skill: u32,
    pub read_only_spinner: bool,
    pub skill_level: u32,
    pub mask: u32,
    pub ingredients: Vec<Ingredient>,
    pub spinner_cost: Option<i32>,
    pub spinner_difficulty: Option<(u32, u32)>,
    pub auto_populate: BTreeMap<u32, bool>,
    pub items: Vec<u64>,
    pub fits: BTreeMap<(u32, u64), bool>,
}

/// An ingredient of a recipe.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ingredient {
    pub id: u32,
    pub description: TextRef,
}

/// What the shop windows read.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Store {
    pub version: u64,
    /// The most of each item that may be bought.
    pub max_purchase: BTreeMap<u64, u32>,
    pub consignment_charge: f32,
}

/// The story's acts.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Acts {
    pub version: u64,
    pub acts: Vec<Act>,
}

/// An act of the story: its name, and its scenes in their order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Act {
    pub id: u32,
    pub name: TextRef,
    pub visible: bool,
    pub scenes: Vec<Scene>,
}

/// A scene of an act.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scene {
    pub id: u32,
    pub name: TextRef,
    pub hidden: bool,
    pub playable: bool,
}

/// The objects around the player and what the panels read of them, and the selection.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Surroundings {
    pub player: u64,
    /// Every object standing in the world, or hanging from a body there.
    pub things: Vec<Thing>,
    pub selection: Selection,
    /// The radar's two ranges, when the scripts gave them.
    pub radar_ranges: Option<(f32, f32)>,
    /// The player stands in an interior cell (a building, a dungeon), where the radar reaches
    /// the shorter of its ranges.
    pub inside: bool,
    /// The player's map coordinates outdoors, east and north (each -100 to 100 from the world's
    /// middle, as the radar's place line gives them to a tenth); none inside, or with no world's
    /// size to measure by.
    pub map_coordinates: Option<(f32, f32)>,
    /// The radar shows the map coordinates; names show over heads.
    pub show_coordinates: bool,
    pub show_names: bool,
    /// The player is in portal space.
    pub portal_space: bool,
    /// The last object that struck the player (0: none).
    pub last_attacker: u64,
    /// The fellows' shares of the experience.
    pub fellow_xp: Vec<(u64, f32)>,
}

/// The kind bits of an object ([`Thing::kind`]).
pub mod kind {
    /// Items: either pattern.
    pub const ITEM: [u32; 2] = [0x81, 0x103];
    /// Players.
    pub const PLAYER: u32 = 0x1F;
    /// Creatures that are not players.
    pub const CREATURE: u32 = 0x4F;

    /// Whether kind bits `kind` are a player's or a creature's, not an item's.
    #[must_use]
    pub const fn is_being(kind: u32) -> bool {
        if kind & ITEM[0] == ITEM[0] || kind & ITEM[1] == ITEM[1] {
            return false;
        }
        kind & PLAYER == PLAYER || kind & CREATURE == CREATURE
    }
}

/// The radar's ranges, inside and outdoors, in meters, when the player's scripts give none.
pub const RADAR_RANGES: (f32, f32) = (50.0, 75.0);

impl Surroundings {
    /// How far the radar reaches where the player stands, in meters: the shorter of its ranges
    /// inside.
    #[must_use]
    pub fn radar_range(&self) -> f32 {
        let (inside, outside) = self.radar_ranges.unwrap_or(RADAR_RANGES);
        if self.inside {
            inside
        } else {
            outside
        }
    }
}

/// The side of the world's blocks, in meters: a cell's place is in its block's frame.
pub const BLOCK_SIDE: f32 = 160.0;

/// Where a place stands from another, each a cell and a place in the cell's block, in meters: east,
/// north and up, across blocks.
#[must_use]
pub fn offset(from: (u32, [f32; 3]), to: (u32, [f32; 3])) -> [f32; 3] {
    let global = |(cell, o): (u32, [f32; 3])| {
        let block = |b: u32| f32::from(u8::try_from(b & 0xFF).unwrap_or(0)) * BLOCK_SIDE;
        [block(cell >> 24) + o[0], block(cell >> 16) + o[1], o[2]]
    };
    let (a, b) = (global(from), global(to));
    [b[0] - a[0], b[1] - a[1], b[2] - a[2]]
}

/// The shapes an object shows as on the radar ([`Thing::radar`]), as the player's scripts name
/// them; each is a few points around the object's place.
pub mod radar_shape {
    /// One point.
    pub const POINT: u32 = 0x4000_0001;
    /// A filled square of three by three.
    pub const FILL: u32 = 0x4000_0002;
    /// A plus: the point and its four sides.
    pub const CROSS: u32 = 0x4000_0003;
    /// An X: the point and its four corners.
    pub const X: u32 = 0x4000_0004;
    /// A hollow square: the sides and the corners, not the point.
    pub const HOLLOW: u32 = 0x4000_0005;
    /// An L: the left column and the point below the middle.
    pub const L: u32 = 0x4000_0006;
    /// The four sides alone.
    pub const EDGES: u32 = 0x4000_0007;
    /// The four corners alone.
    pub const CORNERS: u32 = 0x4000_0008;
    /// A T: the top row and the middle column.
    pub const T: u32 = 0x4000_0009;
    /// A face: two eyes and a mouth.
    pub const FACE: u32 = 0x4000_000A;

    /// The points of `shape` around the object's place, each across and down (down the screen),
    /// one point apart; none for a shape not named here.
    #[must_use]
    pub const fn points(shape: u32) -> &'static [(i8, i8)] {
        const SIDES: [(i8, i8); 4] = [(0, -1), (0, 1), (-1, 0), (1, 0)];
        const CORNER: [(i8, i8); 4] = [(1, 1), (-1, -1), (-1, 1), (1, -1)];
        match shape {
            POINT => &[(0, 0)],
            FILL => &[
                (0, 0),
                SIDES[0],
                SIDES[1],
                SIDES[2],
                SIDES[3],
                CORNER[0],
                CORNER[1],
                CORNER[2],
                CORNER[3],
            ],
            CROSS => &[(0, 0), SIDES[0], SIDES[1], SIDES[2], SIDES[3]],
            X => &[(0, 0), CORNER[0], CORNER[1], CORNER[2], CORNER[3]],
            HOLLOW => &[
                SIDES[0], SIDES[1], SIDES[2], SIDES[3], CORNER[0], CORNER[1], CORNER[2], CORNER[3],
            ],
            L => &[(0, 1), (-1, -1), (-1, 0), (-1, 1)],
            EDGES => &SIDES,
            CORNERS => &CORNER,
            T => &[(-1, -1), (0, -1), (1, -1), (0, 0), (0, 1)],
            FACE => &[(-1, 1), (0, 1), (1, 1), (-1, -1), (1, -1)],
            _ => &[],
        }
    }
}

/// The selection: what is selected, what was before it, and the player's auto-target settings.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    pub current: u64,
    pub last: u64,
    /// The player targets what attacks it.
    pub auto_target: bool,
    /// The scripts would target something now.
    pub should_auto_target: bool,
}

/// An object around the player.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Thing {
    pub id: u64,
    /// The record it was made from.
    pub data_id: u32,
    /// Its cell and place there, when it has one.
    pub place: Option<(u32, [f32; 3])>,
    /// The body it hangs from (a wielded weapon), when it does.
    pub parent: Option<u64>,
    /// Its name as its scripts write it, and the reference they gave, when they gave one.
    pub name: Option<String>,
    pub name_ref: Option<TextRef>,
    /// The name its description carries, which shows when its scripts give none.
    pub own_name: TextRef,
    /// Its kind bits.
    pub kind: u32,
    pub groove: i8,
    /// A pet's owner, what holds an item.
    pub owner: Option<u64>,
    pub container: Option<u64>,
    pub selectable: bool,
    pub dead: bool,
    /// How it shows on the radar: its color and shape (none: not on it).
    pub radar: Option<([f32; 4], u32)>,
    /// A fellow, may be attacked, its owner a fellow: as last asked ([`crate::Action::AskTargets`]).
    pub fellow: bool,
    pub attackable: bool,
    pub owner_fellow: bool,
}

/// The player's options.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GameOptions {
    /// Changes each time they are read again.
    pub version: u64,
    /// The yes-or-no options.
    pub bits: u32,
    pub radar_mask: u32,
    pub shortcut_height: u32,
    /// How far away others' damage shows (negative: everywhere).
    pub damage_range: f32,
    /// The panels' places kept with them (none when the table kept does not read).
    pub ui_locations: Option<Vec<UiLocation>>,
    pub chat: ChatOptions,
}

impl GameOptions {
    /// Whether a yes-or-no option is on, as its bit in the options (a radar filter's in the
    /// radar's mask) says; none for an option that is a number.
    #[must_use]
    pub const fn flag(&self, option: crate::PlayerOption) -> Option<bool> {
        use crate::PlayerOption as O;
        // Each option's bit, the radar's mask's for a radar filter, and whether the bit is set
        // when the option is off.
        let (bit, radar, opposite) = match option {
            O::ShortcutNumbers => (0x1, false, false),
            O::DamageNumbers => (0x4, false, false),
            O::OthersDamageNumbers => (0x8, false, false),
            O::RadarCoordinates => (0x10, false, false),
            O::AutoTarget => (0x20, false, false),
            O::TakeAllegianceOffers => (0x40, false, false),
            O::TakeFellowshipOffers => (0x80, false, false),
            O::TakeTradeOffers => (0x100, false, false),
            O::AskBeforeMelting => (0x400, false, true),
            O::ShortcutLocked => (0x800, false, false),
            O::PlayerKillerDamage => (0x1000, false, false),
            O::ShortcutInstantScroll => (0x2000, false, false),
            O::LatencyGraph => (0x4000, false, false),
            O::FellowsOnMap => (0x8000, false, true),
            O::PetPanel => (0x1_0000, false, true),
            O::ClassicExamination => (0x2_0000, false, false),
            O::RadarMonsters => (0x1, true, false),
            O::RadarPeople => (0x2, true, false),
            O::RadarPlayers => (0x4, true, false),
            O::RadarPortals => (0x8, true, false),
            O::RadarLifestones => (0x10, true, false),
            O::RadarItems => (0x20, true, false),
            O::OthersDamageRange | O::ShortcutHeight => return None,
        };
        let word = if radar { self.radar_mask } else { self.bits };
        Some((word & bit != 0) != opposite)
    }
}

/// The chat's part of the player's options.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChatOptions {
    /// Each chat window's filter (the kinds of text it shows), windows 1 to 4.
    pub filters: [u32; 4],
    /// The channel each window is devoted to.
    pub devoted: [u32; 4],
    /// Each kind of text's font size, font color and whether it shows a hidden window: by kind,
    /// a bit each.
    pub fonts: BTreeMap<u32, (u32, u32, bool)>,
    /// Each window's opacity, once set: by its element name's token.
    pub opacities: BTreeMap<u32, f32>,
}

/// The player's fellowship, allegiance, friends and kingdom, kept from what the player's scripts
/// tell the panels (and from the player's description, for the kingdom) while the player is in the
/// world.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Social {
    /// The player's fellowship as its notices last left it: its fellows in the order the scripts
    /// gave them (a fellow described anew keeps its place, one new to it goes last), each fellow's
    /// health, vigor, level and cell as last told; none while the player is in none.
    pub fellowship: Option<Fellowship>,
    /// The allegiance's profile last told, about the member last asked for
    /// ([`crate::SocialAction::QueryAllegiance`]; the scripts tell one only while one is asked
    /// for); a profile about no member (its member's id 0) says the player is in no allegiance.
    pub allegiance: Option<Allegiance>,
    /// The chat service's friends, each with its state (mode 0: offline), in the order the service
    /// listed them; a friend whose state moves it between online and offline goes last.
    pub friends: Vec<FriendState>,
    /// The player's own state on the chat service, once told ([`friend_mode`]).
    pub own_state: Option<u32>,
    /// The player's kingdom, as its description names it ([`kingdom`]); none for a player in no
    /// kingdom.
    pub kingdom: Option<u32>,
    /// The kind of conflict between players the place allows, once told ([`conflict`]).
    pub conflict: Option<u32>,
}

/// A friend's mode on the chat service, as bits: available, away from the keyboard, not to be
/// disturbed; 0 is offline. The player sets its own state to one of them.
pub mod friend_mode {
    pub const AVAILABLE: u32 = 1;
    pub const AWAY: u32 = 2;
    pub const BUSY: u32 = 4;
}

/// The kingdoms, as a player's description names them.
pub mod kingdom {
    pub const SHADOW: u32 = 1;
    pub const ORDER: u32 = 2;
    pub const DOMINION: u32 = 4;
    pub const NEUTRAL: u32 = 0x8000_0000;
}

/// The kinds of conflict between players a place allows.
pub mod conflict {
    pub const PEACE: u32 = 1;
    /// Players of different kingdoms, neutral players kept out.
    pub const CONTROLLED: u32 = 2;
    pub const KINGDOM_AGAINST_KINGDOM: u32 = 3;
    pub const FREE_FOR_ALL: u32 = 4;
    /// Kingdom against kingdom, even where it is otherwise peace.
    pub const KINGDOM_AGAINST_KINGDOM_ANYWHERE: u32 = 5;
}
