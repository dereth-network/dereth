//! The game server's state and rules, sans-IO: accounts, characters, the world, its creatures and
//! its play, driven by the server's network events and an injected clock.
//!
//! **Depends on** the messages (`auberean-protocol`), the script machine's values
//! (`auberean-script`), the client's written form of names and its word patterns
//! (`auberean-text`, `auberean-assets`), the client's table of record kinds and its data files
//! (`auberean-dat`), `auberean-primitives`, the server's connections (`yalain-net`), the store's
//! trait (`yalain-store`), the content (`yalain-content`), the shared physics its creatures move through
//! (`auberean-physics`, `auberean-world`, `dereth-physics`), the chat service's packet
//! (`dereth-protocol`), the original's float-to-integer conversion (`dereth-primitives`) and the
//! records' compression (`flate2`).
//! **Used by** the server executable (`yalain`) and its test kit (`yalain-testkit`).
//!
//! **Must never** touch a socket, a file or the clock: the host feeds it the network's events and
//! the time ([`Clock`]), and it answers through the network it is handed.
//!
//! One [`Server`] owns everything; every handler runs on the caller's thread, one event at a
//! time.
//!
//! - [`chargen`]: how a new character looks.
//! - [`names`]: the written form of character names, the names a character may have, the
//!   operator's banned words.
//! - [`login`]: the login connection: the account, the character list, creating, deleting,
//!   entering the world and leaving it.
//! - [`download`]: the data records clients ask for, served from the server's data files.
//! - [`world`]: the world connection: a character entering, moving and leaving; its options.
//! - [`presence`]: who sees what: objects and players coming into and leaving view, movements
//!   relayed to the clients that know the mover.
//! - [`requests`]: the calls a client makes on the server in the world (selection, examination,
//!   chat, using objects), each answered as the 2017 servers answered it.
//! - [`player`], [`session`], [`options`]: what the world says about a player as it enters.
//! - [`start`]: where a new character starts and what it starts with.
//! - [`talk`]: what people say, and when: with a quest step, when used, to a player come near,
//!   unprompted, fighting and dying.
//! - [`graph`], [`uploads`], [`calls`]: script-call values built and read.
//! - [`social`], [`allegiance`], [`fellowship`], [`trade`]: what players ask of one another: the
//!   allegiance's oaths and profiles, fellowships and their shared experience, trades.
//! - [`pets`]: the creatures players' skills summon, their orders, their behavior and their end.
//! - [`plants`]: dye plants: planted from seeds, cared for, pollinated and harvested.
//! - [`mounts`]: saddles and mount eggs put their user on a beast; riding, dismounting and the
//!   ride's end.
//! - [`usable`]: doors, chests, their locks and keys, surveyor's maps, items used on the objects
//!   their interaction tables name (a hoard's key on its stockpile).
//! - [`walkers`]: placed people walking their routes.
//! - [`reload`]: the world database taken again while the server runs.
//! - [`unhandled`]: what clients reached for that the server does not do yet, counted.

pub mod ai;
pub mod allegiance;
pub mod arms;
pub mod books;
pub mod butchery;
pub mod calls;
pub mod chargen;
pub mod chat;
pub mod combat;
pub mod craft;
pub mod death;
pub mod download;
pub mod drops;
pub mod effect_defs;
pub mod effects;
pub mod equip;
pub mod examine;
pub mod fellowship;
pub mod focus;
pub mod follow;
pub mod graph;
pub mod inventory;
pub mod item_use;
pub mod levels;
pub mod login;
pub mod market;
pub mod mining;
pub mod missiles;
pub mod mounts;
pub mod names;
pub mod options;
pub mod pack;
pub mod personal;
pub mod pets;
pub mod places;
pub mod plants;
pub mod player;
pub mod presence;
pub mod profile;
pub mod progress;
pub mod quals;
pub mod quests;
pub mod reach;
pub mod regen;
pub mod reload;
pub mod requests;
pub mod rng;
pub mod rules;
pub mod scenes;
pub mod session;
pub mod shutdown;
pub mod skill_use;
pub mod skills;
pub mod social;
pub mod spawn;
pub mod start;
pub mod state;
pub mod store;
pub mod storms;
pub mod talk;
pub mod things;
pub mod trade;
pub mod unhandled;
pub mod uploads;
pub mod usable;
pub mod walkers;
pub mod walls;
pub mod world;
pub mod xp;

