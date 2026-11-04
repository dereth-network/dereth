//! The objects the world makes and unmakes while it runs: the creatures its camps keep alive, the
//! corpses they leave, and the items in corpses and in players' packs. (The objects that stand for
//! good, signs and portals and people, are the content pack's: [`crate::presence`].)
//!
//! A thing has an id of the runtime range, a template from the catalog, and either a place in the
//! world or a container (a corpse, a player) that holds it.

use std::collections::BTreeMap;

use auberean_protocol::physics::CreateObject;
use auberean_protocol::text::StringInfo;
use auberean_protocol::types::{VisualDesc, WeenieDesc};
use yalain_content::{CreatureStats, Description, Facts, Template};
use yalain_store::StoredPosition;

use crate::player;

/// The first id the world gives the things it makes (the runtime objects' range, above the content
/// pack's numbering).
pub const THING_IDS: u64 = 0x8131_0000_0000_0000;

/// How many numbers are set aside in the store at a time.
pub const ID_BLOCK: u64 = 1 << 20;

/// What a thing is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Creature,
    Corpse,
    Item,
    /// A resource node standing in the world, mined for its material.
    Node,
    /// A portal a player summoned, standing in the world for a while.
    Portal,
    /// A dye plant a player planted.
    Plant,
}

/// What a creature is doing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Mode {
    /// At rest (wandering about its home now and then).
    Idle,
    /// Going after a player.
    Chasing,
    /// Going home, having given up.
    Returning,
    /// Dead: its corpse is being made.
    Dead,
}

/// A creature's state beyond its object: its numbers, its camp and home, what it is doing.
#[derive(Debug, Clone, PartialEq)]
pub struct Creature {
    pub stats: CreatureStats,
    /// The camp that keeps it, by index.
    pub camp: Option<usize>,
    /// Where it appeared, and goes back to.
    pub home: StoredPosition,
    pub mode: Mode,
    /// The player (its character id) it is after.
    pub target: Option<u64>,
    /// When it may next strike, and next wander (world seconds).
    pub next_attack: f64,
    pub next_wander: f64,
    /// When it next plays one of its idle behaviors, at rest (world seconds).
    pub next_idle: f64,
    /// The object its last movement order aimed at.
    pub moving_to: Option<u64>,
    /// Where it is wandering to.
    pub wander_to: Option<StoredPosition>,
    /// Its steps in a row on its way home that something stood in the way of.
    pub blocked: u8,
    /// The way it follows around what stands between it and where it goes.
    pub way: crate::ai::Way,
    /// The damage each player has done it, by character id: the first to hurt it claims it.
    pub damage_by: Vec<(u64, i64)>,
    /// The health each damager took from it this fight (a player, or a pet apart from its owner),
    /// in the order they first hurt it, none past its death: its kill's experience is paid in
    /// parts by it.
    pub taken_by: Vec<(u64, i64)>,
    /// The players (character ids) it could not reach this fight, going after them: their kill of
    /// it is no fair fight.
    pub unreached: Vec<u64>,
    /// This fight it stands and shoots (else it closes in and strikes).
    pub ranged: bool,
    /// The specials it used lately, each with when it may be used again (world seconds).
    pub barred: Vec<(u32, f64)>,
    /// When it next looks about for a player to attack (once a second, at its own moment).
    pub next_look: f64,
    /// Since when its target has been beyond its perception (world seconds).
    pub lost_since: Option<f64>,
    /// A fight of its own kind it is joining: the player, and when it joins.
    pub joining: Option<(u64, f64)>,
    /// Until when (world seconds) it is open to a special that exploits openings, after attacking.
    pub opening: Option<f64>,
    /// The harm players did it lately: when (world seconds), who (character id), how much.
    pub harm: Vec<(f64, u64, i64)>,
    /// The attack (its due time) at which it last thought again about whom it fights.
    pub considered: f64,
    /// Since when (world seconds) it has fought its present target.
    pub target_since: f64,
    /// Its turns (attacks, heals, buffs) this fight.
    pub actions: u32,
    /// The heal or buff it used on one of its kin at its last turn, and on whom.
    pub kin_helped: Option<(u32, u64)>,
    /// The weapons it holds (things of their own), and the count of their attachings and lettings
    /// go (1 as they are first made hung).
    pub weapons: Vec<u64>,
    pub weapon_count: u16,
    /// The route it walks at rest, when its camp names one, and where it is on it.
    pub patrol: Option<crate::ai::Patrol>,
}

