//! The client without a window: the session, the model, the scripts with the model's natives, the
//! player's body and its reports, and what the client says on its own.
//!
//! **Depends on** the session (`auberean-client-net`), the model (`auberean-client-model`), the
//! messages (`auberean-protocol`), the script machine and its natives (`auberean-script`,
//! `auberean-glue`), the data files (`auberean-dat`, `auberean-assets`), `auberean-primitives`,
//! the contract's actions (`auberean-client-contract`), localized text (`auberean-text`), the
//! bodies' collision state and the missiles (`auberean-physics`), the visual descriptions in the
//! client's own form (`auberean-world`), the shared math (`dereth-primitives`) and the chat
//! service's packets (`dereth-protocol`). **Used by** the SDK (`auberean-client-sdk`) and through
//! it every front end.
//!
//! A [`Client`] takes datagrams and actions in and gives datagrams out; the host owns the socket
//! and the clock and calls [`Client::frame`] once a frame. Each frame, as the September 2013 client
//! does: the session runs its clocks and reads its queues; each message goes through the event
//! queue and takes effect on the model in the queue's order, and a call into the client's scripts
//! runs there and then; the event queue gives up on objects that never came; the body moves and
//! reports itself; the scene, once drawn, is reported.
//!
//! The player's requests in the world (selecting, using, attacking, the examination panel,
//! saying) are asked of the player's own scripts, as the game's panels ask them ([`interface`]):
//! what the scripts send is what goes to the server. A combat story's hooks ask the attacker's
//! and the target's scripts what a hit shows ([`combat`]).
//!
//! What the client sends on its own:
//!
//! - its **data files' versions** when the server asks ([`data`]);
//! - a request to send an object again when messages waited too long for it (the event queue's
//!   rule);
//! - the player's **position** by the report rule ([`motion`]);
//! - **the scene is drawn** after the player is created, after a teleport, and when the player
//!   leaves portal space: once the player's object is here and the player is not in portal space.
//!   This client has no frame to draw, so the scene counts as drawn in the frame those conditions
//!   hold.
//!
//! **Must never** do I/O or read a clock: datagrams, files and times come from the host.

pub mod acts;
pub mod body;
pub mod cache;
mod clock;
pub mod combat;
pub mod craft;
pub mod creation;
pub mod data;
pub mod description;
pub mod download;
pub mod effects;
pub mod examination;
pub mod files;
pub mod friends;
pub mod game_time;
pub mod host;
pub mod interface;
pub mod inventory;
pub mod latent;
pub mod lifecycle;
pub mod log_system;
pub mod looks;
pub mod motion;
pub mod notices;
pub mod physics_object;
pub mod quality_dump;
pub mod repository;
pub mod rooms;
pub mod selection;
pub mod shell;
pub mod skills;
pub mod social;
pub mod store;
pub mod strings;
pub mod text;
pub mod toolbar;
pub mod world;

use dereth_primitives::num::math;
use std::collections::BTreeMap;
use std::sync::Arc;

use core::net::SocketAddr;

use auberean_client_contract::{Action, CraftAction, NewCharacter, PlayerOption, SkillAction};
use auberean_client_model::{Effect, EventQueue, Item, Model};
use auberean_client_net::logon::Logon;
use auberean_client_net::{Event, Incoming, LocalTime, Session, Settings};
use auberean_dat::DatFile;
use auberean_primitives::Vec3;
use auberean_protocol::login::{
    CharacterRef, CreateCharacter, DataRequest, DeleteCharacter, Empty, PlayCharacter,
    RestoreCharacter,
};
use auberean_protocol::physics::{HeadDirection, Target};
use auberean_protocol::script::{ScriptArgs, ServerCall};
use auberean_protocol::table::Table;
use auberean_protocol::text::{NarrowString, WideString};
use auberean_protocol::{ClientMessage, ServerMessage};
use auberean_script::value::FunctionId;
use auberean_script::Program;

pub use auberean_glue::chat_options;

pub use host::{CallResult, InterfaceArg, Scripts};
use interface::function;

/// The server's call that puts the player in portal space, and the one that takes it out.
pub const ENTER_PORTAL_SPACE: u32 = 0x8212_0020;
pub const EXIT_PORTAL_SPACE: u32 = 0x8212_001B;

/// The server's events for an object's body that the scene plays ([`Client::physics`]): a
/// behavior played and stopped, a change of mode (gone through, or set at once), an effect played
/// and stopped, a combat story, a move ordered, the scales of its speed, its acceleration and its
/// jump, and where it looks (what at, and its head's direction).
pub const PHYSICS_EVENTS: [u32; 29] = [
    0x1007F, 0x10080, 0x10082, 0x100A9, 0x100A0, 0x100A1, 0x100A8, 0x10083, 0x10092, 0x10093,
    0x100BA, 0x10096, 0x100C9, 0x10081, 0x10091, 0x10094, 0x10095, 0x10098, 0x10099, 0x1009B,
    0x100A2, 0x100A5, 0x100A6, 0x100A7, 0x100B5, 0x100BE, 0x100C5, 0x100CF, 0x100D2,
];
/// How many of them are kept for a scene that has not taken them.
pub const PHYSICS_KEPT: usize = 1024;
/// How many of the player's selection looks are kept for a scene that has not taken them.
pub const LOOKS_KEPT: usize = 64;
/// The server is told where the player's head points at most this often, seconds (more than this
/// apart), and only when it moved more than [`motion::EPSILON`].
pub const HEAD_INTERVAL: f64 = 0.5;

/// What the client needs to start.
#[derive(Clone, Debug)]
pub struct ClientSettings {
    pub login_server: SocketAddr,
    /// The account, as typed.
    pub account: Vec<u8>,
    pub ticket: Vec<u8>,
    /// Seconds since 1970 when the login starts.
    pub timestamp: u32,
    /// The encrypted session key's 64 bytes.
    pub session_key: [u8; 64],
    /// Whether what the client's scripts send reaches the server (a replay keeps them back and
    /// sends the recorded player's instead).
    pub script_sends: bool,
}

/// The client's data: its data files, in the order the client opens them, and the script library
/// linked against the client's natives.
#[derive(Clone, Default)]
pub struct ClientData {
    pub files: data::DataFiles,
    /// The portal file and the linked program, when the scripts are to run.
    pub scripts: Option<(Arc<DatFile>, Arc<Program>)>,
}

impl core::fmt::Debug for ClientData {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ClientData")
            .field("files", &self.files)
            .field("scripts", &self.scripts.is_some())
            .finish()
    }
}

/// Link the script library of a portal file against the client's natives: the library is the
/// record the client's enumeration names (no other is ever chosen: the program's options to read a
/// library from a named file are kept and ignored), read by the data's own resolver, its natives
/// dispatched by the client's table and its objects made by the client's factory. A read or decode
/// that fails is checked against the file and read once more.
///
/// # Errors
/// The library is not there, or does not load or link.
pub fn link_scripts(portal: &DatFile) -> Result<Program, String> {
    link_scripts_noting(portal, |_| {})
}

/// [`link_scripts`], `note` told when the library read right only the second time, for the host to
/// say so.
///
/// # Errors
/// The library is not there, or does not load or link.
// D2-00741, D2-07978, D2-08022, D2-07998, D2-07997, D2-07992, D2-07993, D2-00748, D2-00749
// D2-01657
pub fn link_scripts_noting(
    portal: &DatFile,
    note: impl FnOnce(&auberean_dat::checked::Retried),
) -> Result<Program, String> {
    let lib = portal
        .read_checked(
            auberean_script::library::RECORD,
            auberean_script::library::Library::from_record,
        )
        .map_err(|e| format!("the script library: {e}"))?;
    if let Some(r) = &lib.retried {
        note(r);
    }
    let lib = lib.value;
    let natives = host::natives(&lib);
    Program::link(lib, natives).map_err(|e| format!("{e:?}"))
}

/// One message the client sent, and when.
#[derive(Clone, Debug, PartialEq)]
pub struct Sent {
    pub at: f64,
    pub message: ClientMessage,
}

/// How the calls from the server went, counted.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CallTally {
    pub calls: u64,
    pub completed: u64,
    pub ended: u64,
    pub refused: u64,
    pub null_reference: u64,
    pub no_receiver: u64,
    /// Stopped at a native not written yet, by native.
    pub not_ported: BTreeMap<u32, u64>,
    /// Stopped for another reason, by reason, with a count.
    pub stopped: BTreeMap<String, u64>,
}

impl CallTally {
    fn add(&mut self, r: &CallResult) {
        self.calls += 1;
        match r {
            CallResult::Completed => self.completed += 1,
            CallResult::Ended(_) => self.ended += 1,
            CallResult::Refused(_) => self.refused += 1,
            CallResult::NullReference => self.null_reference += 1,
            CallResult::NoReceiver => self.no_receiver += 1,
            CallResult::NotPorted(n) => *self.not_ported.entry(*n).or_default() += 1,
            CallResult::Stopped(s) => {
                let key: String = s.split([':', '(']).next().unwrap_or("").to_owned();
                *self.stopped.entry(key).or_default() += 1;
            }
        }
    }
}

/// A place the scripts stopped short, named for a report: what stopped them, the native by its
/// name when a native not written yet did, the function they stopped in and the code offset
/// there, and how many times.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptStop {
    pub what: auberean_script::native::Stopped,
    pub native: Option<String>,
    pub function: String,
    pub offset: u32,
    pub count: u64,
}