use std::collections::HashMap;

use yalain_content::{Catalog, ContentPack, LootTable, SpawnTable};
use yalain_net::{ConnId, LocalTime, NetEvent, ServerNet};
use yalain_store::{Account, Store};

pub use chargen::CharGenRules;
pub use start::StartRules;
pub use unhandled::Unhandled;
pub use world::Player;

/// The time, as the host gives it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Clock {
    /// The transport's clock, in seconds.
    pub local: LocalTime,
    /// Seconds since 1970, for what is kept.
    pub unix: i64,
}

/// Who may log in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountPolicy {
    /// Any account: one is made the first time its name logs in, with its ticket, which it needs
    /// from then on.
    Open,
    /// Only accounts made beforehand, with their ticket.
    Listed,
}

/// The server's settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerConfig {
    /// The world's name, shown at login.
    pub world_name: String,
    pub accounts: AccountPolicy,
    /// How many characters an account may have.
    pub slots: u32,
    /// How long a deleted character stays in the list, greyed out, before it is gone.
    pub delete_seconds: u32,
    /// Send a referral to the world service when a character enters (as 2017 did); without,
    /// the world comes on the login connection.
    pub referral: bool,
    /// Send each player its own position and head back as it reports them.
    pub echo_positions: bool,
    /// The seed of the world's dice.
    pub seed: u64,
}

impl Default for ServerConfig {
    /// The January 2017 servers' numbers: six characters, an hour for a deletion.
    fn default() -> Self {
        Self {
            world_name: "Local World".to_string(),
            accounts: AccountPolicy::Open,
            slots: 6,
            delete_seconds: 3600,
            referral: true,
            echo_positions: true,
            seed: 0x5EED_00A2,
        }
    }
}

/// Where a login connection's client is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// At the character list.
    Selecting,
    /// Asked to enter the world with this character; the referral is out.
    Entering { character: u64, cookie: u64 },
    /// Connected to the world with this character.
    InWorld { character: u64, world: ConnId },
}

/// One client's login.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub account: Account,
    pub stage: Stage,
}