/// A corpse's state: who killed what, what it holds, when it goes.
#[derive(Debug, Clone, PartialEq)]
pub struct Corpse {
    /// The player that killed the creature (its character id).
    pub killer: u64,
    /// The creature it was.
    pub originator: u64,
    /// The things inside, in slot order.
    pub contents: Vec<u64>,
    /// The creature's template (none for a player's corpse), and whether it has been butchered.
    pub creature: u32,
    pub butchered: bool,
    /// The killer's level when it killed (0 when no player killed it).
    pub killer_level: i32,
    /// When it leaves the world (world seconds).
    pub expires: f64,
}

/// A resource node's state: who summoned it, what it yields, how often it has been mined, when
/// it goes.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    /// The player that summoned it (its character id); only it may mine the node.
    pub summoner: u64,
    /// The units a use draws around (a vein's: its base quantity, the miner's level added at each
    /// use), and their spread either side as a share.
    pub units: f32,
    pub variance: f32,
    /// Its material, the craft skill that mines it, and the uses it allows.
    pub material: u32,
    pub skill: u32,
    pub max_uses: u32,
    /// The uses made of it.
    pub uses: u32,
    /// A use of it under way.
    pub busy: bool,
    /// When it leaves the world (world seconds).
    pub expires: f64,
    /// A use of it may unearth a find in place of its material (a surveyor's map's node).
    pub finds: bool,
    /// It stands in the world for anyone to mine (a vein), not summoned: its units are drawn for
    /// each miner, and a while after it goes a new one stands where it stood.
    pub public: bool,
}

/// One thing.
#[derive(Debug, Clone, PartialEq)]
pub struct Thing {
    pub id: u64,
    pub template: u32,
    pub kind: Kind,
    pub description: Description,
    pub facts: Facts,
    /// Its place, when it stands in the world.
    pub at: Option<StoredPosition>,
    /// The object holding it, and its slot there.
    pub container: Option<(u64, u32)>,
    /// The player wielding or wearing it, and where.
    pub wielded: Option<(u64, u32)>,
    pub instance: u16,
    /// The last visual stamp its events used.
    pub visual: u16,
    pub position_stamp: u16,
    pub move_to_stamp: u16,
    pub heading_motion: u32,
    pub health: i32,
    pub max_health: i32,
    pub vigor: i32,
    pub max_vigor: i32,
    pub quantity: u32,
    pub value: u32,
    /// It appeared since the clients around were last told: it comes with its entry effect.
    pub fresh: bool,
    /// When it was last hurt (world seconds).
    pub last_hurt: f64,
    /// When it began regaining health (world seconds; 0 while it is not), and how much it has
    /// regained since.
    pub regen_since: f64,
    pub regained: i32,
    pub creature: Option<Creature>,
    pub corpse: Option<Corpse>,
    pub node: Option<Node>,
    /// The name of the player who made it (an item made by crafting).
    pub crafter: Option<String>,
    /// The player who summoned it (a pet: its character id), and the velocity scale it moves with.
    pub summoner: Option<u64>,
    pub velocity_scale: Option<f32>,
    /// The kingdom it shows as its own (a pet: its owner's).
    pub kingdom: Option<u32>,
    /// The name it is shown with in place of its template's (a pet its owner named).
    pub named: Option<StringInfo>,
    /// Its durability now, once use has worn it below its template's.
    pub durability: Option<u32>,
    /// The death pose it lies in: a dead creature's, a corpse's.
    pub pose: Option<f32>,
    /// The look it is drawn with in place of its template's (a player's corpse: the player's).
    pub drawn_as: Option<VisualDesc>,
    /// A surveyor's map's hidden place, once chosen; where a summoned portal leads.
    pub destination: Option<StoredPosition>,
    /// Where a surveyor's map's holder last took its bearings this time in the world, and how far
    /// from the map's place it was then.
    pub bearing: Option<(StoredPosition, f32)>,
    /// What a player inscribed on it, and the player's name.
    pub inscription: Option<(String, String)>,
    /// Its own effects on whoever wears or wields it (a crafted bonus): each saved state and the
    /// spellcraft it is cast at.
    pub bonuses: Vec<(u32, f32)>,
    /// It becomes bound to its holder when used in combat (an item its making binds), and has not
    /// been yet.
    pub binds_on_use: bool,
    /// What a treasure item carries of its own (its least level, Arcane Lore, a potion's effect).
    pub own: crate::treasure::Own,
    /// Its own permanent effects (a made item's): each saved state and the spellcraft it is at.
    pub enchantments: Vec<(u32, f32)>,
    /// It is a bow, made strung.
    pub strung: bool,
}

