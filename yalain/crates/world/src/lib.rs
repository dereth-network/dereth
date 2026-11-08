//! The game server's state and rules, sans-IO: accounts, characters, the world, its creatures and
//! its play, driven by the server's network events and an injected clock.
//!
//! **Depends on** the messages (`auberean-protocol`), the script machine's values
//! (`auberean-script`), the client's written form of names and its word patterns
//! (`auberean-text`, `auberean-assets`), the client's table of record kinds and its data files
//! (`auberean-dat`), the overlay folder a world's own records are sent from (`dereth-dat`),
//! `auberean-primitives`, the server's connections (`yalain-net`), the store's
//! trait (`yalain-store`), the content (`yalain-content`), the shared physics its creatures move through
//! (`auberean-physics`, `auberean-world`, `dereth-physics`), the chat service's packet
//! (`dereth-protocol`), the original's float-to-integer conversion (`dereth-primitives`), the
//! records' compression (`flate2`) and the account secrets' hashes (`bcrypt`; `sha2` and `subtle`
//! for the form earlier versions kept).
//! **Used by** the server executable (`yalain`) and its test kit (`yalain-testkit`).
//!
//! **Must never** touch a socket, a file or the clock: the host feeds it the network's events and
//! the time ([`Clock`]), and it answers through the network it is handed. The one thing it takes
//! from the system is a new account secret's salt, from its random source.
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
//! - [`kingdoms`]: a player's kingdom, joined at a faction shrine, and the kind of player conflict
//!   each place allows, which the client is told of as its player comes and goes.
//! - [`plants`]: dye plants: planted from seeds, cared for, pollinated and harvested.
//! - [`mounts`]: saddles and mount eggs put their user on a beast; riding, dismounting and the
//!   ride's end.
//! - [`usable`]: doors, chests, their locks and keys, surveyor's maps, items used on the objects
//!   their interaction tables name (a hoard's key on its stockpile).
//! - [`made`]: what a recipe's item level makes a product carry of its own.
//! - [`treasure`]: what a treasure item carries of its own, chosen as it drops.
//! - [`walkers`]: placed people walking their routes.
//! - [`reload`]: the world database taken again while the server runs.
//! - [`saves`], [`shutdown`]: every character in the world kept every few minutes; the world shut
//!   down, its countdown, cancelled or cut short.
//! - [`admin`]: the operator's moderation: access levels, accounts made and changed while the
//!   world runs, bans, gags and boots, at the console and said in game by staff.
//! - [`characters`], [`answers`]: the operator's characters (made with a build, given one, placed,
//!   deleted, carried between worlds, teleported), and the fixed form of the log lines a program
//!   driving the console reads.
//! - [`unhandled`]: what clients reached for that the server does not do yet, counted.
//! - [`call_limit`]: how fast one connection may send game calls.
//! - [`guard`]: a panic in the world stops it, whole.

pub mod admin;
pub mod admin_calls;
pub mod admin_groups;
pub mod admin_library;
pub mod ai;
pub mod allegiance;
pub mod answers;
pub mod arms;
pub mod books;
pub mod butchery;
pub mod call_limit;
pub mod calls;
pub mod characters;
pub mod chargen;
pub mod chat;
pub mod combat;
pub mod craft;
pub mod death;
pub mod death_spawn;
pub mod download;
pub mod drops;
pub mod effect_defs;
pub mod effects;
pub mod equip;
pub mod examine;
pub mod fellowship;
pub mod focus;
pub mod follow;
pub mod gives;
pub mod graph;
pub mod guard;
pub mod inventory;
pub mod item_use;
pub mod kingdoms;
pub mod levels;
pub mod login;
pub mod made;
pub mod makes;
pub mod map_trips;
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
pub mod resurrections;
pub mod rng;
pub mod rules;
pub mod saves;
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
pub mod treasure;
pub mod unhandled;
pub mod uploads;
pub mod usable;
pub mod walkers;
pub mod walls;
pub mod workshops;
pub mod world;
pub mod world_patch;
pub mod xp;

use std::collections::{HashMap, HashSet};
use std::net::IpAddr;