/// The game server.
#[derive(Debug)]
pub struct Server<S: Store> {
    pub cfg: ServerConfig,
    pub store: S,
    pub rules: CharGenRules,
    /// The operator's rules for character names.
    pub names: names::NameRules,
    /// Where new characters start and what they start with.
    pub start: StartRules,
    /// The objects of the world.
    pub content: ContentPack,
    /// Login connections and their sessions.
    pub sessions: HashMap<ConnId, Session>,
    /// World connections and the login connection each belongs to.
    pub worlds: HashMap<ConnId, ConnId>,
    /// The characters in the world, by the connection their world travels on.
    pub players: HashMap<ConnId, Player>,
    /// Whether the world is shutting down.
    pub shutdown: shutdown::Shutdown,
    /// What is to happen later, by when.
    pub timers: Vec<(LocalTime, Timer)>,
    /// The last context number a behavior or movement order carried.
    pub next_context: u32,
    /// The last visual stamp each content object's visual events used (made with 1).
    pub visuals: HashMap<u64, u16>,
    pub unhandled: Unhandled,
    /// The Dereth Network's commands said in chat since the host last took them: heard by
    /// nobody, answered by the world's friends service.
    pub network_commands: Vec<chat::NetworkCommand>,
    /// The names of this world's characters each character ignores on its friends service, in
    /// small letters, by character id: the game's own tell honours them. The friends service
    /// keeps it.
    pub ignoring: HashMap<u64, std::collections::HashSet<String>>,
    /// The accounts with a friends stream the world's friends service admitted; `None` while
    /// the world serves no friends service. The friends service keeps it.
    pub friends_streams: Option<std::collections::HashSet<u64>>,
    /// The templates the world makes things of, and the names of generator profiles.
    pub catalog: Catalog,
    /// Where creatures spawn, and what they leave.
    pub spawns: SpawnTable,
    pub loot: LootTable,
    /// What storekeepers sell, and the quests the world runs.
    pub stores: yalain_content::StoreTable,
    pub quests: yalain_content::QuestTable,
    /// What people say, and when.
    pub talk: yalain_content::TalkTable,
    /// The content objects with lines for a player near, and with idle lines (found once).
    pub talkers: Option<(Vec<usize>, Vec<usize>)>,
    /// When each player was last greeted by each content object (world seconds).
    pub greeted: HashMap<(u64, usize), f64>,
    /// When each content object next chatters (world seconds).
    pub chatter: HashMap<usize, f64>,
    /// The things the world has made, and each camp's state.
    pub things: things::Things,
    pub camps: Vec<spawn::CampState>,
    /// The ground's height outdoors, when the host knows the land.
    pub ground: Option<Box<dyn Ground>>,
    /// The walls, floors and solid things creatures meet, when the host knows the world.
    pub walls: Option<walls::Walls>,
    /// The data files a client's requests for records are served from, when the host has them.
    pub data: Option<Box<dyn download::DataFiles>>,
    pub rng: rng::Rng,
    /// The dice of the players' regeneration steps, apart from the world's.
    pub regen_rng: rng::Rng,
    /// The character levels.
    pub levels: levels::LevelTable,
    /// The skills' definitions and advancement tables.
    pub skill_table: skills::SkillTable,
    /// The effects' definitions, and the recipes' and craft skills'.
    pub effect_table: effect_defs::EffectTable,
    /// The effects on creatures (the server's alone: no client holds them).
    pub creature_effects: effects::CreatureRecords,
    pub crafting: craft::Crafting,
    /// The vitae cost table: the experience to raise vitae by one at full vitae, by level from 1.
    pub vitae_costs: Vec<u64>,
    /// When the world last took a step (its creatures, camps and corpses).
    pub last_step: Option<LocalTime>,
    /// The game clock as the world last read it (set as the world ticks and as players enter).
    pub game_now: f64,
    /// The effects each item template's profile lists.
    pub item_effects: HashMap<u32, Vec<profile::ItemEffect>>,
    /// The effects each item template's use puts on its user.
    pub item_uses: item_use::ItemUses,
    /// The effects each item template puts on whoever wears or wields it.
    pub worn_effects: item_use::ItemUses,
    /// What each player is examining.
    pub examining: HashMap<ConnId, examine::Examining>,
    /// The packages of the objects a player uses by walking into them (portals).
    pub walk_in_packages: std::collections::HashSet<u32>,
    /// The packages of the world's people and objects that answer a use as a person does (those
    /// derived from the script library's person), and of its faction shrines.
    pub person_packages: std::collections::HashSet<u32>,
    /// The portal storms of crowded landblocks.
    pub storms: storms::Storms,
    pub shrine_packages: std::collections::HashSet<u32>,
    /// The packages of the items that take an inscription.
    pub inscribable_packages: std::collections::HashSet<u32>,
    /// The packages of the items whose use from the pack only answers: the status and flags.
    pub held_answers: std::collections::HashMap<u32, (u32, u32)>,
    /// The vaults' trips.
    pub vault_trips: usable::vaults::VaultTrips,
    /// The templates of creatures' weapons, by the names creatures' templates give them.
    pub creature_weapons: HashMap<String, u32>,
    /// What the world database says of those weapons beyond their templates, by the same names.
    pub creature_weapon_rules: HashMap<String, yalain_content::WeaponRule>,
    /// The packages whose items are destroyed on use by their kind (keys, potions, ...).
    pub destroyed_on_use: std::collections::HashSet<u32>,
    /// The object each player's body touches that it walked into.
    pub touching: HashMap<ConnId, u64>,
    /// The allegiances (kept as a record of the world), the fellowships and the trades between
    /// players.
    pub allegiances: allegiance::Allegiances,
    pub fellowships: fellowship::Fellowships,
    pub trades: trade::Trades,
    /// The story's scenes and the shards that bestow them.
    pub story: yalain_content::StoryTable,
    /// The brokers (by template) and the consignments players put up with them.
    pub brokers: std::collections::BTreeMap<u32, market::Broker>,
    pub market: market::Market,
    /// The pets players have summoned.
    pub pets: pets::Pets,
    /// The dye plants players have planted, and the species' ripe looks.
    pub plants: plants::Plants,
    /// The locked doors and their keys, and the doors' states.
    pub locks: yalain_content::LockTable,
    pub doors: usable::doors::Doors,
    /// The chests' states.
    pub chests: usable::chests::Chests,
    /// The mounts: their effects, the items that mount their users, the mounted looks.
    pub mounts: mounts::Mounts,
    /// The item templates' interaction tables: what each item may be used on, and to what end.
    pub interactions: usable::interactions::Interactions,
    /// The placed people walking their routes.
    pub walkers: walkers::Walkers,
}