/// The client.
#[derive(Debug)]
pub struct Client {
    pub session: Session,
    pub queue: EventQueue,
    pub model: Model,
    /// The world the objects are kept in: its region, and whether their upkeep waits.
    pub world: world::ClientWorld,
    pub scripts: Option<Scripts>,
    pub body: Body,
    pub calls: CallTally,
    /// How the objects' scripts took being told of their qualities' changes, counted.
    pub stat_changes: CallTally,
    /// How the player's scripts took the server's lines of text, counted.
    pub server_texts: CallTally,
    /// Which objects are in the world as their scripts were told, and how the scripts took being
    /// told of their objects' lives (made, into the world, out of it, destroyed), counted.
    pub life: lifecycle::Life,
    pub lives: CallTally,
    /// How the saved states the data cache loaded were asked for their records, counted.
    pub states_named: auberean_glue::data_ids::Named,
    /// A scene places the objects' bodies: an object enters and leaves the world as the scene
    /// says ([`Client::set_in_world`]), not once it has a place and no parent.
    pub bodies_place: bool,
    /// The toolbar as the shortcut bar shows it, asked of the scripts again when it may have
    /// changed (the scripts said so, objects came or went, the player changed a slot or the set);
    /// the objects created and forgotten when it was last asked.
    toolbar: toolbar::Toolbar,
    toolbar_stale: bool,
    toolbar_seen: (u32, u32),
    /// The effects on the player as the effects panel lists them, asked of the scripts again
    /// when they said an effect changed, a session started or the player arrived (the player it
    /// was last asked for).
    effects: effects::EffectList,
    effects_stale: bool,
    effects_player: Option<u64>,
    /// The story's acts as the story page lists them, asked of the scripts again when they said
    /// the story changed or the player arrived (the player they were last asked for).
    acts: acts::ActList,
    acts_stale: bool,
    acts_player: Option<u64>,
    /// The connection as last measured, and how many times it was.
    heartbeat: Option<Heartbeat>,
    data: data::DataFiles,
    download: download::DataDownload,
    account: Vec<u8>,
    script_sends: bool,
    /// The scene waits to be reported drawn.
    scene_wanted: bool,
    /// Whether the host's last frame drew the world; none for a host that draws nothing. The server
    /// hears the scene is ready only once it has.
    pub world_frame_drawn: Option<bool>,
    portal_space: bool,
    /// The entry's session was ended: one log-off an entry into the world.
    ended: bool,
    /// How the session ended under the player, as far as it has.
    session_end: SessionEnd,
    /// The game clock.
    clock: clock::Clock,
    /// The world's clock over the region's calendar, from the first region on.
    game_clock: Option<auberean_world::calendar::GameClock>,
    now: f64,
    sent: Vec<Sent>,
    /// Calls from the server a host wants to see as they take effect, by function id.
    pub observe: Vec<u32>,
    /// The observed calls, with the time they took effect and the object a call on an object is
    /// for, for the host to take.
    pub observed: Vec<(f64, u32, Option<ScriptArgs>, Option<u64>)>,
    /// What the player asked of its scripts through the interface, and how each went.
    pub asked: Vec<(u32, CallResult)>,
    /// Calls waiting to go on: sleeping ones until the client's clock reaches their time, and
    /// ones whose wait was closed at the next upkeep (time negative infinity), earliest first.
    pub waiting: Vec<(f64, latent::Waiting)>,
    /// How the calls that went on after waiting went, counted.
    pub woken: CallTally,
    /// The lines the scripts gave the chat windows, in order, for a front end to take.
    pub shown: Vec<host::Shown>,
    /// The notices the scripts sent the panels, in order, for a front end to take.
    pub notices: Vec<host::PanelNotice>,
    /// What the server asked of the objects' bodies ([`PHYSICS_EVENTS`]: the behaviors they play,
    /// stop and let go of, their modes, sliders and motion descriptions, the effects they play and
    /// stop, the combat stories told on them, the moves it orders, the pushes it gives them, their
    /// speed, acceleration and jump scales, what the ground does to their motion, where they look,
    /// the missiles it launches), in the order
    /// it took effect, for the scene to play. When no scene takes them, the oldest
    /// [`PHYSICS_KEPT`] go once twice that many are kept (the heads' directions come many times a
    /// second).
    pub physics: Vec<ServerMessage>,
    /// The object that last attacked the player with a harmful skill, as its story told it (0 for
    /// none yet).
    pub last_attacker: u64,
    /// The option to look at each new selection.
    pub look_at_selection: bool,
    /// What the player looks at (0 for nothing): its selection, or what the server or its scripts
    /// said, until it stops looking.
    pub looking_at: u64,
    /// What the player was asked to look at by its selections since the scene last took them (0
    /// for nothing), in order: its head turns there. The last [`LOOKS_KEPT`] are kept.
    pub selection_looks: Vec<u64>,
    /// Where the player's head points (the pitch and the turn of its look sliders) as the world
    /// last moved it, once a world moves it; the server is told when it changes
    /// ([`HEAD_INTERVAL`]).
    pub head: Option<(f32, f32)>,
    /// The head as the server was last told, and when (game time).
    head_sent: Option<(f32, f32)>,
    head_sent_at: f64,
    /// The lines the player typed.
    pub shell: shell::History,
    /// What the skill window shows, as the scripts last answered; the player it was first asked
    /// for when the player arrived.
    pub skills: skills::Skills,
    skills_player: Option<u64>,
    /// What the crafting page shows, asked of the scripts again when they said the craft panel or
    /// a craft skill changed, or the player arrived (the player it was last asked for).
    craft: craft::Craft,
    craft_stale: bool,
    /// What the shop windows read, and whether to ask it again.
    store: store::Store,
    store_stale: bool,
    craft_player: Option<u64>,
    /// The recipe the ingredient panel has open, at a spinner value (its last when none), asked
    /// of the scripts again when they said something about it, its craft skill or the items.
    recipe_open: Option<(u32, Option<i32>)>,
    recipe_stale: bool,
    /// The inventory as the panels show it, asked of the scripts again when it may have changed.
    inventory: inventory::Inventory,
    /// The scripts said something about the inventory since it was last asked; the objects
    /// created and forgotten then; the containers the scripts have open for the player.
    inventory_stale: bool,
    inventory_seen: (u32, u32),
    open_containers: std::collections::BTreeSet<u64>,
    /// The items offered in a trade, as the scripts told the trade window (each side's, the
    /// partner's still in the partner's pack): the window shows them as the inventory's items.
    trade_items: std::collections::BTreeSet<u64>,
    /// The friends service's layer, when the player was started with one.
    friends: Option<friends::Friends>,
    /// Rooms (lent to the scripts while a call runs).
    rooms: Option<rooms::RoomChat>,
    /// The login was given its secret directly: a friends service that cannot be reached does not
    /// end it.
    login_secret_given: bool,
}

pub use motion::Body;

/// The connection as the client measures it every two seconds or more: the average round trip
/// (seconds), the share of packets lost, the bytes received and sent over the last span (seconds),
/// and the seconds since the server was last heard; the packets received over the last span, and
/// the bytes received a second over the last two measures; `count` numbers the measures.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Heartbeat {
    pub count: u64,
    pub latency: f64,
    pub loss: f64,
    pub bytes_in: u32,
    pub bytes_out: u32,
    pub span: f32,
    pub since_heard: f32,
    pub packets_in: u16,
    pub rate_in: f64,
}

/// How the session ended under the player, as far as it has: the login failed (`login_failed`),
/// or the login server let its connection go, with an error (`None` inside: one this client has no
/// name for); and whether the server is gone (the client has logged off).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SessionEnd {
    pub connect_failed: Option<Option<auberean_client_net::NetErrorCode>>,
    pub login_failed: bool,
    pub server_died: bool,
}

impl SessionEnd {
    /// Whether the session can go no further: the login failed, or the server is gone.
    #[must_use]
    pub const fn over(&self) -> bool {
        self.server_died || self.login_failed
    }
}

impl Client {
    #[must_use]
    pub fn new(settings: ClientSettings, data: ClientData) -> Self {
        let login_secret_given = !settings.ticket.is_empty();
        let logon = Logon {
            account: settings.account.clone(),
            ticket: settings.ticket,
            timestamp: settings.timestamp,
            session_key: settings.session_key,
        };
        // One data cache for the whole client: the scripts read records through the one the
        // download stores into.
        let download = download::DataDownload::new(settings.timestamp).over(&data.files);
        Self {
            session: Session::new(Settings {
                login_server: settings.login_server,
                logon,
            }),
            // The world: its objects' book and their model, in no region yet.
            // D2-10800
            queue: EventQueue::new(),
            model: Model::new(),
            world: world::ClientWorld::new(),
            scripts: data.scripts.map(|(portal, program)| {
                let mut s = Scripts::new(portal, program);
                s.host.set_data_cache(download.cache().clone());
                s
            }),
            body: Body::default(),
            calls: CallTally::default(),
            server_texts: CallTally::default(),
            stat_changes: CallTally::default(),
            life: lifecycle::Life::default(),
            lives: CallTally::default(),
            states_named: auberean_glue::data_ids::Named::default(),
            bodies_place: false,
            toolbar: toolbar::Toolbar::default(),
            toolbar_stale: false,
            toolbar_seen: (0, 0),
            effects: effects::EffectList::default(),
            effects_stale: false,
            effects_player: None,
            acts: acts::ActList::default(),
            acts_stale: false,
            acts_player: None,
            heartbeat: None,
            download,
            data: data.files,
            account: settings.account,
            script_sends: settings.script_sends,
            scene_wanted: false,
            world_frame_drawn: None,
            portal_space: false,
            ended: false,
            session_end: SessionEnd::default(),
            clock: clock::Clock::default(),
            game_clock: None,
            now: 0.0,
            sent: Vec::new(),
            observe: Vec::new(),
            observed: Vec::new(),
            asked: Vec::new(),
            waiting: Vec::new(),
            woken: CallTally::default(),
            shown: Vec::new(),
            notices: Vec::new(),
            physics: Vec::new(),
            last_attacker: 0,
            look_at_selection: true,
            looking_at: 0,
            selection_looks: Vec::new(),
            head: None,
            head_sent: None,
            head_sent_at: f64::NEG_INFINITY,
            shell: shell::History::default(),
            skills: skills::Skills::default(),
            skills_player: None,
            craft: craft::Craft::default(),
            craft_stale: false,
            store: store::Store::default(),
            store_stale: false,
            craft_player: None,
            recipe_open: None,
            recipe_stale: false,
            inventory: inventory::Inventory::default(),
            inventory_stale: false,
            inventory_seen: (0, 0),
            open_containers: std::collections::BTreeSet::new(),
            trade_items: std::collections::BTreeSet::new(),
            friends: None,
            rooms: Some(rooms::RoomChat::default()),
            login_secret_given,
        }
    }

    /// Draw the scripts' random numbers from `random`, the generator the rest of the program
    /// draws from.
    pub fn set_random(&mut self, random: &auberean_glue::numbers::Random) {
        if let Some(s) = &mut self.scripts {
            s.host.dice = random.clone();
        }
    }

    /// Everything the client sent so far.
    #[must_use]
    pub fn sent(&self) -> &[Sent] {
        &self.sent
    }

    /// Whether the server has the player in portal space: it took the player out of the world for
    /// a trip and has not put it back yet.
    #[must_use]
    pub fn in_portal_space(&self) -> bool {
        self.portal_space
    }

    /// The client's own clock as last given (the time of the frame), seconds.
    #[must_use]
    pub fn local_time(&self) -> f64 {
        self.now
    }

    /// The game clock now: the client's own clock until the server's time moves it forward.
    #[must_use]
    pub const fn game_time(&self) -> f64 {
        self.clock.now()
    }

    /// The server's clock from a time-sync header: the game clock moves forward to it, never back.
    // D2-17628
    pub fn sync_time(&mut self, server: f64) {
        self.clock.set(server, self.now);
    }

    fn send(&mut self, m: ClientMessage) {
        self.session.send(&m);
        self.sent.push(Sent {
            at: self.now,
            message: m,
        });
    }

    /// A datagram from the network.
    pub fn handle_datagram(&mut self, from: SocketAddr, bytes: &[u8], now: LocalTime) {
        self.session.handle_datagram(from, bytes, now);
    }

    /// The next datagram to send, and where.
    pub fn poll_transmit(&mut self) -> Option<(SocketAddr, Vec<u8>)> {
        self.session.poll_transmit()
    }

    /// What [`Self::disconnect`] would send, made now and sending nothing: each datagram and where
    /// it goes.
    #[must_use]
    pub fn goodbye(&self) -> Vec<(SocketAddr, Vec<u8>)> {
        self.session.goodbye()
    }

    /// Close every connection: a goodbye on each, as the client says when it closes; nothing more
    /// is sent. The data cache is reset.
    // D2-05261
    pub fn disconnect(&mut self) {
        self.session.log_off();
        self.download.reset_cache();
    }

    /// The entry's session ends, once an entry: the character logged off (none before the
    /// player's object is made).
    // D2-01936, D2-05339
    fn end_session(&mut self) {
        if self.ended {
            return;
        }
        self.ended = true;
        let character = self.model.player.unwrap_or(0);
        self.send(ClientMessage::LogOff(CharacterRef { character }));
    }

