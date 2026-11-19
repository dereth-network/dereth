//! What the places are built from: a world database's weenies, where it places them, its events
//! and its named landmarks, as [`WorldSource`] gives them.
//!
//! A world database is the content a server loads: every kind of object (a weenie, by its class
//! id), the instances it places in the world (each where it stands, linked to the instances it
//! spawns or names), the events that switch spawners on and off, and the landmarks its operators
//! may be sent to by name. A server reads its own; a launcher reads the public one of a world's
//! era ([`crate::sql`]).

use std::collections::HashMap;
use std::sync::Arc;

/// The world database's numbers for the properties and kinds the places are read from.
pub mod id {
    /// Kinds of weenie.
    pub mod kind {
        pub const GENERIC: u32 = 1;
        pub const PORTAL: u32 = 7;
        pub const CREATURE: u32 = 10;
        pub const VENDOR: u32 = 12;
        pub const COW: u32 = 15;
        pub const DOOR: u32 = 19;
        pub const LIFESTONE: u32 = 25;
    }
    /// Integer properties.
    pub mod int {
        pub const ITEM_TYPE: u32 = 1;
        /// How the object may be used: `1` for not at all.
        pub const ITEM_USEABLE: u32 = 16;
        pub const ACCOUNT_REQUIREMENTS: u32 = 26;
        pub const MIN_LEVEL: u32 = 86;
        pub const MAX_LEVEL: u32 = 87;
        pub const PORTAL_BITMASK: u32 = 111;
        pub const GENERATOR_TIME_TYPE: u32 = 142;
        pub const GENERATOR_START_TIME: u32 = 143;
        pub const GENERATOR_END_TIME: u32 = 144;
        pub const LIFESPAN: u32 = 267;
        /// The object's physics state word.
        pub const PHYSICS_STATE: u32 = 93;
    }
    /// Boolean properties.
    pub mod bool {
        pub const ATTACKABLE: u32 = 19;
        pub const UI_HIDDEN: u32 = 24;
        pub const ADVOCATE_QUEST: u32 = 43;
    }
    /// Floating-point properties.
    pub mod float {
        pub const DEFAULT_SCALE: u32 = 39;
        pub const GENERATOR_RADIUS: u32 = 43;
    }
    /// Data id properties.
    pub mod did {
        pub const SETUP: u32 = 1;
    }
    /// String properties.
    pub mod string {
        pub const NAME: u32 = 1;
        pub const GENERATOR_EVENT: u32 = 34;
        pub const QUEST_RESTRICTION: u32 = 37;
    }
    /// Kinds of position.
    pub mod position {
        pub const DESTINATION: u32 = 2;
        pub const RELATIVE_DESTINATION: u32 = 26;
    }
}

/// A point in the world: a cell, a point in its landblock's frame and a rotation (`w`, `x`, `y`,
/// `z`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spot {
    pub cell: u32,
    pub origin: [f32; 3],
    pub rotation: [f32; 4],
}

impl Spot {
    /// `cell` at `origin`, unrotated.
    #[must_use]
    pub const fn at(cell: u32, origin: [f32; 3]) -> Self {
        Self {
            cell,
            origin,
            rotation: [1.0, 0.0, 0.0, 0.0],
        }
    }

    /// Whether the cell is a dungeon's or a building's (an interior cell), not the land's.
    #[must_use]
    pub const fn indoors(&self) -> bool {
        self.cell & 0xFFFF >= 0x100
    }

    /// The landblock, `0xXXYY`.
    #[must_use]
    pub const fn landblock(&self) -> u32 {
        self.cell >> 16
    }
}

/// One thing a spawner makes: what, how likely, and where.
#[derive(Debug, Clone, PartialEq)]
pub struct Spawn {
    /// The weenie it makes.
    pub wcid: u32,
    /// The chance it is picked; `-1` for every one of its spawner's entries made each time.
    pub probability: f32,
    /// How many it makes at first, and at most.
    pub init_create: i32,
    pub max_create: i32,
    /// What makes it again ([`when`]).
    pub when_create: u32,
    /// Where it appears ([`spawn_where`]).
    pub where_create: u32,
    /// The point it names: a cell of `0` means none, and the point is then an offset from the
    /// spawner.
    pub at: Spot,
}