/// The height of the ground under a point outdoors (a cell, a point in its landblock), and the
/// interior cells seen from an interior cell.
pub trait Ground: std::fmt::Debug {
    fn height(&self, cell: u32, origin: [f32; 3]) -> Option<f32>;

    /// The interior cells (whole ids) visible from an interior cell, when the host knows them.
    fn sees(&self, _cell: u32) -> Option<&[u32]> {
        None
    }
}

/// Something the world does later.
#[derive(Debug, Clone, PartialEq)]
pub enum Timer {
    /// The next step of a player's trip through portal space.
    Trip { world: ConnId, step: places::Step },
    /// A player's taking of an item is through.
    Take { world: ConnId, item: u64 },
    /// A player's taking of everything in a container is through.
    TakeAll { world: ConnId, container: u64 },
    /// A dead creature's corpse appears.
    Corpse {
        creature: u64,
        killer: u64,
        pose: f32,
    },
    /// A thing leaves the world.
    Vanish { id: u64 },
    /// A creature's missile arrives.
    Missile(missiles::Flight),
    /// A pet's missile lands.
    PetMissile {
        pet: u64,
        target: u64,
        skill: missiles::AttackSkill,
    },
    /// A pet's lifetime is over.
    PetExpires { pet: u64 },
    /// A creature's heal or buff on itself takes effect.
    CreatureHelps { creature: u64, skill: u32, on: u64 },
    /// A dead player's corpse is made.
    PlayerCorpse { world: ConnId },
    /// A player's lifestone protection runs out.
    ProtectionEnds { world: ConnId, eid: u32 },
    /// The next step of a player's use of a skill (the use's number).
    Skill {
        world: ConnId,
        id: u32,
        step: skill_use::Step,
    },
    /// A creature's opening opens, or closes.
    Opening { creature: u64, closes: bool },
    /// An effect on a player runs out.
    EffectEnds { world: ConnId, eid: u32 },
    /// An effect on a player changes its vitals again (the last change at `since`, world
    /// seconds).
    EffectTick { world: ConnId, eid: u32, since: f64 },
    /// An effect on a creature runs out.
    CreatureEffectEnds { creature: u64, eid: u32 },
    /// An effect on a creature changes its health again (the last change at `since`, world
    /// seconds).
    CreatureEffectTick { creature: u64, eid: u32, since: f64 },
    /// A player's aura (its record's id) pulses.
    AuraPulse { world: ConnId, eid: u32 },
    /// The next step of a player's craft.
    Craft { world: ConnId, step: craft::Step },
    /// A player's day of craft experience may be over.
    CraftDay { world: ConnId },
    /// A player's butchery of a corpse is done.
    Butcher {
        world: ConnId,
        work: Box<butchery::Work>,
    },
    /// A player's mining of a node is done.
    Mine {
        world: ConnId,
        work: Box<mining::Work>,
    },
    /// A spent vein's time away is over: a new one stands where it stood.
    VeinBack {
        template: u32,
        at: yalain_store::StoredPosition,
    },
    /// A quest given at `started` may have run out.
    QuestRunsOut {
        world: ConnId,
        quest: u32,
        started: i64,
    },
    /// An effect that sends its player somewhere has run its time: off it goes.
    SentAway {
        world: ConnId,
        to: yalain_store::StoredPosition,
    },
    /// A player's work at a quest mine is done.
    QuestMine {
        world: ConnId,
        work: Box<mining::MineWork>,
    },
    /// A player's picking up of an item is through.
    PickUp { world: ConnId, item: u64 },
    /// A player's use of an item on an object, its behavior played out.
    Interaction {
        world: ConnId,
        used: Box<places::Use>,
    },
    /// A dye plant thinks: its look, its needs.
    PlantThinks { plant: u64 },
    /// A player's harvest of a dye plant has played out.
    Harvest {
        world: ConnId,
        used: Box<places::Use>,
    },
    /// A player's use of an item has played out.
    ItemUse {
        world: ConnId,
        using: Box<item_use::Using>,
    },
    /// A player's basic attack lands (a missile arrives, a strike reaches).
    Landing(combat::Landing),
    /// Look again at what a player examines (the examination's number).
    Examine { world: ConnId, number: u32 },
    /// A player's putting an item down is through: the item it asked to move, from its pack's slot.
    PutDown {
        world: ConnId,
        slot: u32,
        asked: Box<inventory::Move>,
    },
    /// An item put down comes to rest.
    Settle { item: u64 },
    /// A player's binding to a lifestone has played: its behavior is stopped.
    BindEnds { world: ConnId },
    /// A recruit's asking runs out.
    Asking { leader: u64, recruit: u64 },
    /// The next step of a door's moving (its movement's number).
    Door {
        door: u64,
        generation: u32,
        step: usable::doors::Step,
    },
    /// The next step of a chest's moving (its movement's number).
    Chest {
        chest: u64,
        generation: u32,
        step: usable::chests::Step,
    },
    /// A story shard's protection ends.
    ShardProtectionEnds { world: ConnId, eid: u32 },
    /// A rider seen indoors is checked again (its ride's record).
    MountIndoors { world: ConnId, eid: u32 },
    /// The next step of a player's leaving the world after it logged off.
    LogOff {
        world: ConnId,
        step: world::LogOffStep,
    },
    /// The world's shutdown countdown, with this many seconds left.
    Shutdown { left: u32 },
    /// The world's shutdown is through.
    ShutdownEnds,
}