    /// The player asks for something.
    pub fn act(&mut self, action: Action) {
        match action {
            Action::PlayCharacter { character } => {
                // D2-05342
                self.ended = false;
                self.queue.start_entering();
                self.session.enter_world();
                let account = NarrowString(self.account.clone());
                self.send(ClientMessage::PlayCharacter(PlayCharacter {
                    character,
                    account,
                }));
            }
            Action::LogOff => self.end_session(),
            Action::CreateCharacter(c) => {
                // D2-05340
                let m = self.new_character(c);
                self.send(ClientMessage::CreateCharacter(m));
            }
            Action::DeleteCharacter { character } => {
                // D2-05341
                let account = NarrowString(self.account.clone());
                self.send(ClientMessage::DeleteCharacter(DeleteCharacter {
                    account,
                    character,
                }));
            }
            Action::RestoreCharacter { character, name } => {
                // D2-05343
                let account = NarrowString(self.account.clone());
                self.send(ClientMessage::RestoreCharacter(RestoreCharacter {
                    character,
                    name: WideString::from(name.as_str()),
                    account,
                }));
            }
            Action::Chat(bytes) => self.send(ClientMessage::Chat(
                auberean_protocol::login::ChatPacket(bytes),
            )),
            Action::ServerCall { function, args } => {
                self.send(ClientMessage::ServerCall(ServerCall {
                    function: FunctionId(function),
                    args: ScriptArgs(args),
                }))
            }
            Action::LookAt { target } => self.send(ClientMessage::LookAt(Target(target))),
            Action::TurnHead { x, y } => self.send(ClientMessage::TurnHead(HeadDirection { x, y })),
            Action::RequestData { kind, id } => {
                self.send(ClientMessage::DataRequest(DataRequest { kind, id }))
            }
            Action::Move(m) => self.body.ask(m),
            Action::Jump { velocity } => self.body.jump(velocity),
            Action::SetToolbarSlot { slot, shortcut } => {
                // D2-00533, D2-01255
                let ty = auberean_script::native_object::native_type::SHORTCUT_INFO;
                let packed = auberean_script::options::Shortcut {
                    kind: shortcut.kind,
                    item: shortcut.item,
                    data_enum: shortcut.data_enum,
                    data_id: shortcut.data_id,
                    text: shortcut.text,
                }
                .pack();
                let args = [
                    InterfaceArg::Native { ty, bytes: packed },
                    InterfaceArg::Word(slot),
                ];
                self.ask(function::SET_SHORTCUT, &args);
                self.toolbar_stale = true;
            }
            Action::SetOptions { changes, tell } => self.set_options(&changes, tell),
            // D2-00565, D2-01265
            Action::ShowRadarCoordinates { on } => {
                let on = InterfaceArg::Word(u32::from(on));
                self.ask(function::SET_SHOW_RADAR_COORDS, &[on]);
            }
            Action::KeepOptions => self.upload_game_options(),
            Action::Social(a) => self.social(a),
            Action::Emote { string, table } => self.emote(string, table),
            Action::AssistedTell { name } => self.assisted_tell(&name),
            Action::ToolbarSet { set } => {
                // D2-00535, D2-01268
                let set = [InterfaceArg::Word(set)];
                self.ask(toolbar::function::SET_WHICH_SHORTCUT_SET, &set);
                self.toolbar_stale = true;
            }
            Action::Select { target } => {
                // D2-01984, D2-00419, D2-01254
                self.ask(function::SET_SELECTION, &[InterfaceArg::Long(target)]);
                self.look_at_selection(target);
            }
            // D2-00423, D2-01285
            Action::UseSelected => self.ask(function::USE_SELECTION, &[]),
            // D2-00424, D2-01286
            Action::UseToolbarSlot { slot } => {
                self.ask(function::USE_SHORTCUT, &[InterfaceArg::Word(slot)]);
            }
            Action::PutItem {
                item,
                from,
                to,
                old_slot,
                new_slot,
                quantity,
            } => {
                // D2-00470, D2-01193
                let args = [
                    InterfaceArg::Word(quantity),
                    InterfaceArg::Word(new_slot),
                    InterfaceArg::Long(to),
                    InterfaceArg::Word(old_slot),
                    InterfaceArg::Long(from),
                    InterfaceArg::Long(item),
                ];
                self.ask(inventory::function::MOVE_ITEM, &args);
            }
            Action::Wear { item, location } => {
                // D2-00471, D2-01101
                let args = [InterfaceArg::Word(location), InterfaceArg::Long(item)];
                self.ask(inventory::function::EQUIP_ITEM, &args);
            }
            Action::TakeOff { item, slot } => {
                // D2-00472, D2-01280
                let args = [InterfaceArg::Word(slot), InterfaceArg::Long(item)];
                self.ask(inventory::function::UNEQUIP_ITEM, &args);
            }
            Action::IntoMoney {
                item,
                quantity,
                slot,
                from,
            } => {
                // D2-00480, D2-01100
                let args = [
                    InterfaceArg::Long(from),
                    InterfaceArg::Word(slot),
                    InterfaceArg::Word(quantity),
                    InterfaceArg::Long(item),
                ];
                self.ask(inventory::function::INTO_MONEY, &args);
            }
            Action::OutOfMoney {
                amount,
                slot,
                container,
            } => {
                // D2-00481, D2-01099
                let args = [
                    InterfaceArg::Long(container),
                    InterfaceArg::Word(slot),
                    InterfaceArg::Word(amount),
                ];
                self.ask(inventory::function::OUT_OF_MONEY, &args);
            }
            Action::TakeAll { container } => {
                // D2-00482, D2-01192
                self.ask(
                    inventory::function::LOOT_ALL,
                    &[InterfaceArg::Long(container)],
                );
            }
            Action::MeltAll { container } => {
                // D2-00483, D2-01279
                self.ask(
                    inventory::function::MELT_ALL,
                    &[InterfaceArg::Long(container)],
                );
            }
            Action::SetOpen { container, open } => {
                // D2-00531, D2-01281
                let player = self.model.player.unwrap_or(0);
                let args = [
                    inventory::Arg::Word(u32::from(open)),
                    inventory::Arg::Long(player),
                ];
                let asked = self.with_scripts(|s| {
                    s.call_on(container, inventory::function::UPDATE_OPEN, &[], &args)
                });
                if let Some(a) = asked.flatten() {
                    self.asked
                        .push((inventory::function::UPDATE_OPEN, a.result));
                }
            }
            Action::Attack { start } => {
                // D2-02492, D2-00426, D2-01203
                let start = InterfaceArg::Word(u32::from(start));
                self.ask(function::REQUEST_ATTACK_STATE_CHANGE, &[start]);
            }
            Action::Examination { tabs } => match tabs {
                // D2-02621, D2-00525, D2-01274
                Some(t) => self.ask(function::START_EXAMINATION, &[InterfaceArg::Word(t)]),
                // D2-00526, D2-01277
                None => self.ask(function::STOP_EXAMINATION, &[]),
            },
            // D2-02713, D2-00591, D2-01078
            Action::CancelQuest { quest } => {
                self.ask(function::CANCEL_QUEST, &[InterfaceArg::Word(quest)]);
            }
            Action::Resurrect => self.request_resurrect(),
            Action::DeclineResurrection => self.clear_resurrection_request(),
            Action::Dismount => self.dismount(),
            // D2-00452, D2-01270
            Action::ApplyTotem {
                target,
                source,
                effect,
            } => self.ask(
                function::SHOULD_APPLY_TOTEM_DONE,
                &[
                    InterfaceArg::Word(effect),
                    InterfaceArg::Long(source),
                    InterfaceArg::Long(target),
                ],
            ),
            Action::Skill(a) => self.skill(a),
            Action::Craft(a) => self.craft_action(a),
            Action::Trade(a) => {
                let (f, args) = store::trade_call(a);
                self.ask(f, &args);
            }
            Action::Store(a) => {
                let (f, args) = store::store_call(a);
                self.ask(f, &args);
            }
            Action::Pet(a) => {
                let (f, args) = store::pet_call(a);
                self.ask(f, &args);
            }
            // D2-00593, D2-01152
            Action::PlayScenesDone { scenes } => {
                let ty = store::SCENE_LIST;
                // None were handed to play (a door's, once the player has arrived): an empty list.
                let bytes = if scenes.is_empty() {
                    let mut w = auberean_script::wire::Out::new();
                    auberean_script::story::write_scenes(&mut w, &[]);
                    w.buf
                } else {
                    scenes
                };
                self.ask(
                    store::function::PLAY_SCENES_DONE,
                    &[InterfaceArg::Native { ty, bytes }],
                );
            }
            // D2-00463, D2-01086
            Action::SaleReminders => {
                self.ask(store::function::DISPLAY_SALE_REMINDERS, &[]);
            }
            Action::Examine {
                kind,
                data_enum,
                item,
                data_id,
                selection,
            } => {
                let request = examination::Request {
                    kind,
                    data_enum,
                    data_id,
                    object: item,
                    ..examination::Request::default()
                };
                self.examine(&request, selection);
            }
            // A line typed: carried out by the player's command line, as the chat panel's lines
            // are (the commands made known first when nothing has made them known).
            Action::Say { text } => {
                if self
                    .scripts
                    .as_ref()
                    .is_some_and(|s| s.host.commands.names.is_empty())
                {
                    self.know_commands();
                }
                self.process_text(&text);
            }
        }
    }

    /// Ask the player's scripts through the interface, and keep how it went.
    /// What the skill window asks of a skill, of the player's scripts.
    // D2-00504, D2-00505, D2-00506, D2-00507, D2-01225, D2-01226, D2-01219, D2-01227
    fn skill(&mut self, a: SkillAction) {
        use skills::function as f;
        match a {
            SkillAction::Train(s) => self.ask(f::TRAIN, &[InterfaceArg::Word(s)]),
            SkillAction::Untrain(s) => self.ask(f::UNTRAIN, &[InterfaceArg::Word(s)]),
            SkillAction::Raise(s) => self.ask(f::RAISE, &[InterfaceArg::Word(s)]),
            SkillAction::CancelUntrain => self.ask(f::CANCEL_UNTRAIN, &[]),
        }
    }

    /// What the crafting page and the ingredient panel ask of crafting, of the player's scripts.
    // D2-00437, D2-01195, D2-00441, D2-01218, D2-00436, D2-01202, D2-00443, D2-01207
    fn craft_action(&mut self, a: CraftAction) {
        use craft::function as f;
        match a {
            CraftAction::Open(r) => self.ask(f::OPEN_RECIPE, &[InterfaceArg::Word(r)]),
            CraftAction::RaiseCraftSkill(s) => {
                self.ask(f::RAISE_CRAFT_SKILL, &[InterfaceArg::Word(s)]);
            }
            CraftAction::AddIngredient {
                recipe,
                ordinal,
                item,
                spinner,
            } => self.ask(
                f::ADD_INGREDIENT,
                &[
                    InterfaceArg::Word(spinner.cast_unsigned()),
                    InterfaceArg::Long(item),
                    InterfaceArg::Word(ordinal),
                    InterfaceArg::Word(recipe),
                ],
            ),
            CraftAction::Inquire { recipe, spinner } => {
                if recipe == 0 {
                    self.recipe_open = None;
                    self.craft.opened = None;
                    self.craft.version += 1;
                } else {
                    self.recipe_open = Some((recipe, Some(spinner)));
                    self.recipe_stale = true;
                }
            }
            CraftAction::ExecuteCraft {
                recipe,
                spinner,
                ingredients,
            } => self.ask(
                f::EXECUTE_CRAFT,
                &[
                    InterfaceArg::Native {
                        ty: craft::INGREDIENT_TABLE,
                        bytes: craft::ingredient_table(&ingredients),
                    },
                    InterfaceArg::Word(spinner.cast_unsigned()),
                    InterfaceArg::Word(recipe),
                ],
            ),
        }
    }

    /// Something selected (or nothing) and the examination's request set, of the player's
    /// scripts.
    // D2-00523, D2-01250
    fn examine(&mut self, request: &examination::Request, selection: u64) {
        self.ask(function::SET_SELECTION, &[InterfaceArg::Long(selection)]);
        let bytes = request.pack();
        let ty = auberean_script::native_object::native_type::EXAMINATION_REQUEST;
        self.ask(
            skills::function::SET_EXAMINATION_REQUEST,
            &[InterfaceArg::Native { ty, bytes }],
        );
    }

    /// The options window's changes made through the player's interface, each in turn: a
    /// yes-or-no option by its own setter (melting without a question set to the opposite of
    /// asking), a radar filter in the radar's mask, and after each of those the radar's mask set
    /// (as read before the change, with the filter's bit); others' damage range; the shortcut
    /// bar's rows. Then, with `tell`, the panels told the options changed; the toolbar asked
    /// again; the options sent to the server to keep.
    // D2-02825, D2-02826, D2-02827, D2-00164
    fn set_options(&mut self, changes: &[(PlayerOption, u32)], tell: bool) {
        for &(option, value) in changes {
            match option {
                PlayerOption::OthersDamageRange | PlayerOption::ShortcutHeight => {
                    self.set_option(option, value);
                }
                flag => {
                    let mut mask = self.options().map_or(0, |o| o.radar_mask);
                    let on = value != 0;
                    if interface::flag_setter(flag).is_some() {
                        let v = if flag == PlayerOption::AskBeforeMelting {
                            !on
                        } else {
                            on
                        };
                        self.set_option(flag, u32::from(v));
                    }
                    if let Some(bit) = interface::radar_bit(flag) {
                        if on {
                            mask |= bit;
                        } else {
                            mask &= !bit;
                        }
                    }
                    self.ask(interface::RADAR_MASK.1, &[InterfaceArg::Word(mask)]);
                }
            }
        }
        if tell {
            let about = self.model.player;
            self.with_scripts(|s| {
                s.host.panel_notices.push(host::PanelNotice {
                    native: OPTIONS_CHANGED,
                    about,
                    words: Vec::new(),
                    objects: Vec::new(),
                    requests: Vec::new(),
                    effect_lines: Vec::new(),
                    listed: Vec::new(),
                });
            });
        }
        self.toolbar_stale = true;
        self.upload_game_options();
    }

    /// Send the player's options to the server to keep, through the player's interface.
    // D2-00594, D2-01283
    fn upload_game_options(&mut self) {
        self.ask(function::UPLOAD_GAME_OPTIONS, &[]);
    }