use yalain_content::{Catalog, ContentPack, LootTable, SpawnTable};
use yalain_net::{ConnId, LocalTime, NetEvent, ServerNet};
use yalain_store::{Account, Store, StoreResult};

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
    /// The admin commands kept to named groups: each command's word, and the groups that may give
    /// it. A command listed nowhere is open to every admin account.
    pub admin_commands: Vec<(String, Vec<String>)>,
    /// How many characters an account may have.
    pub slots: u32,
    /// How long a deleted character stays in the list, greyed out, before it is gone.
    pub delete_seconds: u32,
    /// Send a referral to the world service when a character enters (as 2017 did); without,
    /// the world comes on the login connection.
    pub referral: bool,
    /// Send each player its own position and head back as it reports them.
    pub echo_positions: bool,
    /// Hold what a player reports of its place to what a body can do: a land cell its point does not
    /// lie in is ignored, and a player told it stands where it stood when a place is further than it
    /// could have gone, as the 2017 servers told a client gone astray. An admin character is not
    /// held.
    pub holds_reports: bool,
    /// The seed of the world's dice.
    pub seed: u64,
    /// What the operator chose for a world's overlay.
    pub dat_overlay: world_patch::DatOverlaySettings,
    /// The bcrypt cost account secrets are kept at ([`login::SECRET_COST`] unless the operator
    /// gives another).
    pub secret_cost: u32,
    /// Seconds between a character's saves while it is in the world ([`saves::SAVE_SECONDS`]
    /// unless the operator gives another).
    pub save_seconds: u32,
    /// Seconds a shutdown counts down ([`shutdown::LEAD`] unless the operator gives another).
    pub shutdown_seconds: u32,
    /// How many logins the world holds at once ([`login::MAX_SESSIONS`] unless the operator
    /// gives another); a login past them is told the world is full.
    pub max_sessions: u32,
    /// The seconds a character must have played before it sends to the Dereth Network: tells,
    /// room lines and channels (0: none, the default).
    pub min_character_age_seconds: u32,
    /// How many logins one address holds at once; `None` for no limit (the default).
    pub max_sessions_per_address: Option<u32>,
    /// Addresses no per-address limit holds back.
    pub unlimited_addresses: Vec<IpAddr>,
    /// The operator's message of the day, shown to each player as its character enters the
    /// world, in one notice (empty: none).
    pub motd: String,
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
            holds_reports: true,
            seed: 0x5EED_00A2,
            dat_overlay: world_patch::DatOverlaySettings::default(),
            secret_cost: login::SECRET_COST,
            save_seconds: saves::SAVE_SECONDS,
            shutdown_seconds: shutdown::LEAD,
            admin_commands: Vec::new(),
            max_sessions: login::MAX_SESSIONS,
            min_character_age_seconds: 0,
            max_sessions_per_address: None,
            unlimited_addresses: Vec::new(),
            motd: String::new(),
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
    /// The addresses whose logins were refused, and those refused unchecked for a while.
    pub login_guard: login::LoginGuard,
    /// The gags of the characters in the world.
    pub gags: admin::Gags,
    /// The admin calls refused.
    pub admin_calls: admin_calls::AdminCalls,
    /// The chat rooms admins made.
    pub admin_rooms: admin_calls::Rooms,
    /// The script library admin accounts' retail clients are served, when the operator serves one,
    /// and the logins waiting for it to be stored before they see their characters.
    pub admin_library: Option<admin_library::AdminLibrary>,
    pub storing_library: std::collections::HashSet<ConnId>,
    /// The world's overlay as clients are patched to it, when it has one; the clients that said
    /// they keep overlays, and those being patched.
    pub world_patch: Option<std::sync::Arc<world_patch::WorldPatch>>,
    pub overlay_clients: HashMap<ConnId, auberean_protocol::login::OverlayHeld>,
    pub patches: world_patch::Patches,
    /// World connections and the login connection each belongs to.
    pub worlds: HashMap<ConnId, ConnId>,
    /// The characters in the world, by the connection their world travels on.
    pub players: HashMap<ConnId, Player>,
    /// Whether the world is shutting down.
    pub shutdown: shutdown::Shutdown,
    /// What panicked, once something has: the world's memory is not saved after that.
    pub panicked: Option<String>,
    /// When each character in the world was last kept.
    pub saves: saves::Saves,
    /// What is to happen later, by when.
    pub timers: Vec<(LocalTime, Timer)>,
    /// The last context number a behavior or movement order carried.
    pub next_context: u32,
    /// The last visual stamp each content object's visual events used (made with 1).
    pub visuals: HashMap<u64, u16>,
    pub unhandled: Unhandled,
    /// How fast each connection has been sending game calls.
    pub call_limits: call_limit::CallLimits,
    /// The Dereth Network's commands said in chat since the host last took them: heard by
    /// nobody, answered by the world's friends service.
    pub network_commands: Vec<chat::NetworkCommand>,
    /// The tells that reached no player in the world since the host last took them.
    pub unheard_tells: Vec<chat::UnheardTell>,
    /// The characters renamed since the host last took them.
    pub renames: Vec<player::Renamed>,
    /// The lines said in the rooms every player has, kept for the world's link.
    pub room_lines: Vec<chat::RoomSaid>,
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
    /// The creatures fighting at the world's last step: those [`ai::attacks_due`] looks at.
    pub fighting: Vec<u64>,
    /// The ground's height outdoors, when the host knows the land.
    pub ground: Option<Box<dyn Ground>>,
    /// The walls, floors and solid things creatures meet, when the host knows the world.
    pub walls: Option<walls::Walls>,
    /// The data files a client's requests for records are served from, when the host has them.
    pub data: Option<Box<dyn download::DataFiles>>,
    /// What answering requests for records has cost, and the answers made ahead of time.
    pub downloads: download::Downloads,
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
    /// The enumeration of quests: each quest's id by its name, for the effects that name quests.
    pub quest_ids: HashMap<String, u32>,
    /// What each player is examining.
    pub examining: HashMap<ConnId, examine::Examining>,
    /// The players with a look at it waiting.
    pub examine_waits: HashSet<ConnId>,
    /// The packages of the objects a player uses by walking into them (portals).
    pub walk_in_packages: std::collections::HashSet<u32>,
    /// The packages of the world's people and objects that answer a use as a person does (those
    /// derived from the script library's person), and of its faction shrines.
    pub person_packages: std::collections::HashSet<u32>,
    /// The script packages of the world's lifestones.
    pub lifestone_packages: std::collections::HashSet<u32>,
    /// The portal storms of crowded landblocks.
    pub storms: storms::Storms,
    pub shrine_packages: std::collections::HashSet<u32>,
    /// The kingdom each faction shrine's use joins, by the shrine's package.
    pub shrine_kingdoms: HashMap<u32, u32>,
    /// The packages of the world's objects that cannot be used (forges, road signs).
    pub cannot_use_packages: std::collections::HashSet<u32>,
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
    /// What creatures' death effects make, by the effects' names.
    pub death_spawns: HashMap<String, yalain_content::DeathSpawn>,
    /// Those made so far that live, by the effects' names.
    pub death_spawned: HashMap<String, Vec<u64>>,
    /// The effects creatures' templates put on whoever kills them.
    pub death_effects: item_use::ItemUses,
    /// The packages whose items are destroyed on use by their kind (keys, potions, ...).
    pub destroyed_on_use: std::collections::HashSet<u32>,
    /// The packages of the treasure items that carry properties of their own (equipment, potions).
    pub treasure: treasure::Kinds,
    /// The packages of the Tesserae (each use a hero skill credit more).
    pub tesserae: std::collections::HashSet<u32>,
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
    /// The workshops standing in the world.
    pub workshops: workshops::Workshops,
    pub market: market::Market,
    /// The items players have put down, and when each goes.
    pub lying: drops::Lying,
    /// The pets players have summoned.
    pub pets: pets::Pets,
    /// The dye plants players have planted, and the species' ripe looks.
    pub plants: plants::Plants,
    /// The locked doors and their keys, and the doors' states.
    pub locks: yalain_content::LockTable,
    /// The places templates name: where a person who sends players by their maps sends the
    /// holder of a map of the template.
    pub places_named: std::collections::BTreeMap<u32, yalain_content::Place>,
    /// The places by their names (lowercase): where a city's recall sends a player.
    pub places_by_name: std::collections::BTreeMap<String, yalain_content::Place>,
    pub doors: usable::doors::Doors,
    /// The chests' states.
    pub chests: usable::chests::Chests,
    /// The mounts: their effects, the items that mount their users, the mounted looks.
    pub mounts: mounts::Mounts,
    /// The item templates' interaction tables: what each item may be used on, and to what end.
    pub interactions: usable::interactions::Interactions,
    /// The placed people walking their routes.
    pub walkers: walkers::Walkers,
    /// What reloading the world database keeps of the placed objects and camps between reloads.
    pub placed: reload::Placed,
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
    /// What a world's row makes has stood its time.
    MadeEnds { creature: u64 },
    /// A creature's heal or buff on itself takes effect.
    CreatureHelps { creature: u64, skill: u32, on: u64 },
    /// A creature's strike reaches the player it was at.
    CreatureStrikeLands {
        creature: u64,
        target: ConnId,
        harm: effects::Harm,
    },
    /// A strike between a creature and a pet reaches its target.
    ThingStrikeLands {
        by: u64,
        target: u64,
        skill: u32,
        harm: effects::Harm,
    },
    /// A dead player's corpse is made.
    PlayerCorpse { world: ConnId },
    /// A character that left the world dead comes back.
    BackAfterEntry { world: ConnId },
    /// A player left dead since this corpse of its is sent back.
    SentBack { world: ConnId, corpse: u64 },
    /// A player's lifestone protection runs out.
    ProtectionEnds { world: ConnId, eid: u32 },
    /// The next step of a player's use of a skill (the server's number for the use).
    Skill {
        world: ConnId,
        serial: u32,
        step: skill_use::Step,
    },
    /// A creature's opening opens, or closes.
    Opening { creature: u64, closes: bool },
    /// An effect on a player runs out.
    EffectEnds { world: ConnId, eid: u32 },
    /// The look of the effects on an item may be up.
    ItemEffectEnds { item: u64, fx: u32 },
    /// A skill's effects reach the item it was aimed at, after its use is over.
    SkillOnItemLands {
        world: ConnId,
        skill: u32,
        item: u64,
        spellcraft: f32,
    },
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
    /// Back from a resurrection, the auras it held pulse again.
    AurasPulseAgain { world: ConnId },
    /// A player another raised may move.
    RaisedMoves { world: ConnId },
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
    /// A player's handing over of an item it gives is through.
    Handed { world: ConnId },
    /// A person's handing back of an item given it is through.
    HandedBack {
        world: ConnId,
        asked: Box<inventory::Move>,
        /// Where the giver is sent as the item is back (a map handed to a person who sends
        /// players where their maps lead).
        to: Option<yalain_store::StoredPosition>,
    },
    /// A give's quest step, due after its wait.
    GiveStep {
        world: ConnId,
        step: Box<yalain_content::quests::Step>,
    },
    /// A player's use of an item on an object, its behavior played out.
    Interaction {
        world: ConnId,
        used: Box<places::Use>,
    },
    /// A dye plant thinks: its look, its needs.
    PlantThinks { plant: u64 },
    /// A player's join of a kingdom at a faction shrine is made.
    KingdomJoin { world: ConnId, kingdom: u32 },
    /// A player who joined a kingdom is sent its channels again.
    KingdomChannels { world: ConnId },
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
    Examine { world: ConnId },
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
    /// A connection told its login is over (a boot, a ban) is closed.
    Close { conn: ConnId },
}