impl<S: Store> Server<S> {
    #[must_use]
    pub fn new(cfg: ServerConfig, store: S, rules: CharGenRules) -> Self {
        let cfg_seed = cfg.seed;
        Self {
            cfg,
            store,
            rules,
            names: names::NameRules::default(),
            start: StartRules::default(),
            content: ContentPack::default(),
            sessions: HashMap::new(),
            worlds: HashMap::new(),
            players: HashMap::new(),
            shutdown: shutdown::Shutdown::No,
            timers: Vec::new(),
            next_context: 0,
            visuals: HashMap::new(),
            unhandled: Unhandled::default(),
            network_commands: Vec::new(),
            ignoring: HashMap::new(),
            friends_streams: None,
            catalog: Catalog::default(),
            spawns: SpawnTable::default(),
            loot: LootTable::default(),
            stores: yalain_content::StoreTable::default(),
            quests: yalain_content::QuestTable::default(),
            talk: yalain_content::TalkTable::default(),
            talkers: None,
            greeted: HashMap::new(),
            chatter: HashMap::new(),
            things: things::Things::starting_at(cfg_seed),
            camps: Vec::new(),
            ground: None,
            walls: None,
            data: None,
            rng: rng::Rng::new(cfg_seed),
            regen_rng: rng::Rng::new(cfg_seed ^ 0x7265_6765_6E00),
            levels: levels::LevelTable::default(),
            skill_table: skills::SkillTable::default(),
            effect_table: effect_defs::EffectTable::default(),
            creature_effects: effects::CreatureRecords::default(),
            crafting: craft::Crafting::default(),
            vitae_costs: Vec::new(),
            last_step: None,
            game_now: 0.0,
            item_effects: HashMap::new(),
            item_uses: HashMap::new(),
            worn_effects: HashMap::new(),
            examining: HashMap::new(),
            walk_in_packages: std::collections::HashSet::new(),
            person_packages: std::collections::HashSet::new(),
            storms: storms::Storms::default(),
            shrine_packages: std::collections::HashSet::new(),
            inscribable_packages: std::collections::HashSet::new(),
            held_answers: std::collections::HashMap::new(),
            vault_trips: usable::vaults::VaultTrips::new(),
            creature_weapons: HashMap::new(),
            creature_weapon_rules: HashMap::new(),
            destroyed_on_use: std::collections::HashSet::new(),
            touching: HashMap::new(),
            allegiances: allegiance::Allegiances::default(),
            fellowships: fellowship::Fellowships::default(),
            trades: trade::Trades::default(),
            story: yalain_content::StoryTable::default(),
            brokers: std::collections::BTreeMap::new(),
            market: market::Market::default(),
            pets: pets::Pets::default(),
            plants: plants::Plants::default(),
            locks: yalain_content::LockTable::default(),
            doors: usable::doors::Doors::default(),
            chests: usable::chests::Chests::default(),
            mounts: mounts::Mounts::default(),
            interactions: usable::interactions::Interactions::default(),
            walkers: walkers::Walkers::default(),
        }
    }