/// Where a spawned thing appears, as bits of [`Spawn::where_create`].
pub mod spawn_where {
    pub const ON_TOP: u32 = 0x01;
    pub const SCATTER: u32 = 0x02;
    pub const SPECIFIC: u32 = 0x04;
    pub const CONTAIN: u32 = 0x08;
    pub const WIELD: u32 = 0x10;
    pub const SHOP: u32 = 0x20;
    pub const TREASURE: u32 = 0x40;
}

/// What makes a spawned thing again, as bits of [`Spawn::when_create`].
pub mod when {
    pub const DESTRUCTION: u32 = 0x1;
    pub const PICK_UP: u32 = 0x2;
    pub const DEATH: u32 = 0x4;
}

/// A kind of object, with the properties the places are read from.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Weenie {
    pub class_id: u32,
    pub class_name: String,
    /// Its kind ([`id::kind`]).
    pub weenie_type: u32,
    pub ints: Vec<(u32, i32)>,
    pub bools: Vec<(u32, bool)>,
    pub floats: Vec<(u32, f64)>,
    pub strings: Vec<(u32, String)>,
    /// Its data id properties (its setup among them).
    pub dids: Vec<(u32, u32)>,
    pub positions: Vec<(u32, Spot)>,
    /// What it spawns, when it is a spawner.
    pub spawns: Vec<Spawn>,
}

impl Weenie {
    /// An empty weenie of class `class_id`, named `class_name`, of kind `weenie_type`.
    #[must_use]
    pub fn new(class_id: u32, class_name: &str, weenie_type: u32) -> Self {
        Self {
            class_id,
            class_name: class_name.to_owned(),
            weenie_type,
            ..Self::default()
        }
    }

    /// This weenie with integer property `id` set to `value`.
    #[must_use]
    pub fn with_int(mut self, id: u32, value: i32) -> Self {
        self.ints.push((id, value));
        self
    }

    /// This weenie with boolean property `id` set to `value`.
    #[must_use]
    pub fn with_bool(mut self, id: u32, value: bool) -> Self {
        self.bools.push((id, value));
        self
    }

    /// This weenie with floating-point property `id` set to `value`.
    #[must_use]
    pub fn with_float(mut self, id: u32, value: f64) -> Self {
        self.floats.push((id, value));
        self
    }

    /// The weenie with data id property `id` set to `value`.
    #[must_use]
    pub fn with_did(mut self, id: u32, value: u32) -> Self {
        self.dids.push((id, value));
        self
    }

    /// This weenie with string property `id` set to `value`.
    #[must_use]
    pub fn with_string(mut self, id: u32, value: &str) -> Self {
        self.strings.push((id, value.to_owned()));
        self
    }

    /// This weenie with position `id` at `at`.
    #[must_use]
    pub fn with_position(mut self, id: u32, at: Spot) -> Self {
        self.positions.push((id, at));
        self
    }

    /// This weenie spawning `spawn` too.
    #[must_use]
    pub fn with_spawn(mut self, spawn: Spawn) -> Self {
        self.spawns.push(spawn);
        self
    }

    /// Integer property `id`.
    #[must_use]
    pub fn int(&self, id: u32) -> Option<i32> {
        self.ints.iter().find(|(k, _)| *k == id).map(|&(_, v)| v)
    }

    /// Boolean property `id`.
    #[must_use]
    pub fn flag(&self, id: u32) -> Option<bool> {
        self.bools.iter().find(|(k, _)| *k == id).map(|&(_, v)| v)
    }

    /// Data id property `id`.
    #[must_use]
    pub fn did(&self, id: u32) -> Option<u32> {
        self.dids.iter().find(|(k, _)| *k == id).map(|&(_, v)| v)
    }

    /// Floating-point property `id`.
    #[must_use]
    pub fn float(&self, id: u32) -> Option<f64> {
        self.floats.iter().find(|(k, _)| *k == id).map(|&(_, v)| v)
    }

    /// String property `id`.
    #[must_use]
    pub fn string(&self, id: u32) -> Option<&str> {
        self.strings
            .iter()
            .find(|(k, _)| *k == id)
            .map(|(_, v)| v.as_str())
    }