    /// Set one of the player's options through its own setter (a yes-or-no one 1 or 0, as kept:
    /// melting without asking for the question); the radar's filters have none.
    pub fn set_option(&mut self, option: PlayerOption, value: u32) {
        if let Some((_, set)) = interface::option_functions(option) {
            self.ask(set, &[InterfaceArg::Word(value)]);
        }
    }

    /// One of the player's options as its scripts answer it (as kept: melting without asking for
    /// the question); 0 with no player; `None` when no scripts run or for the radar's filters,
    /// which are bits of the radar's mask.
    pub fn option(&mut self, option: PlayerOption) -> Option<u32> {
        let (get, _) = interface::option_functions(option)?;
        self.interface(get, &[]).map(|(_, v)| v)
    }

    /// The radar's mask as the player's scripts answer it; 0 with no player.
    pub fn radar_mask(&mut self) -> Option<u32> {
        self.interface(interface::RADAR_MASK.0, &[]).map(|(_, v)| v)
    }

    /// Whether the player's scripts have something selected (1 or 0); 0 with no player.
    // D2-00527, D2-01155
    pub fn has_something_selected(&mut self) -> Option<u32> {
        self.interface(function::HAS_SOMETHING_SELECTED, &[])
            .map(|(_, v)| v)
    }

    /// Whether the player turns to what attacks it on its own (1 or 0), as its scripts answer.
    // D2-00422, D2-01271
    pub fn should_auto_target(&mut self) -> Option<u32> {
        self.interface(function::SHOULD_AUTO_TARGET, &[])
            .map(|(_, v)| v)
    }

    /// The player gets down from its mount.
    // D2-00492, D2-01085
    pub fn dismount(&mut self) {
        self.ask(function::DISMOUNT, &[]);
    }

    /// The player asks to be brought back to life.
    // D2-00529, D2-01221
    pub fn request_resurrect(&mut self) {
        self.ask(function::REQUEST_RESURRECT, &[]);
    }

    /// The player calls off an offer to be brought back to life.
    // D2-00530, D2-01080
    pub fn clear_resurrection_request(&mut self) {
        self.ask(function::CLEAR_RESURRECTION_REQUEST, &[]);
    }

    /// The player's body began a special attack (its number), as the body tells the scripts.
    // D2-00622, D2-01273
    pub fn special_attack_started(&mut self, attack: u32) {
        self.ask(
            function::SPECIAL_ATTACK_STARTED,
            &[InterfaceArg::Word(attack)],
        );
    }

    /// The player's body ended a special attack (its number).
    // D2-00623, D2-01272
    pub fn special_attack_ended(&mut self, attack: u32) {
        self.ask(
            function::SPECIAL_ATTACK_ENDED,
            &[InterfaceArg::Word(attack)],
        );
    }

    /// The player moves on its own: an attack the scripts started stops.
    // D2-00624, D2-01276
    pub fn stop_attacking_if_auto_moving(&mut self) {
        self.ask(function::STOP_ATTACKING_IF_AUTO_MOVING, &[]);
    }

    fn ask(&mut self, function: u32, args: &[InterfaceArg]) {
        if let Some((result, _)) = self.interface(function, args) {
            self.asked.push((function, result));
        }
    }

    /// The player looks at its new selection, or, with the option off, at nothing; the server is
    /// told what the player then looks at (nothing, for the player itself or what it holds), when
    /// that changes, or with the option on at every selection.
    // D2-08159, D2-06084, D2-04043
    fn look_at_selection(&mut self, target: u64) {
        let Some(player) = self.model.player_object().map(|o| o.id) else {
            return;
        };
        let target = if self.look_at_selection {
            target
        } else if self.looking_at == 0 {
            return;
        } else {
            0
        };
        self.looking_at = self.look_of(player, target);
        if self.selection_looks.len() >= LOOKS_KEPT {
            self.selection_looks.remove(0);
        }
        self.selection_looks.push(self.looking_at);
        self.send(ClientMessage::LookAt(Target(self.looking_at)));
    }

    /// What `object` looks at when asked to look at `target`: nothing for itself or for something
    /// it holds.
    // D2-06234
    fn look_of(&self, object: u64, target: u64) -> u64 {
        if target == object || self.model.children(object).contains(&target) {
            0
        } else {
            target
        }
    }

    /// Whether the server is told where the player's head points this frame: the player looks at
    /// nothing, more than [`HEAD_INTERVAL`] has passed since it was last told, and the head moved
    /// more than [`motion::EPSILON`] either way since.
    // D2-09637
    fn should_send_head(&self, now: f64) -> bool {
        let Some((pitch, turn)) = self.head else {
            return false;
        };
        if self.model.player_object().is_none() || self.looking_at != 0 {
            return false;
        }
        if now - self.head_sent_at <= HEAD_INTERVAL {
            return false;
        }
        self.head_sent.is_none_or(|(p, t)| {
            (pitch - p).abs() > motion::EPSILON || (turn - t).abs() > motion::EPSILON
        })
    }

    /// The server told where the player's head points, when the player is in the world.
    // D2-09638, D2-04044
    fn send_head(&mut self, now: f64) {
        let Some((x, y)) = self.head else {
            return;
        };
        if !self.model.player_object().is_some_and(|o| o.in_world) || !self.body.placed() {
            return;
        }
        self.head_sent = Some((x, y));
        self.head_sent_at = now;
        self.send(ClientMessage::TurnHead(HeadDirection { x, y }));
    }

    fn new_character(&self, c: NewCharacter) -> CreateCharacter {
        CreateCharacter {
            account: NarrowString(self.account.clone()),
            name: WideString(c.name),
            entity_did: c.entity,
            start_area: c.start_area,
            race: c.race,
            sex: c.sex,
            class: c.class,
            implement: c.implement,
            physique: Table {
                size_class: c.physique_size_class,
                entries: c.physique,
            },
        }
    }

    /// One frame at `now`.
    pub fn frame(&mut self, now: LocalTime) {
        let dt = (now.0 - self.now).max(0.0);
        self.now = now.0;
        self.clock.update(now.0);
        self.use_game_clock();
        self.download.set_time(now.0);
        // The friends service's news first; while its part of the login runs, the game login
        // waits.
        // D2-01660
        self.friends_frame();
        if self.friends_hold_login() {
            return;
        }
        self.session.tick(now);
        self.upkeep();
        while let Some(e) = self.session.poll_event() {
            self.event(e);
        }
        self.friends_world_name();
        if let Some(t) = self.session.take_time_sync() {
            self.sync_time(t);
        }
        self.refresh_toolbar();
        self.refresh_effects();
        self.refresh_acts();
        self.refresh_craft();
        self.refresh_skills();
        self.refresh_recipe();
        self.refresh_store();
        self.take_heartbeat();
        self.move_body(dt);
        self.report_scene();
        self.refresh_inventory();
    }

    /// The inventory as the panels show it: the player's pack in its segments and what it wears,
    /// the containers the scripts have open for the player, each item in them; [`Inventory`]'s
    /// version counts its changes.
    ///
    /// [`Inventory`]: inventory::Inventory
    #[must_use]
    pub fn inventory(&self) -> &inventory::Inventory {
        &self.inventory
    }

    /// The inventory asked of the scripts again, when they said something about it, or when
    /// objects came or went that it shows (an item it listed arrived or went, a container went, the
    /// player changed).
    pub fn refresh_inventory(&mut self) {
        let seen = (self.model.stats.created, self.model.stats.forgotten);
        let moved = seen != self.inventory_seen && self.inventory_moved();
        self.inventory_seen = seen;
        if !self.inventory_stale && !moved {
            return;
        }
        if self.model.player_object().is_none() {
            self.open_containers.clear();
            self.inventory_stale = false;
            return;
        }
        let open = self.open_containers.clone();
        let trade = self.trade_items.clone();
        let Some(mut inv) = self.with_scripts(|s| s.inventory(&open, &trade)) else {
            return;
        };
        self.inventory_stale = false;
        inv.version = self.inventory.version;
        if inv != self.inventory {
            inv.version += 1;
            self.inventory = inv;
        }
    }

    /// Whether objects the inventory shows came or went since it was asked.
    fn inventory_moved(&self) -> bool {
        let inv = &self.inventory;
        if inv.player != self.model.player.unwrap_or(0) {
            return true;
        }
        let listed = inv
            .containers
            .values()
            .flat_map(|c| c.segments.iter().flat_map(|s| s.items.iter()))
            .chain(inv.worn.iter().map(|(_, id)| id))
            .chain(self.trade_items.iter());
        let mut listed = listed;
        listed.any(|id| self.model.object(*id).is_some() != inv.items.contains_key(id))
            || inv
                .containers
                .keys()
                .any(|c| self.model.object(*c).is_none())
    }

    /// The scripts' notices about the inventory mark it to be asked again; a container opened or
    /// closed for the player is kept open or let go.
    fn inventory_noticed(
        stale: &mut bool,
        open: &mut std::collections::BTreeSet<u64>,
        notices: &[host::PanelNotice],
    ) {
        for n in notices {
            if !inventory::Inventory::noticed(n.native) {
                continue;
            }
            *stale = true;
            if n.native == inventory::CONTAINER_STATE_UPDATE {
                if let Some(c) = n.about {
                    if n.words.get(2).is_some_and(|w| w & 0xFF != 0) {
                        open.insert(c);
                    } else {
                        open.remove(&c);
                    }
                }
            }
        }
    }

    /// The scripts' notices to the trade window: an item offered (by either side) is shown with
    /// the inventory's items until it is taken back or the offers go (emptied, or the trade
    /// closed).
    fn trade_noticed(
        stale: &mut bool,
        items: &mut std::collections::BTreeSet<u64>,
        notices: &[host::PanelNotice],
    ) {
        let long = |n: &host::PanelNotice, at: usize| {
            let w = |i: usize| u64::from(n.words.get(i).copied().unwrap_or(0));
            w(at) << 32 | w(at + 1)
        };
        for n in notices {
            match n.native {
                store::trade::OFFERED => {
                    items.insert(long(n, 2));
                }
                store::trade::REVOKED => {
                    items.remove(&long(n, 2));
                }
                store::trade::RESET | store::trade::CLOSED => items.clear(),
                _ => continue,
            }
            *stale = true;
        }
    }

    /// The world's upkeep, before a frame's messages: objects waited for too long are asked for
    /// again, and objects the server had the client forget go. (A host that hands messages over
    /// itself, a recording, calls it once a frame.)
    pub fn upkeep(&mut self) {
        self.wake_due();
        // The data cache's worker and its askers.
        let now = self.now;
        let sends = Self::naming_states(
            &mut self.scripts,
            &mut self.model,
            &mut self.states_named,
            |states| self.download.use_time(now, states),
        );
        for m in sends {
            self.send(m);
        }
        // The world's upkeep: the objects', unless the scene waits for the cells around the
        // player.
        // D2-10802
        if !self.world.blocking_for_cells {
            self.maintain_objects();
        }
    }

    /// The objects' upkeep: the creations whose records have all finished make their objects,
    /// then the event queue's (the objects asked for again, the objects let go of).
    fn maintain_objects(&mut self) {
        self.download.finish_creations();
        let download = &self.download;
        self.queue
            .use_time_creating(self.now, |id| download.is_object_being_created(id));
        for id in self.queue.take_force_creates() {
            // D2-04042
            self.send(ClientMessage::ForceCreate(Target(id)));
        }
        for id in self.queue.take_destroyed() {
            // An object in the world leaves it as it goes.
            if self.life.leave(id) {
                self.tell_leave_world(id);
            }
            self.forget_waiting(id);
            self.model.forget(id);
            self.forget_look(id);
        }
    }

    /// An object goes: the calls of its scripts that wait are dropped, as its interface is
    /// destroyed.
    // D2-08046
    fn forget_waiting(&mut self, object: u64) {
        if let Some(p) = self.scripts.as_ref().and_then(|s| s.process(object)) {
            latent::purge(&mut self.waiting, p);
        }
    }

    /// Let go of the scripts' change to an object's own description.
    fn forget_look(&mut self, object: u64) {
        if let Some(s) = self.scripts.as_mut() {
            s.host.looks.remove(&object);
        }
    }