impl<S: Store> Server<S> {
    /// A server over this store, with the first ids this run gives set aside there.
    ///
    /// # Errors
    /// The store will not set them aside: a world without them could give an id a kept item holds.
    pub fn new(cfg: ServerConfig, mut store: S, rules: CharGenRules) -> StoreResult<Self> {
        let cfg_seed = cfg.seed;
        let mut things = things::Things::default();
        let first = store.reserve_thing_numbers(things::ID_BLOCK)?;
        things.add_block(first, things::ID_BLOCK);
        Ok(Self {
            cfg,
            store,
            rules,
            names: names::NameRules::default(),
            start: StartRules::default(),
            content: ContentPack::default(),
            sessions: HashMap::new(),
            login_guard: login::LoginGuard::default(),
            gags: admin::Gags::default(),
            admin_calls: admin_calls::AdminCalls::default(),
            admin_rooms: admin_calls::Rooms::default(),
            admin_library: None,
            storing_library: std::collections::HashSet::new(),
            world_patch: None,
            overlay_clients: HashMap::new(),
            patches: world_patch::Patches::default(),
            worlds: HashMap::new(),
            players: HashMap::new(),
            shutdown: shutdown::Shutdown::No,
            panicked: None,
            call_limits: call_limit::CallLimits::default(),
            saves: saves::Saves::default(),
            timers: Vec::new(),
            next_context: 0,
            visuals: HashMap::new(),
            unhandled: Unhandled::default(),
            network_commands: Vec::new(),
            unheard_tells: Vec::new(),
            renames: Vec::new(),
            room_lines: Vec::new(),
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
            things,
            camps: Vec::new(),
            fighting: Vec::new(),
            ground: None,
            walls: None,
            data: None,
            downloads: download::Downloads::default(),
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
            quest_ids: HashMap::new(),
            examining: HashMap::new(),
            examine_waits: HashSet::new(),
            walk_in_packages: std::collections::HashSet::new(),
            person_packages: std::collections::HashSet::new(),
            lifestone_packages: std::collections::HashSet::new(),
            storms: storms::Storms::default(),
            shrine_packages: std::collections::HashSet::new(),
            shrine_kingdoms: HashMap::new(),
            cannot_use_packages: std::collections::HashSet::new(),
            inscribable_packages: std::collections::HashSet::new(),
            held_answers: std::collections::HashMap::new(),
            vault_trips: usable::vaults::VaultTrips::new(),
            creature_weapons: HashMap::new(),
            creature_weapon_rules: HashMap::new(),
            death_spawns: HashMap::new(),
            death_spawned: HashMap::new(),
            death_effects: HashMap::new(),
            destroyed_on_use: std::collections::HashSet::new(),
            treasure: treasure::Kinds::default(),
            tesserae: std::collections::HashSet::new(),
            touching: HashMap::new(),
            allegiances: allegiance::Allegiances::default(),
            fellowships: fellowship::Fellowships::default(),
            trades: trade::Trades::default(),
            story: yalain_content::StoryTable::default(),
            brokers: std::collections::BTreeMap::new(),
            workshops: workshops::Workshops::default(),
            market: market::Market::default(),
            lying: drops::Lying::default(),
            pets: pets::Pets::default(),
            plants: plants::Plants::default(),
            locks: yalain_content::LockTable::default(),
            places_named: std::collections::BTreeMap::new(),
            places_by_name: std::collections::BTreeMap::new(),
            doors: usable::doors::Doors::default(),
            chests: usable::chests::Chests::default(),
            mounts: mounts::Mounts::default(),
            interactions: usable::interactions::Interactions::default(),
            walkers: walkers::Walkers::default(),
            placed: reload::Placed::default(),
        })
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
        let veins = self
            .content
            .objects
            .iter()
            .filter(|o| {
                self.catalog
                    .get(o.template)
                    .is_some_and(mining::is_craft_node)
            })
            .cloned()
            .collect();
        mining::place_veins(&mut self);
        self.workshops = workshops::Workshops::of(&self.content.objects, &self.catalog);
        self.placed.loaded = Some(self.content.objects.clone());
        self.placed.veins = veins;
        self
    }