    /// The server with its start rules and its world's objects.
    #[must_use]
    pub fn with_world(mut self, start: StartRules, content: ContentPack) -> Self {
        self.start = start;
        self.content = content;
        self
    }

    /// The server with what it plays: the templates, where creatures spawn and what they leave.
    #[must_use]
    pub fn with_play(mut self, catalog: Catalog, spawns: SpawnTable, loot: LootTable) -> Self {
        self.camps = vec![spawn::CampState::default(); spawns.camps.len()];
        if !spawns.camps.is_empty() {
            // The camps keep the creatures now: the pack's, placed where one stood, give way.
            self.content
                .objects
                .retain(|o| o.kind != yalain_content::Kind::Creature);
        }
        self.catalog = catalog;
        self.spawns = spawns;
        self.loot = loot;
        mining::place_veins(&mut self);
        self
    }

    /// Handle every event the network has, then whatever is due.
    pub fn pump(&mut self, net: &mut ServerNet, clock: Clock) {
        while let Some(ev) = net.poll_event() {
            self.handle(net, ev, clock);
        }
        self.tick(net, clock);
    }

    /// Run what is due by now, and the world's step when one is due.
    pub fn tick(&mut self, net: &mut ServerNet, clock: Clock) {
        self.game_now = net.game_time(clock.local);
        while let Some(i) = self.timers.iter().position(|(at, _)| at.0 <= clock.local.0) {
            let (_, t) = self.timers.remove(i);
            match t {
                Timer::Trip { world, step } => places::step(self, net, world, step, clock),
                Timer::Take { world, item } => inventory::taken(self, net, world, item, clock),
                Timer::TakeAll { world, container } => {
                    pack::took_all(self, net, world, container, clock);
                }
                Timer::Corpse {
                    creature,
                    killer,
                    pose,
                } => death::corpse_appears(self, net, creature, killer, pose, clock),
                Timer::Vanish { id } => death::vanish(self, net, id),
                Timer::Missile(f) => missiles::arrives(self, net, f, clock),
                Timer::PetMissile { pet, target, skill } => {
                    pets::lands(self, net, pet, target, skill, clock);
                }
                Timer::PetExpires { pet } => pets::expires(self, net, pet),
                Timer::CreatureHelps {
                    creature,
                    skill,
                    on,
                } => {
                    missiles::helped(self, net, creature, skill, on, clock);
                }
                Timer::PlayerCorpse { world } => death::player_corpse(self, net, world, clock),
                Timer::ProtectionEnds { world, eid } => effects::runs_out(self, net, world, eid),
                Timer::Skill { world, id, step } => {
                    skill_use::step(self, net, world, id, step, clock);
                }
                Timer::EffectEnds { world, eid } => {
                    if !mounts::record_ends(self, net, world, eid) {
                        effects::ends(self, net, world, eid);
                    }
                }
                Timer::EffectTick { world, eid, since } => {
                    effects::tick(self, net, world, eid, since, clock);
                }
                Timer::CreatureEffectEnds { creature, eid } => {
                    effects::creature_ends(self, net, creature, eid, clock);
                }
                Timer::CreatureEffectTick {
                    creature,
                    eid,
                    since,
                } => effects::creature_tick(self, net, creature, eid, since, clock),
                Timer::AuraPulse { world, eid } => effects::pulse(self, net, world, eid, clock),
                Timer::Craft { world, step } => craft::step(self, net, world, step, clock),
                Timer::CraftDay { world } => craft::day_due(self, net, world, clock),
                Timer::Butcher { world, work } => butchery::done(self, net, world, &work),
                Timer::Mine { world, work } => mining::done(self, net, world, &work, clock),
                Timer::VeinBack { template, at } => mining::vein_back(self, net, template, at),
                Timer::QuestMine { world, work } => {
                    mining::quest_mine_done(self, net, world, &work, clock);
                }
                Timer::QuestRunsOut {
                    world,
                    quest,
                    started,
                } => quests::runs_out(self, net, world, quest, started, clock),
                Timer::SentAway { world, to } => places::sent_away(self, net, world, to, clock),
                Timer::Interaction { world, used } => {
                    usable::interactions::played(self, net, world, &used, clock);
                }
                Timer::ItemUse { world, using } => {
                    item_use::played(self, net, world, &using, clock);
                }
                Timer::PlantThinks { plant } => plants::thinks(self, net, plant, clock),
                Timer::Harvest { world, used } => {
                    plants::harvested(self, net, world, &used, clock);
                }
                Timer::PickUp { world, item } => {
                    inventory::picked_up(self, net, world, item, clock);
                }
                Timer::Landing(l) => combat::lands(self, net, l, clock),
                Timer::BindEnds { world } => places::bind_ends(self, net, world),
                Timer::Opening { creature, closes } => {
                    skill_use::opening(self, net, creature, closes, clock);
                }
                Timer::PutDown { world, slot, asked } => {
                    drops::put(self, net, world, slot, &asked, clock);
                }
                Timer::Settle { item } => drops::settle(self, net, item, clock),
                Timer::Examine { world, number } => {
                    requests::examine_again(self, net, world, number, clock);
                }
                Timer::ShardProtectionEnds { world, eid } => {
                    scenes::protection_ends(self, net, world, eid);
                }
                Timer::Asking { leader, recruit } => {
                    fellowship::asking_ends(self, net, leader, recruit, clock);
                }
                Timer::Door {
                    door,
                    generation,
                    step,
                } => usable::doors::step(self, net, door, generation, step, clock),
                Timer::Chest {
                    chest,
                    generation,
                    step,
                } => usable::chests::step(self, net, chest, generation, step, clock),
                Timer::MountIndoors { world, eid } => mounts::indoors_due(self, net, world, eid),
                Timer::LogOff { world, step } => world::log_off_step(self, net, world, step, clock),
                Timer::Shutdown { left } => shutdown::step(self, net, left, clock),
                Timer::ShutdownEnds => shutdown::ends(self),
            }
        }
        let due = self
            .last_step
            .is_none_or(|t| clock.local.0 - t.0 >= spawn::STEP_SECONDS);
        if due {
            self.last_step = Some(clock.local);
            spawn::step(self, net, clock);
        }
        fellowship::tick(self, net, clock.local);
        trade::look(self, net, clock);
        market::look(self, net, clock);
        storms::look(self, net, clock);
    }