    /// Position `id`.
    #[must_use]
    pub fn position(&self, id: u32) -> Option<Spot> {
        self.positions
            .iter()
            .find(|(k, _)| *k == id)
            .map(|&(_, v)| v)
    }

    /// Its `Name`, else its class name.
    #[must_use]
    pub fn name(&self) -> &str {
        self.string(id::string::NAME)
            .unwrap_or(self.class_name.as_str())
    }
}

/// An instance the world database places: where it stands, and the instances linked to it.
#[derive(Debug, Clone, PartialEq)]
pub struct Placement {
    /// Its own id, unique in the world.
    pub guid: u32,
    /// Its weenie.
    pub wcid: u32,
    pub at: Spot,
    /// The instances linked to it, by guid: what a spawner makes there, or where a portal leads.
    pub links: Vec<u32>,
}

/// An event the world database names, which switches spawners on and off.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameEvent {
    pub name: String,
    /// Its state as the database stores it: `4` is running.
    pub state: i32,
    /// When it starts and ends, in seconds since 1970; `-1` for not set.
    pub start: i64,
    pub end: i64,
}

impl GameEvent {
    /// The state of a running event.
    pub const ON: i32 = 4;

    /// Whether the event runs as the database leaves it.
    #[must_use]
    pub const fn running(&self) -> bool {
        self.state == Self::ON
    }
}

/// A landmark the world database names: where the portal of class `wcid` leads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Landmark {
    pub name: String,
    pub wcid: u32,
}

/// A world database, as the places are read from it.
pub trait WorldSource {
    /// Every instance it places, in guid order.
    fn placements(&self) -> Vec<Placement>;
    /// The weenie of class `wcid`.
    fn weenie(&self, wcid: u32) -> Option<Arc<Weenie>>;
    /// Every event.
    fn events(&self) -> Vec<GameEvent>;
    /// Every named landmark.
    fn landmarks(&self) -> Vec<Landmark>;
}

/// A world database held whole in memory: what [`crate::sql::read`] reads, and a world a test
/// writes by hand.
#[derive(Debug, Clone, Default)]
pub struct WorldContent {
    pub weenies: HashMap<u32, Arc<Weenie>>,
    /// In guid order.
    pub placements: Vec<Placement>,
    pub events: Vec<GameEvent>,
    pub landmarks: Vec<Landmark>,
}

impl WorldContent {
    /// An empty world.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// This world with `weenie` in it.
    #[must_use]
    pub fn with_weenie(mut self, weenie: Weenie) -> Self {
        self.weenies.insert(weenie.class_id, Arc::new(weenie));
        self
    }

    /// This world with an instance of `wcid` placed at `at` as `guid`, linked to `links`.
    #[must_use]
    pub fn with_placement(mut self, guid: u32, wcid: u32, at: Spot, links: &[u32]) -> Self {
        let i = self.placements.partition_point(|p| p.guid < guid);
        self.placements.insert(
            i,
            Placement {
                guid,
                wcid,
                at,
                links: links.to_vec(),
            },
        );
        self
    }

    /// This world with `event` in it.
    #[must_use]
    pub fn with_event(mut self, name: &str, state: i32) -> Self {
        self.events.push(GameEvent {
            name: name.to_owned(),
            state,
            start: -1,
            end: -1,
        });
        self
    }

    /// This world with the landmark `name`, where the portal of class `wcid` leads.
    #[must_use]
    pub fn with_landmark(mut self, name: &str, wcid: u32) -> Self {
        self.landmarks.push(Landmark {
            name: name.to_owned(),
            wcid,
        });
        self
    }
}

impl WorldSource for WorldContent {
    fn placements(&self) -> Vec<Placement> {
        self.placements.clone()
    }

    fn weenie(&self, wcid: u32) -> Option<Arc<Weenie>> {
        self.weenies.get(&wcid).cloned()
    }

    fn events(&self) -> Vec<GameEvent> {
        self.events.clone()
    }

    fn landmarks(&self) -> Vec<Landmark> {
        self.landmarks.clone()
    }
}