    /// Object `object`'s body entered the world (`in_world`) or left it, as the scene placing the
    /// bodies says: its scripts are told when that changes what they were told.
    // D2-06160, D2-06359
    pub fn set_in_world(&mut self, object: u64, in_world: bool) {
        if in_world {
            if self.model.object(object).is_some() && self.life.enter(object) {
                self.tell_life(object, lifecycle::function::HANDLE_ENTER_WORLD);
            }
        } else if self.life.leave(object) {
            self.tell_leave_world(object);
        }
    }

    /// An object's scripts told it left the world, then that it is out of it.
    // D2-08048, D2-08221, D2-08049
    fn tell_leave_world(&mut self, object: u64) {
        self.tell_life(object, lifecycle::function::HANDLE_LEAVE_WORLD);
        self.tell_life(object, lifecycle::function::HANDLE_POST_LEAVE_WORLD);
    }

    /// The waiting calls whose time came go on, earliest first (a sleeping one only while it
    /// still sleeps; see [`latent`]).
    // D2-08084, D2-08080, D2-00126
    fn wake_due(&mut self) {
        if self.waiting.is_empty() {
            return;
        }
        self.waiting.sort_by(|a, b| a.0.total_cmp(&b.0));
        let due = self.waiting.partition_point(|(t, _)| *t <= self.now);
        let woken: Vec<(f64, latent::Waiting)> = self.waiting.drain(..due).collect();
        for (t, w) in woken {
            let r = self
                .with_scripts(|s| {
                    if t == f64::NEG_INFINITY {
                        s.go_on(w)
                    } else {
                        s.wake(w)
                    }
                })
                .flatten();
            if let Some(r) = r {
                self.woken.add(&r);
            }
        }
    }

    /// A behavior, release, slider or mode an object's call waits in ended with `result` (the
    /// latent native's id, [`latent::waits`]): the call goes on at the next upkeep. Whether a call
    /// of the object waited in it.
    pub fn wait_done(&mut self, object: u64, context: u32, native: u32, result: u32) -> bool {
        let Some(w) = self
            .with_scripts(|s| s.complete(object, context, native, result))
            .flatten()
        else {
            return false;
        };
        self.waiting.push((f64::NEG_INFINITY, w));
        true
    }

    /// An object's scripts asked to show a string reference (its packed form) of a kind.
    // D2-08073, D2-04673
    pub fn display_string_info(
        &mut self,
        object: u64,
        kind: u32,
        info: Vec<u8>,
    ) -> Option<CallResult> {
        let ty = auberean_script::native_object::native_type::STRING_INFO;
        let args = [
            InterfaceArg::Native { ty, bytes: info },
            InterfaceArg::Word(kind),
        ];
        self.with_scripts(|s| s.call_object(object, latent::function::DISPLAY_STRING_INFO, &args))
            .flatten()
            .map(|(r, _)| r)
    }

    /// An object's scripts given an entity description (its packed form) to take properties
    /// from: how it went and what they answered.
    // D2-08054, D2-04672
    pub fn apply_properties(&mut self, object: u64, entity: Vec<u8>) -> Option<(CallResult, u32)> {
        let ty = latent::ENTITY_DESC;
        let args = [InterfaceArg::Native { ty, bytes: entity }];
        self.with_scripts(|s| s.call_object(object, latent::function::APPLY_PROPERTIES, &args))
            .flatten()
    }

    /// An object's scripts told of one event of its life, counted.
    fn tell_life(&mut self, object: u64, function: u32) {
        self.life.count(function);
        if let Some(Some(r)) = self.with_scripts(|s| s.tell_life(object, function)) {
            self.lives.add(&r);
        }
    }

    /// What a message did to an object's life, told to its scripts once it took effect: a new
    /// object made, then placed when it has a place; a placed object in the world; one taken out
    /// of it; one the server destroyed.
    fn live(&mut self, change: lifecycle::Change) {
        use lifecycle::Change;
        match change {
            // D2-08042, D2-08122, D2-05622
            Change::Made { object, .. } => {
                self.tell_life(object, lifecycle::function::HANDLE_CREATION);
                if !self.bodies_place
                    && lifecycle::placeable(&self.model, object)
                    && self.life.enter(object)
                {
                    self.tell_life(object, lifecycle::function::HANDLE_ENTER_WORLD);
                }
            }
            // D2-08047, D2-08220
            Change::Placed(object) => {
                if !self.bodies_place
                    && lifecycle::placeable(&self.model, object)
                    && self.life.enter(object)
                {
                    self.tell_life(object, lifecycle::function::HANDLE_ENTER_WORLD);
                }
            }
            Change::Taken(object) => {
                if !self.bodies_place && self.life.leave(object) {
                    self.tell_leave_world(object);
                }
            }
            // D2-08045, D2-08123, D2-05623
            Change::Destroyed(object) => {
                if self.model.object(object).is_some() {
                    self.tell_life(object, lifecycle::function::HANDLE_DESTRUCTION);
                }
                self.download.object_gone(object);
            }
        }
    }

    /// The character has left the world: its connections left, the events waiting for it
    /// dropped, the scripts started afresh, the body and the objects' lives gone.
    fn left_world(&mut self) {
        self.session.exit_world();
        self.queue.leave_world();
        if let Some(s) = self.scripts.as_mut() {
            s.reset();
        }
        self.body = Body::default();
        self.life.clear();
        self.download.objects_gone();
        self.world.leave_region();
        self.scene_wanted = false;
        self.portal_space = false;
        self.looking_at = 0;
    }

    /// The region the player's creation names becomes the world's: its number looked up in the
    /// region mapping for its record, which says its own number.
    // D2-10649
    fn set_region_id(&mut self, region: u32) {
        let cache = self.download.cache();
        let load = |n: u32| {
            if cache.is_empty() {
                return None;
            }
            let db = cache.database();
            let record = db.data_id_by_enum(n, world::REGION_MAPPING);
            db.get::<auberean_assets::region::Region>(record)
                .map(|r| (record, r.number))
        };
        let was = self.world.region();
        self.world.set_region(region, load);
        if region != 0 && self.world.region() != was {
            let had = self.game_clock.take();
            self.game_clock = Some(game_time::region_clock(cache, region, had));
            self.use_game_clock();
        }
    }

    /// The world's clock kept up with the game clock's.
    // D2-13067
    fn use_game_clock(&mut self) {
        let t = self.clock.now();
        if let Some(clock) = self.game_clock.as_mut() {
            clock.use_time(t);
        }
    }

    /// The world's time now: the game clock's, at the rate and from the place an admin may have
    /// moved it to. The sky and the calendar read this one.
    #[must_use]
    pub fn world_time(&self) -> f64 {
        let t = self.clock.now();
        self.game_clock.as_ref().map_or(t, |c| c.world_time(t))
    }

    /// The world's clock, once the world is in a region.
    #[must_use]
    pub const fn game_clock(&self) -> Option<&auberean_world::calendar::GameClock> {
        self.game_clock.as_ref()
    }

    /// The world's clock, to say the land has been lit for it.
    pub const fn game_clock_mut(&mut self) -> Option<&mut auberean_world::calendar::GameClock> {
        self.game_clock.as_mut()
    }

    /// The physics object of game object `id`, made by the scene (see
    /// [`world::create_physics_obj`]): from the physics record its qualities name, as its
    /// repository answers them (its own qualities without scripts). Whether it was made.
    pub fn create_physics_obj(&mut self, id: u64, scene: &mut dyn world::WorldScene) -> bool {
        let own = self.model.object(id).map_or(0, |o| {
            o.qualities
                .data_ids
                .get(&world::PHYSICS_RECORD)
                .copied()
                .unwrap_or(0)
        });
        let record = self
            .with_host(|h| repository::phys_obj_id(h, id))
            .unwrap_or(own);
        world::create_physics_obj(&self.model, id, record, scene)
    }

    /// Set the client's clock without a frame (a host that hands messages over itself).
    pub fn set_time(&mut self, now: LocalTime) {
        self.now = now.0;
        self.clock.update(now.0);
        self.download.set_time(now.0);
    }

    /// The data download as it stands: the connecting screen's events this session, oldest
    /// first, the notice of a record that did not store (which stops the client), and the files
    /// it stores into.
    #[must_use]
    pub fn data_download(&self) -> &download::DataDownload {
        &self.download
    }

    /// The data download and the data cache's requests, to ask for records through.
    pub fn data_download_mut(&mut self) -> &mut download::DataDownload {
        &mut self.download
    }

    fn event(&mut self, e: Event) {
        match e {
            Event::Message(r) => match r.incoming {
                Incoming::Message(m) => self.take_message(r.blob_id, *m),
                Incoming::Undecodable { bytes, .. } => self.take_unread(r.blob_id, &bytes),
            },
            Event::LoginFailed(error) => {
                self.session_end.login_failed = true;
                self.connect_failed(Some(error));
            }
            Event::ConnectionError {
                login: true, error, ..
            } => self.connect_failed(error),
            Event::ServerDied => self.server_died(),
            _ => {}
        }
    }

    /// The login failed, or the login server is letting its connection go, with an error: the
    /// connection's watchers are told why (the connecting screen, while it is up, says it). A
    /// world server's goodbye with an error is told to watchers that do nothing with it.
    fn connect_failed(&mut self, error: Option<auberean_client_net::NetErrorCode>) {
        if self.session_end.connect_failed.is_none() {
            self.session_end.connect_failed = Some(error);
        }
    }

    /// The server is gone: the client has logged off, and whoever listens for it is told (the
    /// game's flow, which puts up the disconnected screen).
    // D2-03995
    fn server_died(&mut self) {
        self.session_end.server_died = true;
    }

    /// How the session has ended under the player so far (nothing yet, while it goes on).
    #[must_use]
    pub const fn session_state(&self) -> SessionEnd {
        self.session_end
    }

    /// How the session ended under the player, once it can go no further: the login failed, or
    /// the server is gone. A host ends the session there.
    #[must_use]
    pub fn session_end(&self) -> Option<SessionEnd> {
        self.session_end.over().then_some(self.session_end)
    }

    /// A message from a server, as it came off its queue: through the event queue, and whatever
    /// may take effect does. (The session hands messages here; a host that reads messages from
    /// elsewhere, a recording, may too.)
    pub fn take_message(&mut self, blob_id: u64, message: ServerMessage) {
        // The chat service's packets go to its client, not through the event queue.
        if let ServerMessage::Chat(p) = &message {
            self.hear_rooms(&p.0);
            return;
        }
        if let ServerMessage::CharacterList(l) = &message {
            if let Some(r) = self.rooms.as_mut() {
                r.character_set(l.uses_chat_service);
            }
        }
        self.queue.receive(self.now, Item { blob_id, message });
        while let Some(item) = self.queue.pop() {
            self.take_effect(&item);
        }
    }

    /// A message that did not read came under `blob_id`: nothing of it takes effect, but what its
    /// stamp was holding back does.
    fn take_unread(&mut self, blob_id: u64, raw: &[u8]) {
        self.queue.receive_unread(self.now, blob_id, raw);
        while let Some(item) = self.queue.pop() {
            self.take_effect(&item);
        }
    }