impl Thing {
    /// A thing of a template, nowhere yet.
    #[must_use]
    pub fn of(id: u64, t: &Template, kind: Kind) -> Self {
        Self {
            id,
            template: t.id,
            kind,
            description: t.description.clone(),
            facts: t.facts,
            at: None,
            container: None,
            wielded: None,
            instance: 0,
            visual: 1,
            position_stamp: 1,
            move_to_stamp: 0,
            heading_motion: crate::world::IDLE_MOTION,
            health: 0,
            max_health: 0,
            vigor: 0,
            max_vigor: 0,
            quantity: 1,
            value: 0,
            fresh: false,
            last_hurt: 0.0,
            regen_since: 0.0,
            regained: 0,
            creature: None,
            corpse: None,
            node: None,
            crafter: None,
            summoner: None,
            velocity_scale: None,
            kingdom: None,
            named: None,
            durability: None,
            pose: None,
            drawn_as: None,
            destination: None,
            bearing: None,
            inscription: None,
            bonuses: Vec::new(),
            binds_on_use: false,
            own: crate::treasure::Own::default(),
            enchantments: Vec::new(),
            strung: crate::skill_use::is_bow(t),
        }
    }

    /// Its level, if it has one.
    #[must_use]
    pub fn level(&self) -> i32 {
        self.creature
            .as_ref()
            .map(|c| c.stats.level)
            .or(self.facts.level)
            .unwrap_or(1)
    }

    /// Is it a creature still alive?
    #[must_use]
    pub fn alive(&self) -> bool {
        self.kind == Kind::Creature && self.health > 0
    }
}

/// Every thing of the world, by id.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Things {
    pub by_id: BTreeMap<u64, Thing>,
    /// The numbers of the block set aside in the store still to give, the first of them next.
    next: u64,
    end: u64,
}

impl Things {
    /// Ids from this block of numbers the store has set aside.
    pub fn add_block(&mut self, first: u64, n: u64) {
        // What has been given past the end of a block (the store would not answer) stays given.
        self.next = self.next.max(first);
        self.end = first + n;
    }

    /// Is the block running low, so another should be set aside?
    #[must_use]
    pub const fn low(&self) -> bool {
        self.end <= self.next || self.end - self.next < ID_BLOCK / 2
    }

    /// A fresh id. Nothing a store holds has it, or will be given it: its number was set aside in
    /// the store before it was given, and every id of the runs before there were blocks is lower.
    pub fn new_id(&mut self) -> u64 {
        self.next += 1;
        THING_IDS + self.next - 1
    }

    #[must_use]
    pub fn get(&self, id: u64) -> Option<&Thing> {
        self.by_id.get(&id)
    }

    pub fn get_mut(&mut self, id: u64) -> Option<&mut Thing> {
        self.by_id.get_mut(&id)
    }