    /// Have something happen `seconds` from now.
    pub fn after(&mut self, clock: Clock, seconds: f64, t: Timer) {
        self.timers.push((LocalTime(clock.local.0 + seconds), t));
    }

    /// Handle one network event.
    pub fn handle(&mut self, net: &mut ServerNet, ev: NetEvent, clock: Clock) {
        match ev {
            NetEvent::LoggedIn { conn, login } => login::logged_in(self, net, conn, &login, clock),
            NetEvent::WorldJoined {
                conn,
                login,
                cookie,
            } => login::world_joined(self, net, conn, login, cookie, clock),
            NetEvent::Message {
                conn, queue, bytes, ..
            } => {
                if net.service(conn).is_none() {
                    // A message that arrived with the one that closed its connection (a world
                    // connection closed by the log-off beside it).
                } else if self.worlds.contains_key(&conn) {
                    world::message(self, net, conn, queue, &bytes, clock);
                } else if self.players.contains_key(&conn) && login::is_world_bound(queue, &bytes) {
                    // No referral: the world travels on the login connection.
                    world::message(self, net, conn, queue, &bytes, clock);
                } else {
                    login::message(self, net, conn, queue, &bytes, clock);
                }
            }
            NetEvent::Closed { conn, why } => login::closed(self, net, conn, why, clock),
        }
    }
}