    fn take_effect(&mut self, item: &Item) {
        // A quality the server sets is told to the object's scripts once it is set.
        let changed = body::stat_of(&item.message, self.model.player);
        if description::take_update(&mut self.model, &item.message) {
            self.model.stats.qualities += 1;
            self.stat_changed(changed);
            return;
        }
        if let ServerMessage::Position(p) | ServerMessage::PositionCell(p) = &item.message {
            if Some(p.sender.id) == self.model.player {
                self.server_moved_player(p);
            }
        }
        // A move the server orders the player's body keeps the order's stamp, and every report
        // after it carries that stamp: the server tells by it whether a report that the player
        // moved by hand interrupts the move it ordered.
        if let ServerMessage::MoveTo(e) = &item.message {
            if Some(e.sender.id) == self.model.player {
                self.body.move_to_stamp = e.action.stamp;
            }
        }
        // What the server says the player looks at (nothing for itself or what it holds).
        if let ServerMessage::LookAt(e) = &item.message {
            if Some(e.sender.id) == self.model.player {
                self.looking_at = self.look_of(e.sender.id, e.action.0);
            }
        }
        if let ServerMessage::Notice(n) = &item.message {
            self.server_text(n);
        }
        if PHYSICS_EVENTS.contains(&item.message.message_type()) {
            if self.physics.len() >= 2 * PHYSICS_KEPT {
                self.physics.drain(..PHYSICS_KEPT);
            }
            self.physics.push(item.message.clone());
        }
        if let ServerMessage::CreateObject(c) = &item.message {
            if Some(c.id) == self.model.player {
                self.body.instance_stamp = c.physics.instance_stamp;
                self.body.position_stamp = c.physics.stamps[0];
                if c.physics.bits & auberean_protocol::types::PhysicsDesc::POSITION != 0 {
                    let pos = c.physics.position;
                    self.body
                        .place(pos.cell, pos.frame.origin, heading_of(pos.frame.rotation));
                }
            }
        }
        let change = lifecycle::change_of(&item.message, &self.model);
        // A newer incarnation takes the old one's place: the old one leaves the world first.
        if let Some(lifecycle::Change::Made {
            object,
            replaced: true,
        }) = change
        {
            if self.life.leave(object) {
                self.tell_leave_world(object);
            }
        }
        // A description the server sends, or a new creation, replaces what the scripts changed of
        // the object's own and its body had not taken.
        // D2-08138
        if let ServerMessage::UpdateVisualDesc(e) = &item.message {
            self.forget_look(e.sender.id);
        }
        if let ServerMessage::CreateObject(c) = &item.message {
            self.forget_look(c.id);
        }
        // The incarnation the client has, sent again: its description is set over the one it has.
        let described_again = match &item.message {
            ServerMessage::CreateObject(c) => self
                .model
                .object(c.id)
                .is_some_and(|o| o.instance_stamp == c.physics.instance_stamp)
                .then_some(c.id),
            _ => None,
        };
        let effects = self.model.apply(item);
        if let ServerMessage::PlayerObject(p) = &item.message {
            self.set_region_id(p.region);
        }
        if let Some(id) = described_again {
            // D2-08218, D2-07359
            self.with_host(|h| body::described_again(h, id));
        }
        // A new object's creation asks the data cache for its records.
        if let (Some(lifecycle::Change::Made { .. }), ServerMessage::CreateObject(c)) =
            (&change, &item.message)
        {
            Self::naming_states(
                &mut self.scripts,
                &mut self.model,
                &mut self.states_named,
                |states| creation::start(&mut self.download, c, states),
            );
        }
        if let Some(c) = change {
            self.live(c);
        }
        for fx in effects {
            let mut out = Vec::new();
            let theirs = Self::naming_states(
                &mut self.scripts,
                &mut self.model,
                &mut self.states_named,
                |states| self.download.on_effect(&fx, &self.data, &mut out, states),
            );
            if theirs {
                out.into_iter().for_each(|m| self.send(m));
                continue;
            }
            match fx {
                // The download's, taken above.
                Effect::DataQuery
                | Effect::DownloadBegins(_)
                | Effect::DataRecord(_)
                | Effect::DataRefused(_)
                | Effect::DownloadWaiting(_)
                | Effect::DownloadEnds => {}
                Effect::PlayerCreated(_) => self.scene_wanted = true,
                Effect::Teleported => self.scene_wanted = true,
                // An entry refused, or a session ended (a boot), for a reason the screens know:
                // the play screen opened on the entry goes, logging off the character (0 when it
                // never had one) and leaving the world's connections, as the client does whichever
                // screen the reason leads to. A session that was in the world lets the world go
                // with it, as when the character leaves: nothing of it is shown again.
                // D2-01936, D2-05339
                Effect::Refused(reason) if (2..=21).contains(&reason) => {
                    self.end_session();
                    if self.model.player.is_some() {
                        self.model.leave_world();
                        self.left_world();
                    } else {
                        self.session.exit_world();
                        self.queue.leave_world();
                        self.scene_wanted = false;
                        self.portal_space = false;
                    }
                }
                Effect::Refused(_) => {}
                Effect::LeftWorld => self.left_world(),
                Effect::Call {
                    object,
                    function,
                    args,
                } => self.call(object, function, args.as_ref()),
                // Nothing in the client listens for a deletion: the list that follows shows it.
                Effect::PlayerObject | Effect::Characters | Effect::CharacterDeleted(_) => {}
            }
        }
        self.stat_changed(changed);
    }

    /// A line of text the server sends the player (its kind and a string reference): given to the
    /// player's interface, whose scripts show it as a line of that kind; with no player in the
    /// world, nothing shows.
    // D2-03960, D2-03977
    fn server_text(&mut self, n: &auberean_protocol::login::Notice) {
        use auberean_protocol::wire::Field as _;
        let Some(player) = self
            .model
            .player
            .filter(|p| self.model.object(*p).is_some())
        else {
            return;
        };
        let mut w = auberean_protocol::wire::Writer::default();
        n.text.write(&mut w);
        if let Some(r) = self.display_string_info(player, n.kind, w.finish()) {
            self.server_texts.add(&r);
        }
    }

    /// Tell an object's scripts one of its qualities changed (see [`body`]), counting how it went.
    fn stat_changed(&mut self, changed: Option<(u64, u32, u32)>) {
        let Some((object, kind, quality)) = changed else {
            return;
        };
        if let Some(Some(r)) = self.with_scripts(|s| s.stat_changed(object, kind, quality)) {
            self.stat_changes.add(&r);
        }
    }

    /// The server placed the player. A position with no newer stamp, or in no cell, is left. On a
    /// newer teleport the body goes there, facing the position's heading; on a newer forced position
    /// or with an impulse it goes there, facing as it faced, and an impulse then throws it. Any other
    /// position of the player's own moves nothing. A newer forced position tells the host, whose
    /// camera glides to the new place unless it is at the eyes. The position's time leaves the game
    /// clock alone: only the server's time-sync sets it.
    // D2-06372
    fn server_moved_player(&mut self, p: &auberean_protocol::physics::ObjectPosition) {
        use auberean_protocol::physics::position_flags;
        let first = !self.body.placed();
        if !first && (p.cell == 0 || !motion::is_newer(self.body.position_stamp, p.position_stamp))
        {
            return;
        }
        let teleported = motion::is_newer(self.body.teleport_stamp, p.teleport_stamp) || first;
        let forced = motion::is_newer(self.body.force_position_stamp, p.force_position_stamp);
        let impulse = p.flags & position_flags::IMPULSE != 0;
        self.body.position_stamp = p.position_stamp;
        self.body.teleport_stamp = p.teleport_stamp;
        if forced {
            self.body.force_position_stamp = p.force_position_stamp;
            self.body.force_moved();
        }
        if teleported {
            let heading = (p.heading_motion >> 24) as f32 * 360.0 / 255.0;
            self.body.place(p.cell, p.origin, heading);
        } else if forced || impulse {
            self.body.place(p.cell, p.origin, self.body.heading);
        }
        if impulse {
            self.body.throw(p.velocity);
        }
    }

    fn call(&mut self, object: Option<u64>, function: u32, args: Option<&ScriptArgs>) {
        if self.observe.contains(&function) {
            self.observed
                .push((self.now, function, args.cloned(), object));
        }
        match function {
            // The play screen's panels are made anew when a session starts, the effects panel
            // asking for the effects then.
            effects::SESSION_START => self.effects_stale = true,
            ENTER_PORTAL_SPACE => self.portal_space = true,
            EXIT_PORTAL_SPACE => {
                self.portal_space = false;
                self.scene_wanted = true;
            }
            _ => {}
        }
        let bytes = args.map_or(&[][..], |a| a.0.as_slice());
        if let Some(result) = self.with_scripts(|s| s.run(object, function, bytes)) {
            self.calls.add(&result);
        }
    }

    /// Run `f` with the client's scripts lent to the data cache to name the records of each saved
    /// state it loads (none without scripts), the model lent to the scripts meanwhile.
    fn naming_states<T>(
        scripts: &mut Option<Scripts>,
        model: &mut Model,
        named: &mut auberean_glue::data_ids::Named,
        f: impl FnOnce(Option<&dyn auberean_assets::requests::StateRecords>) -> T,
    ) -> T {
        let Some(s) = scripts.as_mut() else {
            return f(None);
        };
        std::mem::swap(&mut s.host.model, model);
        let namer = auberean_glue::data_ids::StateNamer::new(&mut s.vm, &mut s.host);
        let out = f(Some(&namer));
        named.add(&namer.named());
        drop(namer);
        std::mem::swap(&mut s.host.model, model);
        s.vm.heap.abandon_builds();
        s.vm.stopped_at = None;
        out
    }

    /// The scripts' host with the model lent to it, as a native sees it; nothing when no scripts
    /// run.
    pub fn with_host<T>(&mut self, f: impl FnOnce(&mut host::ClientHost) -> T) -> Option<T> {
        let scripts = self.scripts.as_mut()?;
        std::mem::swap(&mut scripts.host.model, &mut self.model);
        let out = f(&mut scripts.host);
        std::mem::swap(&mut scripts.host.model, &mut self.model);
        Some(out)
    }

    /// Run something on the scripts with the model lent to them; what they sent goes to the
    /// server (when the client sends its scripts' calls), and the player's freeze follows them.
    fn with_scripts<T>(&mut self, f: impl FnOnce(&mut Scripts) -> T) -> Option<T> {
        let scripts = self.scripts.as_mut()?;
        scripts.host.model = std::mem::take(&mut self.model);
        scripts.host.history = std::mem::take(&mut self.shell);
        scripts.host.friends = self.friends.take();
        scripts.host.rooms = self.rooms.take();
        scripts.host.clock = self.clock.now();
        scripts.host.game_clock = self.game_clock.take();
        scripts.host.my_position = self.body.placed().then(|| {
            let o = self.body.origin;
            // The heading turns the body about the up axis, clockwise from north.
            let (s, c) = math::sin_cosf(-self.body.heading.to_radians() / 2.0);
            (self.body.cell, [o.x, o.y, o.z], [c, 0.0, 0.0, s])
        });
        let out = f(scripts);
        attach_records(scripts);
        let said = scripts.host.panel_notices.clone();
        self.skills.follow(scripts, &said);
        let noticed = scripts.host.panel_notices.iter();
        self.toolbar_stale |= noticed
            .clone()
            .any(|n| toolbar::NOTICED.contains(&n.native));
        self.effects_stale |= noticed.clone().any(|n| n.native == effects::UPDATE_EFFECT);
        self.acts_stale |= noticed
            .clone()
            .any(|n| n.native == acts::UPDATE_STORY_QUESTS);
        self.craft_stale |= noticed.clone().any(|n| {
            matches!(
                n.native,
                craft::notice::UPDATE_CRAFT_PANEL | craft::notice::UPDATE_CRAFT_SKILL
            )
        });
        for n in noticed.clone() {
            if n.native == craft::notice::OPEN_INGREDIENT_PANEL {
                if let Some(r) = n.words.first() {
                    self.recipe_open = Some((*r, None));
                    self.recipe_stale = true;
                }
            } else if self.recipe_open.is_some() && craft::RECIPE_NOTICED.contains(&n.native) {
                self.recipe_stale = true;
            }
        }
        self.model = std::mem::take(&mut scripts.host.model);
        self.shell = std::mem::take(&mut scripts.host.history);
        self.friends = scripts.host.friends.take();
        self.rooms = scripts.host.rooms.take();
        self.game_clock = scripts.host.game_clock.take();
        for (w, seconds) in std::mem::take(&mut scripts.host.sleeps) {
            self.waiting.push((self.now + seconds, w));
        }
        let sent = self.notices.len();
        self.shown.extend(
            std::mem::take(&mut scripts.host.shown)
                .into_iter()
                .map(|mut s| {
                    s.notices_before += sent;
                    s
                }),
        );
        let noticed = std::mem::take(&mut scripts.host.panel_notices);
        self.store_stale |= self.store.noticed(&noticed);
        Self::inventory_noticed(
            &mut self.inventory_stale,
            &mut self.open_containers,
            &noticed,
        );
        Self::trade_noticed(&mut self.inventory_stale, &mut self.trade_items, &noticed);
        self.notices.extend(noticed);
        for (id, add) in std::mem::take(&mut scripts.host.destruction) {
            if add {
                self.queue.add_object_to_be_destroyed(id, self.now);
            } else {
                self.queue.remove_object_to_be_destroyed(id);
            }
        }
        let frozen = self
            .model
            .player
            .and_then(|p| scripts.host.frozen.get(&p))
            .is_some_and(|f| f.0);
        self.body.frozen = frozen;
        let sends: Vec<(u32, Vec<u8>, bool)> = std::mem::take(&mut scripts.host.sends);
        if self.script_sends {
            // Those marked for encryption go the same way: CD2-script-sends-in-the-clear.
            for (function, args, _) in sends {
                self.send(ClientMessage::ServerCall(ServerCall {
                    function: FunctionId(function),
                    args: ScriptArgs(args),
                }));
            }
        }
        // The scripts' room lines go to the chat service on the login connection.
        for p in self
            .rooms
            .as_mut()
            .map(rooms::RoomChat::take_packets)
            .unwrap_or_default()
        {
            self.send(ClientMessage::Chat(auberean_protocol::login::ChatPacket(p)));
        }
        Some(out)
    }

