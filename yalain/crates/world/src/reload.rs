//! Reloading the world database while the server runs.
//!
//! The host compiles the database again and hands the result over; the world takes what it can
//! take without a restart and says what it did. A compile that fails never reaches here, so the
//! world goes on as it was.
//!
//! - **Taken at once:** the templates the world makes things of, the loot, what storekeepers sell,
//!   the quests, what people say, the story's scenes, the doors' locks, the effects and the
//!   crafting, the templates of creatures' weapons, and the operator's banned names. A template or
//!   an effect the new database no longer has stays known, so a thing or an effect already in the
//!   world still resolves.
//! - **Held until a restart:** the placed objects (with the market's brokers among them) and the
//!   camps. Clients know each placed object by its place in the list, a broker keeps what players
//!   put up with it, and each camp keeps its creatures, so none changes under them; a change to
//!   either is reported and waits for the server to start again.
//!
//! What the server reads from the install alone (the character data, the skills, the levels) is
//! read at start and is not part of the world database.

use yalain_content::{
    Catalog, ContentPack, Kind, LockTable, LootTable, QuestTable, SpawnTable, StoreTable,
    StoryTable, TalkTable,
};
use yalain_store::Store;

use crate::craft::Crafting;
use crate::effect_defs::EffectTable;
use crate::names::NameRules;
use crate::Server;

/// A world database compiled again, with what the install adds to it.
#[derive(Debug, Clone, Default)]
pub struct WorldTables {
    pub content: ContentPack,
    pub catalog: Catalog,
    pub spawns: SpawnTable,
    pub loot: LootTable,
    pub stores: StoreTable,
    pub quests: QuestTable,
    pub talk: TalkTable,
    pub story: StoryTable,
    /// The locked doors and their keys.
    pub locks: LockTable,
    pub effects: EffectTable,
    pub crafting: Crafting,
    /// The templates of the weapons creatures wield, by the names their templates give them.
    pub weapons: std::collections::HashMap<String, u32>,
    /// What the database says of those weapons beyond their templates.
    pub weapon_rules: std::collections::HashMap<String, yalain_content::WeaponRule>,
    /// The operator's banned names read again, when the server was given a list.
    pub names: Option<NameRules>,
}

/// The tables a reload looks at, by the name it reports them under.
pub mod table {
    pub const TEMPLATES: &str = "templates";
    pub const LOOT: &str = "loot";
    pub const STORES: &str = "stores";
    pub const QUESTS: &str = "quests";
    pub const TALK: &str = "talk";
    pub const STORY: &str = "story";
    pub const LOCKS: &str = "locks";
    pub const EFFECTS: &str = "effects";
    pub const CRAFTING: &str = "crafting";
    pub const WEAPONS: &str = "creature weapons";
    pub const BANNED_NAMES: &str = "banned names";
    pub const PLACED: &str = "placed objects";
    pub const CAMPS: &str = "camps";
}

/// What a reload did, table by table.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reloaded {
    /// Changed and taken at once.
    pub applied: Vec<&'static str>,
    /// Changed, but held until the server starts again.
    pub restart: Vec<&'static str>,
    /// The same as before.
    pub unchanged: Vec<&'static str>,
}

impl Reloaded {
    /// One line for the operator: what was taken, what waits for a restart, what was the same.
    #[must_use]
    pub fn summary(&self) -> String {
        let list = |v: &[&str]| {
            if v.is_empty() {
                "nothing".to_string()
            } else {
                v.join(", ")
            }
        };
        format!(
            "reloaded: {}; needs a restart: {}; unchanged: {}",
            list(&self.applied),
            list(&self.restart),
            list(&self.unchanged)
        )
    }

    fn note(&mut self, name: &'static str, changed: bool, live: bool) {
        match (changed, live) {
            (false, _) => self.unchanged.push(name),
            (true, true) => self.applied.push(name),
            (true, false) => self.restart.push(name),
        }
    }
}

impl<S: Store> Server<S> {
    /// Take a world database compiled again: what can change while the world runs changes now,
    /// the rest is reported as needing a restart and left as it was.
    pub fn reload(&mut self, mut t: WorldTables) -> Reloaded {
        let mut r = Reloaded::default();
        // The placed objects as the world holds them: with camps, the camps keep the creatures.
        if !t.spawns.camps.is_empty() {
            t.content.objects.retain(|o| o.kind != Kind::Creature);
        }

        let mut catalog = self.catalog.clone();
        catalog.templates.extend(t.catalog.templates);
        catalog.profiles.extend(t.catalog.profiles);
        catalog.icons.extend(t.catalog.icons);
        catalog.bodies.extend(t.catalog.bodies);
        let changed = catalog != self.catalog;
        r.note(table::TEMPLATES, changed, true);
        self.catalog = catalog;

        let changed = t.loot != self.loot;
        r.note(table::LOOT, changed, true);
        self.loot = t.loot;

        let changed = t.stores != self.stores;
        r.note(table::STORES, changed, true);
        self.stores = t.stores;

        let changed = t.quests != self.quests;
        r.note(table::QUESTS, changed, true);
        self.quests = t.quests;

        let changed = t.talk != self.talk;
        r.note(table::TALK, changed, true);
        if changed {
            // Who has lines for a player near, and who chatters, is found again.
            self.talkers = None;
        }
        self.talk = t.talk;

        let changed = t.story != self.story;
        r.note(table::STORY, changed, true);
        self.story = t.story;

        let changed = t.locks != self.locks;
        r.note(table::LOCKS, changed, true);
        self.locks = t.locks;

        let mut effects = self.effect_table.clone();
        effects.effects.extend(t.effects.effects);
        let changed = effects != self.effect_table;
        r.note(table::EFFECTS, changed, true);
        self.effect_table = effects;

        let changed = t.crafting != self.crafting;
        r.note(table::CRAFTING, changed, true);
        self.crafting = t.crafting;

        // A creature already armed keeps its weapons; those made from now on are of the new table.
        let changed =
            t.weapons != self.creature_weapons || t.weapon_rules != self.creature_weapon_rules;
        r.note(table::WEAPONS, changed, true);
        self.creature_weapons = t.weapons;
        self.creature_weapon_rules = t.weapon_rules;

        if let Some(names) = t.names {
            r.note(table::BANNED_NAMES, names != self.names, true);
            self.names = names;
        }

        // Veins stand in the world as nodes, not among the placed objects.
        t.content.objects.retain(|o| {
            !self
                .catalog
                .get(o.template)
                .is_some_and(crate::mining::is_craft_node)
        });
        r.note(table::PLACED, t.content != self.content, false);
        r.note(table::CAMPS, t.spawns != self.spawns, false);
        r
    }
}