    /// A thing joins the world, unless its id is one the world already has, which is left as it is:
    /// no thing ever takes another's place. Whether it joined.
    pub fn insert(&mut self, t: Thing) -> bool {
        if self.by_id.contains_key(&t.id) {
            tracing::error!(
                "an id given twice: {:#x}, template {:08X}, is not put in place of the one the world \
                 has, template {:08X}",
                t.id,
                t.template,
                self.by_id[&t.id].template
            );
            return false;
        }
        self.by_id.insert(t.id, t);
        true
    }

    pub fn remove(&mut self, id: u64) -> Option<Thing> {
        self.by_id.remove(&id)
    }

    /// The things standing in the world.
    pub fn placed(&self) -> impl Iterator<Item = &Thing> + '_ {
        self.by_id.values().filter(|t| t.at.is_some())
    }
}

/// The place among a weenie's object ids of the player who summoned it, and among its numbers of
/// its kingdom.
const PET_SUMMONER: usize = 6;
const FACTION: usize = 2;

/// A thing's object as the world creates it, whenever a client is told of it: an item as items
/// are, with its quantity and name; a corpse as corpses are, lying in its pose; a dead creature
/// in its dead mode and pose; anything else standing.
#[must_use]
pub fn create(t: &Thing) -> CreateObject {
    match t.kind {
        Kind::Item => crate::inventory::item_create(t),
        Kind::Corpse if t.corpse.is_some() => crate::death::corpse_create(t),
        Kind::Creature if t.pose.is_some() => crate::death::dead_create(t),
        _ => standing_create(t),
    }
}

/// A thing's object standing: its template's look and words, its name, its place (if it stands in
/// the world), its stamps; its entry effect when it has just appeared.
#[must_use]
pub fn standing_create(t: &Thing) -> CreateObject {
    let d = &t.description;
    let mut weenie = player::weenie_with_numbers(
        WeenieDesc {
            mask: WeenieDesc::PACKAGE | WeenieDesc::ENTITY,
            package_id: d.package,
            entity_did: t.template,
            ..WeenieDesc::default()
        },
        d.numbers,
    );
    if let Some(s) = d.scale {
        weenie.mask |= WeenieDesc::SCALE;
        weenie.scale = s;
    }
    if let Some(n) = &t.named {
        weenie.mask |= WeenieDesc::NAME;
        weenie.name = n.clone();
    } else if let Some((string_id, table_id)) = d.name {
        weenie.mask |= WeenieDesc::NAME;
        weenie.name = StringInfo {
            string_id,
            table_id,
            ..StringInfo::default()
        };
    }
    if let Some(owner) = t.summoner {
        // A pet names the player who summoned it.
        weenie.mask |= WeenieDesc::OBJECT_BITS[PET_SUMMONER];
        weenie.objects[PET_SUMMONER] = owner;
    }
    if let Some(k) = t.kingdom {
        weenie.mask |= WeenieDesc::NUMBER_BITS[FACTION];
        weenie.numbers[FACTION] = k.cast_signed();
    }
    let at = t.at.unwrap_or(StoredPosition {
        cell: 0,
        origin: [0.0; 3],
        rotation: [1.0, 0.0, 0.0, 0.0],
    });
    let fx = if t.fresh { d.enter_fx } else { 0 };
    let mut physics = player::physics_at(player::placement(&at), t.instance, t.visual, fx);
    physics.stamps = [t.position_stamp, 0, 0, t.move_to_stamp];
    if let Some(v) = t.velocity_scale {
        physics.bits |= auberean_protocol::types::PhysicsDesc::VELOCITY_SCALE;
        physics.velocity_scale = v;
    }
    CreateObject {
        id: t.id,
        // A look of its own (a dye plant grown) in place of its template's.
        visual: t.drawn_as.clone().unwrap_or(VisualDesc {
            bits: VisualDesc::PARENT,
            parent: d.look,
            ..VisualDesc::default()
        }),
        physics,
        weenie,
    }
}