    /// What the skill window shows asked of the scripts when the player arrives, as the window
    /// asks for the race's root panels when the play screen's panels are made.
    pub fn refresh_skills(&mut self) {
        let player = self.model.player_object().and(self.model.player);
        if player.is_none() || player == self.skills_player || self.scripts.is_none() {
            return;
        }
        self.skills_player = player;
        self.ask_skills(None);
    }

    /// What the skill window shows asked again of the scripts, for a panel (or for the one the
    /// examination last asked for).
    pub fn ask_skills(&mut self, panel: Option<u32>) {
        let mut skills = std::mem::take(&mut self.skills);
        if let Some(id) = panel {
            skills.panel = Some(skills::SkillPanel {
                id,
                ..skills::SkillPanel::default()
            });
        }
        self.with_scripts(|s| skills.ask(s));
        self.skills = skills;
    }

    /// A string reference written to text as the client writes it, over the client's data files;
    /// `None` when no scripts run (they hold the files' string source).
    pub fn write_text(
        &mut self,
        info: &auberean_script::native_object::TextInfo,
    ) -> Option<String> {
        let scripts = self.scripts.as_mut()?;
        Some(text::from_script(info).text(&mut scripts.host.text))
    }

    /// A string reference written as the scripts write text; `None` when no scripts run.
    pub fn write_info(&mut self, info: &auberean_text::StringInfo) -> Option<String> {
        let scripts = self.scripts.as_mut()?;
        Some(info.clone().text(&mut scripts.host.text))
    }

    /// The player's quests as the journal asks for them, newest given first; `None` when no
    /// scripts run or there is no player.
    pub fn quests(&mut self) -> Option<Vec<auberean_script::native_object::QuestInfo>> {
        self.with_scripts(Scripts::quest_list)
            .flatten()
            .map(|(_, list)| list)
    }

    /// The player's options as the options panel reads them; `None` when no scripts run, there is
    /// no player, or it has none.
    pub fn options(&mut self) -> Option<auberean_script::options::GameplayOptions> {
        self.with_scripts(Scripts::options).flatten()
    }

    /// Write one of the player's files by a bare name; whether it was written.
    pub fn write_player_file(&mut self, file: &str, bytes: &[u8]) -> bool {
        use auberean_script::Host as _;
        self.scripts
            .as_mut()
            .is_some_and(|s| s.host.write_file(file, bytes))
    }

    /// One of the player's files by a bare name; none when there is no such file.
    pub fn read_player_file(&mut self, file: &str) -> Option<Vec<u8>> {
        use auberean_script::Host as _;
        self.scripts.as_mut()?.host.read_file(file)
    }

    /// The windows' saved places the player's scripts keep (a table's packed form); `None` when
    /// no scripts run, there is no player, or the scripts keep none.
    pub fn ui_save_locations(&mut self) -> Option<Vec<u8>> {
        self.with_scripts(Scripts::ui_save_locations).flatten()
    }

    /// Keep the windows' saved places (a table's packed form) with the player's options, through
    /// the player's interface.
    // D2-00563, D2-01267
    pub fn set_ui_save_locations(&mut self, table: Vec<u8>) {
        let ty = auberean_script::native_object::native_type::UI_SAVE_LOCATIONS;
        self.ask(
            function::SET_UI_SAVE_LOCATIONS,
            &[InterfaceArg::Native { ty, bytes: table }],
        );
    }

    /// Send the windows' saved places and opacities to the server to keep.
    // D2-00596, D2-01284
    pub fn upload_ui_settings(&mut self) {
        self.ask(function::UPLOAD_UI_SETTINGS, &[]);
    }

    /// The player's shortcuts, slot by slot, as the toolbar asks for them; `None` when no scripts
    /// run or there is no player.
    pub fn shortcuts(&mut self) -> Option<Vec<auberean_script::options::Shortcut>> {
        self.with_scripts(Scripts::shortcuts)
            .flatten()
            .map(|(_, all)| all)
    }

    /// The connection as last measured; `None` before the first measure.
    #[must_use]
    pub const fn heartbeat(&self) -> Option<Heartbeat> {
        self.heartbeat
    }

    /// A new measure of the connection, when the session took one.
    fn take_heartbeat(&mut self) {
        if !self.session.take_heartbeat() {
            return;
        }
        let l = self.session.link_status();
        let rtt = &l.round_trip_delays;
        let latency = if rtt.count() == 0 {
            0.0
        } else {
            rtt.total() / f64::from(rtt.count())
        };
        let s = l.snapshot;
        self.heartbeat = Some(Heartbeat {
            count: self.heartbeat.map_or(1, |h| h.count + 1),
            latency,
            loss: l.average_packet_loss(),
            bytes_in: s.bytes_received,
            bytes_out: s.bytes_sent,
            span: s.snapshot_duration,
            since_heard: s.time_since_last_got_data,
            packets_in: s.pkts_received,
            rate_in: self.session.average_bytes_per_second_received(),
        });
    }

    /// The toolbar as the shortcut bar shows it; [`toolbar::Toolbar::version`] counts its
    /// changes.
    #[must_use]
    pub fn toolbar(&self) -> &toolbar::Toolbar {
        &self.toolbar
    }

    /// The toolbar asked of the scripts again, when they said something the bar hears changed,
    /// the player changed a slot or the set, or (while a slot holds an item) objects came or went
    /// since it last was.
    pub fn refresh_toolbar(&mut self) {
        let seen = (self.model.stats.created, self.model.stats.forgotten);
        let churn = seen != self.toolbar_seen && !self.toolbar.items.is_empty();
        self.toolbar_seen = seen;
        if !self.toolbar_stale && !churn {
            return;
        }
        if self.model.player_object().is_none() {
            self.toolbar_stale = false;
            return;
        }
        let Some(asked) = self.with_scripts(Scripts::toolbar) else {
            return;
        };
        self.toolbar_stale = false;
        let Some(mut t) = asked else { return };
        t.version = self.toolbar.version;
        if t != self.toolbar {
            t.version += 1;
            self.toolbar = t;
        }
    }

    /// The effects on the player as the effects panel lists them; [`effects::EffectList::version`]
    /// counts their changes.
    #[must_use]
    pub fn effects(&self) -> &effects::EffectList {
        &self.effects
    }

    /// The effects asked of the scripts again, when they said an effect changed, a session started
    /// or the player arrived since they last were.
    pub fn refresh_effects(&mut self) {
        let player = self.model.player_object().and(self.model.player);
        if player.is_some() && player != self.effects_player {
            self.effects_stale = true;
        }
        if !self.effects_stale {
            return;
        }
        if player.is_none() {
            self.effects_stale = false;
            return;
        }
        let now = self.game_time();
        let Some(asked) = self.with_scripts(Scripts::effect_list) else {
            return;
        };
        self.effects_stale = false;
        self.effects_player = player;
        let Some((_, nodes)) = asked else { return };
        if nodes != self.effects.nodes || self.effects.version == 0 {
            self.effects = effects::EffectList {
                version: self.effects.version + 1,
                now,
                nodes,
            };
        }
    }

    /// What the crafting page shows; [`craft::Craft::version`] counts each asking.
    #[must_use]
    pub fn craft(&self) -> &craft::Craft {
        &self.craft
    }

    /// The crafting page's facts asked of the scripts again, when they said the craft panel or a
    /// craft skill changed or the player arrived since they last were.
    pub fn refresh_craft(&mut self) {
        let player = self.model.player_object().and(self.model.player);
        if player.is_some() && player != self.craft_player {
            self.craft_stale = true;
        }
        if !self.craft_stale {
            return;
        }
        if player.is_none() {
            self.craft_stale = false;
            return;
        }
        let mut craft = std::mem::take(&mut self.craft);
        let asked = self.with_scripts(|s| craft.ask(s));
        self.craft = craft;
        if asked.is_some() {
            self.craft_stale = false;
            self.craft_player = player;
        }
    }

    /// A recipe at a spinner value (its last one when none is given), as the ingredient panel
    /// reads it; `None` when no scripts run or they give no recipe.
    pub fn recipe_at(
        &mut self,
        did: u32,
        spinner: Option<i32>,
    ) -> Option<(craft::RecipeAt, Vec<(u32, CallResult)>)> {
        self.with_scripts(|s| s.recipe_at(did, spinner)).flatten()
    }

    /// The recipe the ingredient panel has open asked of the scripts again (at the spinner value
    /// it was last read at), when they said something about it, its craft skill or the items.
    pub fn refresh_recipe(&mut self) {
        if !self.recipe_stale {
            return;
        }
        let Some((did, spinner)) = self.recipe_open else {
            self.recipe_stale = false;
            return;
        };
        let Some(asked) = self.with_scripts(|s| s.recipe_at(did, spinner)) else {
            return;
        };
        self.recipe_stale = false;
        self.craft.version += 1;
        match asked {
            Some((at, incomplete)) => {
                self.recipe_open = Some((did, Some(at.spinner)));
                self.craft.opened = Some(at);
                self.craft.incomplete.extend(incomplete);
            }
            None => self.craft.opened = None,
        }
    }

    /// What the shop windows read; [`store::Store::version`] counts its askings.
    #[must_use]
    pub fn store(&self) -> &store::Store {
        &self.store
    }

    /// How many of each sale's item the player may buy and the consignment charge asked of the
    /// scripts again, when they showed a shop, changed its sales, or the player's money or pack
    /// changed while one shows.
    pub fn refresh_store(&mut self) {
        if !self.store_stale {
            return;
        }
        let mut store = std::mem::take(&mut self.store);
        let asked = self.with_scripts(|s| store.ask(s));
        self.store = store;
        if asked.is_some() {
            self.store_stale = false;
        }
    }

    /// The story's acts as the story page lists them; [`acts::ActList::version`] counts their
    /// changes.
    #[must_use]
    pub fn acts(&self) -> &acts::ActList {
        &self.acts
    }

    /// The acts asked of the scripts again, when they said the story changed or the player
    /// arrived since they last were.
    pub fn refresh_acts(&mut self) {
        let player = self.model.player_object().and(self.model.player);
        if player.is_some() && player != self.acts_player {
            self.acts_stale = true;
        }
        if !self.acts_stale {
            return;
        }
        if player.is_none() {
            self.acts_stale = false;
            return;
        }
        let Some(asked) = self.with_scripts(Scripts::act_list) else {
            return;
        };
        self.acts_stale = false;
        self.acts_player = player;
        let Some((result, acts)) = asked else { return };
        if acts != self.acts.acts || self.acts.version == 0 {
            self.acts = acts::ActList {
                version: self.acts.version + 1,
                acts,
                result: Some(result),
            };
        } else {
            self.acts.result = Some(result);
        }
    }

    /// Ask the player's scripts through the interface, as a panel does; `None` when no scripts
    /// run.
    pub fn interface(&mut self, function: u32, args: &[InterfaceArg]) -> Option<(CallResult, u32)> {
        self.with_scripts(|s| s.call_interface(function, args))
    }

    /// Whether object `object`'s scripts take emote `emote`, which a behavior the server asked of
    /// it carries (the interface's function for it on the object's own instance); `None` when no
    /// scripts run. An object whose scripts do not answer takes none.
    // D2-00607, D2-01153
    pub fn handle_weenie_emote(&mut self, object: u64, emote: u32) -> Option<bool> {
        let (how, taken) = self.with_scripts(|s| {
            s.call_interface_on(
                object,
                interface::function::HANDLE_WEENIE_EMOTE,
                &[InterfaceArg::Word(emote)],
            )
        })?;
        Some(how == CallResult::Completed && taken != 0)
    }

    /// Hand an object's collisions (a collision profile's packed form) to its scripts; `None` when
    /// no scripts run.
    pub fn handle_collision(&mut self, object: u64, profile: Vec<u8>) -> Option<CallResult> {
        self.with_scripts(|s| s.handle_collision(object, profile))
    }