    /// Stand what the world database places among the walls creatures meet.
    pub fn stand_placed(&mut self) {
        let rows = walls_standing(self);
        if let Some(w) = self.walls.as_mut() {
            w.set_standing(rows);
        }
    }

    /// Handle every event the network has, then whatever is due.
    pub fn pump(&mut self, net: &mut ServerNet, clock: Clock) {
        self.saves.now = clock.local;
        while let Some(ev) = net.poll_event() {
            self.saves.piece_begins();
            self.handle(net, ev, clock);
        }
        self.tick(net, clock);
    }

    /// Run what is due by now, and the world's step when one is due.
    pub fn tick(&mut self, net: &mut ServerNet, clock: Clock) {
        self.game_now = net.game_time(clock.local);
        world_patch::send_due(self, net, clock);
        while let Some(i) = self.timers.iter().position(|(at, _)| at.0 <= clock.local.0) {
            let (_, t) = self.timers.remove(i);
            self.saves.piece_begins();
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
                Timer::MadeEnds { creature } => death_spawn::ends(self, net, creature),
                Timer::CreatureHelps {
                    creature,
                    skill,
                    on,
                } => {
                    missiles::helped(self, net, creature, skill, on, clock);
                }
                Timer::CreatureStrikeLands {
                    creature,
                    target,
                    harm,
                } => combat::creature_strike_lands(self, net, creature, target, harm, clock),
                Timer::ThingStrikeLands {
                    by,
                    target,
                    skill,
                    harm,
                } => combat::thing_strike_lands(self, net, (by, skill), target, harm, clock),
                Timer::PlayerCorpse { world } => death::player_corpse(self, net, world, clock),
                Timer::BackAfterEntry { world } => death::back_after_entry(self, net, world, clock),
                Timer::SentBack { world, corpse } => {
                    death::sent_back(self, net, world, corpse, clock);
                }
                Timer::ProtectionEnds { world, eid } => effects::runs_out(self, net, world, eid),
                Timer::Skill {
                    world,
                    serial,
                    step,
                } => {
                    skill_use::step(self, net, world, serial, step, clock);
                }
                Timer::ItemEffectEnds { item, fx } => {
                    effects::item_effect_ends(self, net, item, fx, clock);
                }
                Timer::SkillOnItemLands {
                    world,
                    skill,
                    item,
                    spellcraft,
                } => {
                    if let Some(u) = self
                        .skill_table
                        .skills
                        .get(&skill)
                        .and_then(|d| d.usage.clone())
                    {
                        skill_use::lands_on_item(self, net, world, &u, item, spellcraft, clock);
                    }
                }
                Timer::EffectEnds { world, eid } => {
                    if !mounts::record_ends(self, net, world, eid) {
                        effects::ends(self, net, world, eid, clock);
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
                Timer::RaisedMoves { world } => resurrections::raised_moves(self, net, world),
                Timer::AurasPulseAgain { world } => {
                    effects::auras_pulse_again(self, net, world, clock);
                }
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
                Timer::KingdomJoin { world, kingdom } => {
                    kingdoms::join(self, net, world, kingdom, clock);
                }
                Timer::KingdomChannels { world } => chat::send_channels(self, net, world, 1),
                Timer::Harvest { world, used } => {
                    plants::harvested(self, net, world, &used, clock);
                }
                Timer::PickUp { world, item } => {
                    inventory::picked_up(self, net, world, item, clock);
                }
                Timer::Handed { world } => gives::handed(self, net, world, clock),
                Timer::HandedBack { world, asked, to } => {
                    gives::back_and_away(self, net, world, &asked, to, clock);
                }
                Timer::GiveStep { world, step } => {
                    gives::step_due(self, net, world, &step, clock);
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
                Timer::Examine { world } => requests::examine_again(self, net, world, clock),
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
                Timer::Close { conn } => {
                    if net.service(conn).is_some() {
                        net.close(conn);
                    }
                }
            }
        }
        let due = self
            .last_step
            .is_none_or(|t| clock.local.0 - t.0 >= spawn::STEP_SECONDS);
        self.saves.piece_begins();
        login::checks_due(self, net, clock);
        self.saves.piece_begins();
        if due {
            self.last_step = Some(clock.local);
            spawn::step(self, net, clock);
        }
        self.saves.piece_begins();
        // A swing comes when it is due, not at the world's next step (2017: 1,880 of 2,881 sword
        // swings within 0.15 of 1.64 s over the time scale, less than a world step either side).
        combat::swing_all(self, net, clock);
        ai::attacks_due(self, net, clock);
        self.saves.piece_begins();
        saves::tick(self, clock);
        admin::tick(self, net, clock);
        self.saves.piece_begins();
        fellowship::tick(self, net, clock.local);
        self.saves.piece_begins();
        trade::look(self, net, clock);
        self.saves.piece_begins();
        market::look(self, net, clock);
        market::look_for_expired(self, net);
        self.saves.piece_begins();
        drops::look(self, net, clock);
        self.saves.piece_begins();
        storms::look(self, net, clock);
        self.saves.piece_begins();
        presence::views_owed(self, net, clock);
        self.saves.piece_begins();
        saves::end_of_pass(self, net, clock);
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
            NetEvent::Closed { conn, why } => {
                self.downloads.closed(conn);
                login::closed(self, net, conn, why, clock);
            }
        }
    }
}

/// What the world database places that creatures meet: people and objects that stay where they are
/// for every player. A person on a route moves, a vein comes and goes, a door is met where it
/// stands and what a quest gates is there for some players only.
#[must_use]
pub fn walls_standing<S: Store>(s: &Server<S>) -> Vec<walls::Standing> {
    s.content
        .objects
        .iter()
        .enumerate()
        .filter(|(_, o)| {
            o.kind != yalain_content::Kind::Creature
                && o.walks.is_none()
                && o.gate.is_none()
                && !s.catalog.get(o.template).is_some_and(mining::is_craft_node)
        })
        .filter(|(i, _)| {
            usable::doors::door(s, presence::content_id(&s.content.objects, *i)).is_none()
        })
        .map(|(i, o)| walls::Standing {
            id: presence::content_id(&s.content.objects, i),
            template: o.template,
            look: o.description.look,
            scale: o.description.scale.unwrap_or(1.0),
            at: yalain_store::StoredPosition {
                cell: o.cell,
                origin: o.origin,
                rotation: o.rotation,
            },
            ethereality: walls::ethereality_of(&o.description),
        })
        .collect()
}