    /// Every place the scripts stopped short since they were loaded (a native not written yet,
    /// the null reference used, another stop), named and counted; empty when no scripts run.
    #[must_use]
    pub fn script_stops(&self) -> Vec<ScriptStop> {
        use auberean_script::native::Stopped;
        let Some(s) = &self.scripts else {
            return Vec::new();
        };
        let lib = &s.host.program.library;
        s.vm.stops
            .iter()
            .map(|((what, at), count)| {
                let offset = u32::try_from(at * 4).unwrap_or(u32::MAX);
                ScriptStop {
                    native: match what {
                        Stopped::NotPorted(n) => Some(lib.native_name(*n)),
                        _ => None,
                    },
                    function: lib.function_at(offset).to_owned(),
                    offset,
                    what: what.clone(),
                    count: *count,
                }
            })
            .collect()
    }

    /// Ask the player's scripts through the interface for an instance id (the selection, the
    /// one before it); `None` when no scripts run.
    pub fn interface_long(&mut self, function: u32) -> Option<(CallResult, u64)> {
        self.with_scripts(|s| s.call_interface_long(function))
    }

    /// How an object shows on the radar, as its scripts answer: how the call went, whether it
    /// shows, its color and its shape, asked with a color and a shape to start from; `None` when
    /// no scripts run or the object is not here.
    pub fn radar_look(
        &mut self,
        object: u64,
        color: [f32; 4],
        shape: u32,
    ) -> Option<(CallResult, u32, [f32; 4], u32)> {
        self.with_scripts(|s| s.radar_look(object, color, shape))
            .flatten()
    }

    /// The effects and behaviors a hit of a skill shows at an attack hook with a result, by the
    /// attacker's scripts (see [`combat`]); `None` when no scripts run or the attacker is not here.
    pub fn hit_effects(
        &mut self,
        attacker: u64,
        skill: u32,
        attack_hook: u32,
        result: u32,
    ) -> Option<(CallResult, combat::HitEffects)> {
        self.with_scripts(|s| s.hit_effects(attacker, skill, attack_hook, result))
            .flatten()
    }

    /// The missile an attack fires at an attack hook, by the scripts of the body playing the story
    /// (see [`combat`]); `None` when no scripts run or the body is not here.
    pub fn missile_info(
        &mut self,
        body: u64,
        skill: u32,
        attack_hook: u32,
        attacker: u64,
        target: u64,
    ) -> Option<(CallResult, combat::MissileInfo)> {
        self.with_scripts(|s| s.missile_info(body, skill, attack_hook, attacker, target))
            .flatten()
    }

    /// A hit shown by the target's scripts: the combat lines and the floating text (see
    /// [`combat`]); `None` when no scripts run or the target is not here.
    pub fn show_damage(&mut self, damage: &combat::Damage) -> Option<CallResult> {
        self.with_scripts(|s| s.show_damage(damage)).flatten()
    }

    /// Whether a skill harms whom it is used on, by the attacker's scripts; `None` when no
    /// scripts run or the attacker is not here.
    pub fn is_skill_harmful(&mut self, attacker: u64, skill: u32) -> Option<(CallResult, bool)> {
        self.with_scripts(|s| s.is_skill_harmful(attacker, skill))
            .flatten()
    }

    /// The player was struck with a harmful skill by `attacker`, as a story told it: it is the
    /// last attacker; with the player's option to target attackers on and nothing selected, it is
    /// selected (and looked at, as any selection is).
    // D2-04052, D2-01976
    pub fn notice_last_attacker(&mut self, attacker: u64) {
        self.last_attacker = attacker;
        let auto = self
            .interface(function::GET_AUTO_TARGET, &[])
            .is_some_and(|(_, v)| v != 0);
        let selected = self.has_something_selected().is_some_and(|v| v != 0);
        if auto && !selected {
            self.act(Action::Select { target: attacker });
        }
    }

    /// An object's name as its scripts write it for a number of it; `None` when no scripts run,
    /// the object is not here or its scripts give no name.
    pub fn object_name(&mut self, object: u64, quantity: u32) -> Option<String> {
        self.with_scripts(|s| s.object_name(object, quantity))
            .flatten()
    }

    /// The player's own name as its repository answers it; `None` when no scripts run or there is
    /// no player.
    pub fn player_name(&mut self) -> Option<String> {
        let id = self.model.player?;
        self.with_host(|h| {
            let name = repository::name(h, id);
            crate::text::from_script(&name).text(&mut h.text)
        })
    }

    /// An object's name as its scripts give it for a number of it, as a string reference, for a
    /// text element to write once the string tables it reads are at hand; `None` as for
    /// [`Self::object_name`].
    pub fn object_name_info(
        &mut self,
        object: u64,
        quantity: u32,
    ) -> Option<auberean_text::StringInfo> {
        self.with_scripts(|s| s.object_name_info(object, quantity))
            .flatten()
    }

    /// A line typed for the player's command line (see [`shell`]): kept among the lines typed,
    /// expanded, then carried out (a command's function called on the player with the line's
    /// words and the rest of the line). Returns how it went; `Failure` for an empty line or with no
    /// scripts.
    pub fn process_text(&mut self, text: &str) -> shell::ShellStatus {
        if text.is_empty() {
            return shell::ShellStatus::Failure;
        }
        match self.with_scripts(|s| s.process_text(text)) {
            Some((status, asked)) => {
                self.asked.extend(asked);
                status
            }
            None => {
                self.shell.add(text);
                shell::ShellStatus::Failure
            }
        }
    }

    /// The player's scripts make the player's commands known to the command line.
    // D2-00528, D2-01201
    pub fn know_commands(&mut self) {
        self.ask(function::REGISTER_COMMANDS, &[]);
    }

    /// One line older among the lines typed.
    // D2-08070
    pub fn previous_command(&mut self) -> Option<String> {
        self.shell.older()
    }

    /// One line newer among the lines typed.
    // D2-08071
    pub fn next_command(&mut self) -> Option<String> {
        self.shell.newer()
    }

    /// A name from one of the player's lists the chat window steps through, as the scripts give
    /// it for one of the list functions: `None` when no scripts run, there is no player, or the
    /// scripts gave none.
    pub fn history_item(&mut self, function: u32) -> Option<String> {
        let (_, given, units) = self.with_scripts(|s| s.history_item(function)).flatten()?;
        (given != 0).then(|| String::from_utf16_lossy(&units))
    }

    /// The radar's ranges indoors and outdoors, meters, as the scripts give them.
    pub fn radar_ranges(&mut self) -> Option<(f32, f32)> {
        self.with_scripts(Scripts::radar_ranges).flatten()
    }

    /// An object's number, yes-or-no and instance-id qualities as the panels read them, as the
    /// scripts read them: its description's, else its own value, else its qualities record's
    /// (which needs the scripts' host: without it, the description and its own values only).
    pub fn int_quality(&mut self, id: u64, key: u32) -> Option<i32> {
        if self.scripts.is_some() {
            return self.with_scripts(|s| s.int_quality(id, key)).flatten();
        }
        let o = self.model.object(id)?;
        description::int(&o.weenie, key).or_else(|| o.qualities.ints.get(&key).copied())
    }

    pub fn bool_quality(&mut self, id: u64, key: u32) -> Option<bool> {
        if self.scripts.is_some() {
            return self.with_scripts(|s| s.bool_quality(id, key)).flatten();
        }
        let o = self.model.object(id)?;
        description::bool(&o.weenie, key)
            .or_else(|| o.qualities.bools.get(&key).copied())
            .map(|v| v != 0)
    }

    pub fn instance_quality(&mut self, id: u64, key: u32) -> Option<u64> {
        if self.scripts.is_some() {
            return self.with_scripts(|s| s.instance_quality(id, key)).flatten();
        }
        let o = self.model.object(id)?;
        description::instance_id(&o.weenie, key)
            .or_else(|| o.qualities.instance_ids.get(&key).copied())
    }

    fn move_body(&mut self, dt: f64) {
        if self.model.player_object().is_none() {
            return;
        }
        #[allow(clippy::cast_possible_truncation)]
        self.body.step(dt as f32);
        if self.body.should_report(self.now) {
            let time = self.game_time();
            let report = self.body.report(self.now, time);
            self.send(ClientMessage::Position(report));
        }
        let time = self.game_time();
        if self.should_send_head(time) {
            self.send_head(time);
        }
    }

    // D2-01904, D2-03986, D2-10650, D2-10651, D2-11020, D2-11021, D2-04055
    fn report_scene(&mut self) {
        if self.scene_wanted
            && !self.portal_space
            && self.world_frame_drawn != Some(false)
            && self.model.player_object().is_some()
        {
            self.scene_wanted = false;
            self.send(ClientMessage::SceneRenderingComplete(Empty));
        }
    }

    /// Whether the player's body is being asked to move: a front end runs frames faster then.
    #[must_use]
    pub fn moving(&self) -> bool {
        let a = self.body.asked;
        a.active || a.forward != 0.0 || a.sideways != 0.0
    }
}

/// The notice that the player's options were read again.
const RELOAD_OPTIONS: u32 = 0x0085 << 16 | 0x0009;
/// The client's own notice to the panels that the options window changed the player's options.
pub const OPTIONS_CHANGED: u32 = 0x46D;
/// The journal's notice, and its kind that says the whole list is to be read.
const UPDATE_QUESTS: u32 = 0x008E << 16 | 0x0001;
const ALL_QUESTS: u32 = 0x4000_0002;

/// What the panels read when a notice comes, given with the notice: the options read again carry
/// the player's options (as the examination window asks for them), the journal's whole list the
/// player's quests (as the journal asks for them).
// D2-02625, D2-02727
fn attach_records(scripts: &mut Scripts) {
    for i in 0..scripts.host.panel_notices.len() {
        let n = &scripts.host.panel_notices[i];
        if n.native == examination::UPDATE_EXAMINATION_PROFILE {
            expand_effect_lines(scripts, i);
            continue;
        }
        let record = if (n.native == RELOAD_OPTIONS || n.native == OPTIONS_CHANGED)
            && n.objects.is_empty()
        {
            scripts
                .options()
                .map(|o| auberean_script::native_object::NativeObject::Options(Box::new(o)))
        } else if n.native == UPDATE_QUESTS && n.words.first() == Some(&ALL_QUESTS) {
            scripts
                .quest_list()
                .map(|(_, l)| auberean_script::native_object::NativeObject::QuestList(l))
        } else {
            continue;
        };
        if let Some(n) = scripts.host.panel_notices.get_mut(i) {
            n.objects.push(record);
        }
    }
}

/// A profile's effect lines' own lines, as the examination window asks the scripts for them when
/// it lays a profile out, kept with the notice that carries the profile (the lines of an effect
/// within an effect are not asked: the window leaves them out).
// D2-02603
fn expand_effect_lines(scripts: &mut Scripts, i: usize) {
    let profiles: Vec<(usize, examination::Profile)> = scripts.host.panel_notices[i]
        .objects
        .iter()
        .enumerate()
        .filter_map(|(j, o)| {
            o.as_ref()
                .and_then(examination::Profile::of)
                .map(|p| (j, p))
        })
        .collect();
    for (j, p) in profiles {
        let mut lines = Vec::new();
        for (k, line) in p.nodes.iter().enumerate() {
            if line.kind != examination::kind::EFFECT {
                continue;
            }
            if let Some(sub) = scripts.effect_lines(line.appearance, line.tabs, line.int()) {
                lines.push((k, sub.pack()));
            }
        }
        if let Some(slot) = scripts
            .host
            .panel_notices
            .get_mut(i)
            .and_then(|n| n.effect_lines.get_mut(j))
        {
            *slot = lines;
        }
    }
}

/// The heading a rotation faces, degrees clockwise from north.
#[must_use]
pub fn heading_of(q: auberean_primitives::Quat) -> f32 {
    // A rotation about the up axis by `a` turns north toward west for positive `a`; the heading
    // is its opposite.
    let a = 2.0 * math::atan2f(q.z, q.w);
    (-a.to_degrees()).rem_euclid(360.0)
}

/// A direction on the ground from a heading.
#[must_use]
pub fn facing(heading: f32) -> Vec3 {
    let (s, c) = math::sin_cosf(heading.to_radians());
    Vec3 { x: s, y: c, z: 0.0 }
}
