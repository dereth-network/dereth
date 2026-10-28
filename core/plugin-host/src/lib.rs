//! The plugin host: loads plugin packages into VMs, holds each to the world's policy, turns each
//! frame into events, runs timers, keeps each plugin's store and declared windows, and answers the server's
//! plugin audit.
//!
//! **Depends on** the interface (`dereth-plugin-api`), and on the game as the headless client
//! reaches it: the runtime's frame hook (`dereth-client-runtime`), the object model
//! (`dereth-client-model`), the view (`dereth-client-contract`), the identifiers
//! (`dereth-primitives`), the game rules
//! objects are classified by (`dereth-rules`), the appraisal's wire types and the message codecs (`dereth-protocol`) and
//! the game-data decoders (`dereth-assets`, for the landblocks and cells plugins read), the portal
//! store's types (`dereth-navigation`, for journeys and what the store knows); on the packages
//! crate (`dereth-plugin-package`) for packages and their manifests; `toml` reads the
//! world's policy document, `regex` matches loot profiles' patterns and `web-time` times each
//! plugin's share of a frame. **Used by** the products that
//! load plugins, through the client SDK, and by the client shell, which reads the declared windows
//! for the plugin window renderer and hands back clicks.
//!
//! **Must never** name a crate under `dereth/client/` (`cargo xtask seams`, `seam: plugin-host
//! names no client crate`), do I/O, or know which script engine it runs: packages arrive through
//! a [`PluginSource`], stores leave through a [`PluginStorage`], and the engine is a
//! [`ScriptEngine`].
//!
//! # A frame
//!
//! The host is installed into the runtime as its frame hook ([`PluginHook`]). Each frame, in
//! order:
//!
//! 1. loads, unloads and reloads asked for since the last frame are carried out;
//! 2. the frame's state becomes events ([`observe`]), then the frame's chat lines, the commands
//!    the player typed, the clicks on plugin windows and the timers that came due;
//! 3. each event is delivered to every running plugin that listens for it, in load order;
//! 4. what the plugins asked for goes back to the frame: lines to print and to hide, the command
//!    words claimed, and the next queued action's requests ([`actions`]).
//!
//! # Gestures and automation
//!
//! A click on a plugin's window or a command it registered is a player gesture: for
//! [`actions::ACTIVATION_SECS`] of the frame clock after it, the plugin's actions are interactive.
//! An action with no gesture behind it is automated. Every plugin may act either way; the world's
//! policy says which domains it allows with a gesture, without one, or not at all
//! ([`dereth_plugin_api::WorldPolicy`]), and an action it does not allow fails as
//! `denied_by_world`. A world that publishes no policy allows everything.
//!
//! # Which plugins load
//!
//! The host has two kinds of source: the packages the player named to load (the explicit source,
//! [`PluginHost::new`]), which load at once; and the plugins it finds, each listed in the plugin
//! panel (`/plugin`) and loaded once the player enables it (ticks it), which the host remembers. A
//! found plugin from a plugins folder or among the ones that come with the client
//! ([`PluginHost::with_found`]) waits for its tick; one a launcher offers
//! ([`PluginHost::with_offered`]: its installs and the player's own folders) is ticked the first
//! time the host sees it. After that the player's tick decides, and a plugin they disable stays
//! off. A plugin's windows and HUDs open only when the player asks: from the plugin bar, or by the
//! plugin while answering the player; logging out closes them all.
//!
//! The chat says, one line each, when a plugin is loaded (at the start, or for any reason but the
//! player's Enable or Reload), reloaded, unloaded (for any reason but Disable), enabled and
//! disabled, by the name its manifest gives it.
//!
//! # Faults
//!
//! A failing handler's error is told in the chat, one line, and written with its traceback to the
//! client's log. Ten failures within a minute, or three entries stopped by the time budget within
//! a minute, suspend the plugin; the player is told, and `/plugin reload <id>` starts it again.
//!
//! # A frame's time
//!
//! Besides each entry's budget, a plugin's entries in one frame together have a share of the
//! frame ([`PLUGIN_FRAME_BUDGETS`] entries' budgets), and all plugins' together a larger one
//! ([`FRAME_BUDGETS`]). Past either, the events still due for a plugin wait, in order, for the
//! next frame, and are delivered before its new ones. A plugin past its own share in thirty frames
//! within a minute is suspended. Loading a plugin is not held to its share (it has a budget of its
//! own), though the time is counted. The time is the machine's monotonic clock's, or that of the
//! clock the host is given ([`PluginHost::with_vm_clock`]).

#![forbid(unsafe_code)]

pub mod actions;
pub mod calls;
pub mod character;
pub mod clock;
pub mod gamedata;
pub mod import;
pub mod loot;
pub mod messages;
pub mod metfile;
pub mod nav;
pub mod navfile;
pub mod net;
pub mod observe;
pub mod overlay;
pub mod package;
pub mod pattern;
pub mod peers;
pub mod pictures;
pub mod places;
pub mod policy_doc;
pub mod reads;
pub mod services;
pub mod session;
pub mod shown;
pub mod slot;
pub use dereth_plugin_package::source;
pub mod storage;
pub mod usdfile;

use std::collections::{BTreeSet, VecDeque};

use dereth_client_contract::chat::interface::ChatMessage;
use dereth_client_contract::panels::map::MapClick;
use dereth_client_runtime::frame_hook::{FrameHook, HookFrame, PluginQueryAnswer};
use dereth_plugin_api::audit::{self, AuditEntry};
use dereth_plugin_api::{
    ApiVersion, AutoDomain, Context, Event, HostCapability, Limits, ScriptEngine, ScriptError,
    ScriptErrorKind, ScriptInstance, Value, WorldPolicy,
};

use crate::calls::{Call, Shared};
pub use crate::clock::{MachineClock, VmClock};
pub use crate::net::{HttpRequest, HttpResponse, NoNet, PluginNet, TableNet};
pub use crate::overlay::{OverlayView, Overlays};
pub use crate::package::{Package, PackageError, PluginSource, StaticSource};
pub use crate::peers::{BusEndpoint, LocalBus, NoPeers, PeerMessage, PluginPeers};
use crate::slot::{Scope, SlotState, Status, Window};
pub use crate::storage::{MemoryFiles, MemoryStorage, NoFiles, PluginFiles, PluginStorage};

/// The host's own command verb: `/plugin`.
pub const HOST_VERB: &str = "plugin";

/// The owner id of the host's own window, the plugin panel, in [`PluginHost::windows`], and of its
/// own stores and data folder (the import folder). It is in the namespace no plugin may take
/// ([`dereth_plugin_api::PluginId::is_reserved`]), so no plugin reaches what is kept under it.
pub const HOST_ID: &str = "dereth.plugins";

/// The host's store scope for the player's tick on each found plugin: plugin id to `true` where
/// it is ticked, `false` where it was unticked. A plugin with neither was never ticked or unticked.
pub const TICKED_SCOPE: &str = "ticked";

/// A plugin the host found, as it lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    /// Whether the player ticked it.
    pub on: bool,
}

/// The plugin panel's window id.
pub const PANEL: &str = "panel";

/// What the panel and `/plugin list` say while there is no plugin.
pub const NO_PLUGINS: &str = "No plugins yet.";

/// The window id of the toasts, under the host's own id.
pub const TOASTS: &str = "toasts";

/// How long, in seconds, the window over which failures are counted is.
const FAULT_WINDOW: f64 = 60.0;
/// Failures within the window that suspend a plugin.
const MAX_ERRORS: usize = 10;
/// Budget overruns within the window that suspend a plugin.
const MAX_OVERRUNS: usize = 3;
/// A plugin's share of one frame, in entries' budgets: all its entries in the frame together.
pub const PLUGIN_FRAME_BUDGETS: f64 = 2.0;
/// All plugins' share of one frame together, in entries' budgets.
pub const FRAME_BUDGETS: f64 = 3.0;
/// Frames past its share within the window that suspend a plugin: a plugin that is heavy now and
/// then only has its events wait; one that takes more than its share frame after frame is stopped.
const MAX_FRAME_OVERRUNS: usize = 30;
/// The most events held for a plugin for later frames.
const MAX_BACKLOG: usize = 1000;
/// How often stores are written out, in seconds.
const FLUSH_INTERVAL: f64 = 60.0;
/// The prefix of the host's own lines in the chat window.
const PREFIX: &str = "[Plugins] ";

/// What a host is.
#[derive(Debug, Clone)]
pub struct HostConfig {
    /// `host.name`: `dereth-desktop`, `dereth-headless`, …
    pub name: String,
    /// `host.version`.
    pub version: String,
    pub capabilities: Vec<HostCapability>,
    pub limits: Limits,
    /// The world's policy for automation.
    pub policy: WorldPolicy,
    /// The most bytes one plugin's store may hold in one scope.
    pub store_quota: usize,
    /// Ask the source once a second which packages changed, and reload those plugins: the
    /// author's edit-and-see loop.
    pub watch: bool,
    /// The actions this host has no way to carry out (a host inside another client, which reaches
    /// the game only through that client's own add-on calls): each fails at once as
    /// `unavailable`.
    pub unavailable: Vec<&'static str>,
    /// The actions this host cannot carry out just now, each with why: each fails at once as
    /// `unavailable (why)`. A host whose means come and go changes them as they do
    /// ([`PluginHost::set_unavailable`]).
    pub unavailable_now: Vec<(&'static str, String)>,
    /// The host's own switches on the plugin panel, below the plugin list: each an id and its
    /// label. Each is off until the host or the player turns it on ([`PluginHost::switch`]).
    pub panel_switches: Vec<(&'static str, String)>,
}

impl HostConfig {
    /// A host named `name` offering `capabilities`, with the default limits.
    #[must_use]
    pub fn new(name: &str, capabilities: Vec<HostCapability>) -> Self {
        Self {
            name: name.to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            capabilities,
            limits: Limits::default(),
            policy: WorldPolicy::unpublished(),
            store_quota: calls::SCOPE_QUOTA,
            watch: false,
            unavailable: Vec::new(),
            unavailable_now: Vec::new(),
            panel_switches: Vec::new(),
        }
    }
}

/// A click or a change on a plugin's window, from whatever renders it.
#[derive(Debug, Clone, PartialEq)]
pub enum UiInput {
    Click {
        node: String,
    },
    /// A toggle's new value, or another input's.
    Change {
        node: String,
        value: Value,
    },
    /// A list's selected rows, from 1.
    Select {
        node: String,
        rows: Vec<i64>,
    },
    Close,
    /// A resizable window given a new size, in character cells and lines.
    Resize {
        width: i64,
        height: i64,
    },
}

/// One hotkey a plugin declared (a `hotkey` node in one of its windows): the command it runs and
/// the key it asks for; the front end binds the key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hotkey {
    pub plugin: String,
    pub window: String,
    /// The node's id, which a press names.
    pub node: String,
    /// The command line it runs, as typed after the `/`.
    pub command: String,
    pub text: String,
    /// The key it asks for (`Ctrl+Shift+F5`), when it names one.
    pub key: Option<String>,
}

/// One command a plugin claimed (`commands.add`): the plugin, the verb typed after the `/`, and
/// what the plugin says it does. An interface may put it on a control of its own (a hotbar slot),
/// which runs it as the player typing it would ([`PluginHost::run_command`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    pub plugin: String,
    pub verb: String,
    pub help: String,
}

/// One plugin on the plugin bar: what it is called, what it is doing (`running`, `paused`, `idle`
/// as it says, or `error` when it has stopped on a fault), and its main window (`main`, else its
/// first) and whether that is open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BarEntry {
    pub id: String,
    pub name: String,
    pub state: String,
    pub window: Option<String>,
    pub open: bool,
}

/// The store scope, under the host's own id, of the interface's remembered places and sizes, by
/// character.
pub const LAYOUT_SCOPE: &str = "layout";

/// One loaded plugin as a list shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub status: String,
    /// The reason, for a faulted or suspended plugin.
    pub reason: Option<String>,
    /// The permissions its manifest names: what it says it uses.
    pub permissions: Vec<String>,
    /// Whether the session is in one of its contexts.
    pub active: bool,
    /// Its contexts, by name.
    pub contexts: Vec<String>,
}

/// Why a plugin starts or stops, for the line the chat is told of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Why {
    /// The client's start, or any other reason: `loaded.`, `unloaded.`.
    Other,
    /// The player's Enable: `enabled.`.
    Enable,
    /// The player's Disable: `disabled.`.
    Disable,
    /// A reload (the player's, or an edited plugin's files): `reloaded.`.
    Reload,
}

#[derive(Debug)]
enum Pending {
    Load(Package, Why),
    Unload(String, Why),
    Reload(String, Why),
}

impl Pending {
    /// The plugin it is for.
    fn id(&self) -> Option<String> {
        match self {
            Self::Load(p, _) => p.manifest().ok().map(|m| m.id.to_string()),
            Self::Unload(id, _) | Self::Reload(id, _) => Some(id.clone()),
        }
    }
}

/// One row of the plugin panel: a plugin as the list shows it, whether it runs, and the player's
/// tick on it (none for a plugin named to load, which is not ticked).
#[derive(Debug, Clone)]
struct PanelRow {
    info: PluginInfo,
    running: bool,
    ticked: Option<bool>,
}

/// A plugin's name as the host's lines tell it: its manifest's, on one line and cut to its length,
/// or its id where it has none.
fn told_name(m: &dereth_plugin_api::Manifest) -> String {
    let name = shown::bounded(&m.name, shown::NAME);
    if name.is_empty() {
        m.id.to_string()
    } else {
        name
    }
}

/// A line of the host's own in the chat window.
pub(crate) fn host_line(text: &str) -> ChatMessage {
    let text = dereth_plugin_api::engine::console_line(text);
    ChatMessage {
        feedback: dereth_client_contract::feedback::Feedback::LOCAL,
        ty: observe::chat_type("system").unwrap_or(5),
        body: format!("{PREFIX}{text}"),
        prefix: None,
        window: 0,
    }
}

/// The chat's line for an error plugin `m` met (`text`, its traceback after its first line): the
/// first line alone, under the plugin's name, and for an entry stopped by its time budget
/// (`budget`) without where it was running. The whole text goes to the client's log.
pub(crate) fn error_line(m: &dereth_plugin_api::Manifest, text: &str, budget: bool) -> ChatMessage {
    tracing::warn!(target: "dereth::plugin", "{}: {text}", m.id);
    let mut first = text.lines().next().unwrap_or_default();
    if budget {
        first = first.split(" (at ").next().unwrap_or_default();
    }
    host_line(&format!(
        "{}: {}",
        told_name(m),
        shown::bounded(first, shown::LINE)
    ))
}

/// The chat's line for a line plugin `m` printed: on one line, cut to its length, under the
/// plugin's name.
pub(crate) fn printed_line(m: &dereth_plugin_api::Manifest, line: &str) -> ChatMessage {
    ChatMessage {
        feedback: dereth_client_contract::feedback::Feedback::LOCAL,
        ty: 0,
        body: format!("[{}] {}", told_name(m), shown::one_line(line, shown::LINE)),
        prefix: None,
        window: 0,
    }
}

/// One source of found plugins.
#[derive(Debug)]
struct FoundSource {
    source: Box<dyn PluginSource>,
    /// A launcher offers them: each is ticked the first time the host sees it.
    offered: bool,
}

struct Slot {
    state: SlotState,
    vm: Option<Box<dyn ScriptInstance>>,
    prints: usize,
}

impl std::fmt::Debug for Slot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Slot")
            .field("id", &self.state.manifest.id)
            .field("status", &self.state.status)
            .finish_non_exhaustive()
    }
}

/// Where an event goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Target {
    /// Every running plugin that listens for it.
    All,
    /// One plugin, by id index at the time the event was made.
    One(usize),
}

/// The plugin host.
#[derive(Debug)]
pub struct PluginHost {
    config: HostConfig,
    /// The engines, the first that handles a package's entry running it.
    engines: Vec<Box<dyn ScriptEngine>>,
    source: Box<dyn PluginSource>,
    /// The ids the explicit source offered when it was last read: the plugins named to load.
    explicit: BTreeSet<String>,
    /// The sources of the plugins the host finds, in the order they were given: listed, and
    /// loaded only once ticked ([`Self::with_found`], [`Self::with_offered`]).
    found: Vec<FoundSource>,
    /// What [`Self::found`] offered when it was last read: id, name, version.
    found_known: Vec<(String, String, String)>,
    /// The player's tick on each found plugin, as the host's store keeps it: on, or off where
    /// they unticked it; a plugin with none is off.
    ticks: std::collections::BTreeMap<String, bool>,
    /// The windows each reloading plugin had open, for it to open again as it declares them.
    carried_open: std::collections::BTreeMap<String, BTreeSet<String>>,
    storage: Box<dyn PluginStorage>,
    plugins: Vec<Slot>,
    pending: VecDeque<Pending>,
    observer: observe::Observer,
    /// The player's input since the last frame, in the order it came: each on a plugin's own
    /// window or bar button for that plugin, and each press on the game's map (no plugin's) for
    /// every plugin listening.
    ui_inbox: VecDeque<(Option<String>, Event)>,
    shared: Shared,
    /// The host's own lines (loads, refusals, suspensions), kept for a test or a tool to read; each
    /// is in the client's log too.
    console: VecDeque<String>,
    last_flush: Option<f64>,
    now: f64,
    claimed_once: bool,
    /// When the source was last asked what changed.
    last_watch: f64,
    /// The plugin panel: the plugins, their state, and the buttons that act on the chosen one.
    panel: Option<Window>,
    /// The panel's chosen plugin, by id.
    panel_chosen: Option<String>,
    /// The player has unlocked the plugins' HUDs (`/plugin huds`): a press on one moves it, and its
    /// controls take no input until they are locked again.
    huds_unlocked: bool,
    /// The host's panel switches that are on, by id.
    switches_on: BTreeSet<String>,
    /// The other clients on this machine.
    peers: Box<dyn PluginPeers>,
    /// The plugins' HTTPS requests.
    net: Box<dyn PluginNet>,
    /// The plugins' data folders.
    files: Box<dyn PluginFiles>,
    /// Every marker, resolved at the last frame.
    overlays: Overlays,
    /// Whether the policy changed since plugins last heard it.
    policy_changed: bool,
    /// The part of the session the last frame was in; none before the first frame.
    context: Option<Context>,
    /// The toasts' window, while any shows.
    toasts: Option<Window>,
    /// Hotkeys pressed since the last frame: plugin and node.
    hotkeys_pressed: Vec<(String, String)>,
    /// Commands the player ran from the interface's own controls, `(plugin, verb)`, for the next
    /// frame.
    commands_run: Vec<(String, String)>,
    /// The files plugins asked the player for, and the files the one in front is offered.
    import: import::Import,
    /// The game's data files, for the plugins that read landblocks and cells.
    game_data: Box<dyn gamedata::PluginGameData>,
    /// Whether the interface shown draws the plugin windows (the toasts among them).
    windows_shown: bool,
    /// All plugins' time in the VMs this frame.
    frame_spent: web_time::Duration,
    /// What the time in the VMs is read from.
    vm_clock: Box<dyn VmClock>,
    /// A plugin is being loaded: its entry is not held to its share of the frame.
    loading: bool,
    /// A plugin's entry chunk is running: a failure there is told as its failure to load.
    entry_chunk: bool,
    /// The toasts the chat has been told of, while the interface shown draws none.
    toasts_told: Vec<(String, String, f64)>,
}

/// The part of the session a frame is in: the world while a character is in it, the character
/// creation screens while they are up, and before the world otherwise.
#[must_use]
pub fn context_of(frame: &HookFrame<'_>) -> Context {
    // In the world once the character is there and named: a plugin finds its character's store
    // open as it begins there.
    if frame.view.player().is_some() && frame.view.character_name().is_some() {
        Context::World
    } else if frame.session.creating_character {
        Context::Chargen
    } else {
        Context::Pregame
    }
}

/// Whether event `e` answers one of the player's gestures: an input on the plugin's own window or
/// its bar button, or one of its commands typed, bound to a key or put on a slot.
fn answers_the_player(e: &Event) -> bool {
    e.name == "command" || e.name.starts_with("ui.")
}

impl PluginHost {
    #[must_use]
    pub fn new(
        config: HostConfig,
        engine: Box<dyn ScriptEngine>,
        source: Box<dyn PluginSource>,
        storage: Box<dyn PluginStorage>,
    ) -> Self {
        Self {
            config,
            engines: vec![engine],
            source,
            explicit: BTreeSet::new(),
            found: Vec::new(),
            found_known: Vec::new(),
            ticks: std::collections::BTreeMap::new(),
            carried_open: std::collections::BTreeMap::new(),
            storage,
            plugins: Vec::new(),
            pending: VecDeque::new(),
            observer: observe::Observer::default(),
            ui_inbox: VecDeque::new(),
            shared: Shared::default(),
            console: VecDeque::new(),
            last_flush: None,
            now: 0.0,
            claimed_once: false,
            last_watch: 0.0,
            panel: None,
            panel_chosen: None,
            huds_unlocked: false,
            switches_on: BTreeSet::new(),
            peers: Box::new(NoPeers),
            net: Box::new(NoNet::default()),
            files: Box::new(NoFiles),
            overlays: Overlays::default(),
            policy_changed: false,
            context: None,
            toasts: None,
            hotkeys_pressed: Vec::new(),
            commands_run: Vec::new(),
            import: import::Import::default(),
            game_data: Box::new(gamedata::NoGameData),
            windows_shown: true,
            toasts_told: Vec::new(),
            frame_spent: web_time::Duration::ZERO,
            vm_clock: Box::new(MachineClock::default()),
            loading: false,
            entry_chunk: false,
        }
    }

    /// The same host, also running packages whose entry `engine` handles (a compiled plugin's
    /// `.wasm`, say). The engine given to [`PluginHost::new`] is asked first.
    #[must_use]
    pub fn with_engine(mut self, engine: Box<dyn ScriptEngine>) -> Self {
        self.engines.push(engine);
        self
    }

    /// The same host, talking to the other clients on this machine through `peers`.
    #[must_use]
    pub fn with_peers(mut self, peers: Box<dyn PluginPeers>) -> Self {
        self.peers = peers;
        self
    }

    /// The same host, reading the game's data files through `data` for the plugins that ask
    /// (`game.data`).
    #[must_use]
    pub fn with_game_data(mut self, data: Box<dyn gamedata::PluginGameData>) -> Self {
        self.game_data = data;
        self
    }

    /// What plugins have asked of the game's own panels: each panel's state as the last plugin
    /// to set it left it, `(panel, plugin, hidden, at)`.
    #[must_use]
    pub fn retail_ui(&self) -> Vec<calls::RetailRequest> {
        self.shared
            .retail_ui
            .iter()
            .map(|(k, (p, h, a))| (k.clone(), p.clone(), *h, *a))
            .collect()
    }

    /// The same host, reading the time the plugins take in their VMs, for their shares of a frame,
    /// from `clock` rather than the machine's own.
    #[must_use]
    pub fn with_vm_clock(mut self, clock: Box<dyn VmClock>) -> Self {
        self.vm_clock = clock;
        self
    }

    /// The same host, making the plugins' HTTPS requests through `net`.
    #[must_use]
    pub fn with_files(mut self, files: Box<dyn PluginFiles>) -> Self {
        self.files = files;
        self
    }

    /// The same host, keeping the plugins' stores in `storage`.
    #[must_use]
    pub fn with_storage(mut self, storage: Box<dyn PluginStorage>) -> Self {
        self.storage = storage;
        self
    }

    /// The same host, with the plugins' HTTPS requests made through `net`.
    #[must_use]
    pub fn with_net(mut self, net: Box<dyn PluginNet>) -> Self {
        self.net = net;
        self
    }

    /// The world's policy, as its world entry published it (or the policy of a world that
    /// publishes none, which allows everything).
    pub fn set_world_policy(&mut self, policy: WorldPolicy) {
        if policy != self.config.policy {
            self.config.policy = policy;
            self.policy_changed = true;
        }
    }

    /// The policy in force: the world's.
    #[must_use]
    pub fn policy(&self) -> &WorldPolicy {
        &self.config.policy
    }

    /// Every marker as it stood at the last frame, for whatever draws them.
    #[must_use]
    pub fn overlays(&self) -> &Overlays {
        &self.overlays
    }

    /// The bytes of image `src` of plugin `plugin`: a file of its package (`maps/town.png`) or an
    /// image it fetched (`net:<n>`). Only PNG files are answered.
    #[must_use]
    pub fn image(&self, plugin: &str, src: &str) -> Option<(&[u8], u64)> {
        let slot = self.slot(plugin)?;
        // Only a picture kept as it arrived, with the hash it was given then.
        let hash = *slot.state.pictures.kept.get(src)?;
        let bytes = if src.starts_with("net:") {
            slot.state.images.get(src)?
        } else {
            slot.state.package.files.get(src)?
        };
        Some((bytes.as_slice(), hash))
    }

    #[must_use]
    pub fn config(&self) -> &HostConfig {
        &self.config
    }

    /// The same host, offering the plugins it finds in `source` (a plugins folder, the plugins
    /// that come with the client): each is listed (in the panel and by `/plugin list`) and stays
    /// off until the player enables it (`/plugin enable <id>`, or the panel's Enable), which the
    /// host remembers. A plugin named to load is that plugin instead of a found one of the same
    /// id, and of two found ones of an id, the first offered is listed: the sources are read in
    /// the order they are given, this one's and [`Self::with_offered`]'s alike.
    #[must_use]
    pub fn with_found(mut self, source: Box<dyn PluginSource>) -> Self {
        self.found.push(FoundSource {
            source,
            offered: false,
        });
        self
    }

    /// The same host, offering the plugins a launcher offers in `source` (the ones it installed,
    /// and the ones in the player's own folders it lists): found plugins as
    /// [`Self::with_found`]'s are, each ticked the first time the host sees it (by its id, so the
    /// copy listed is ticked whichever source lists it first). After that the player's tick
    /// decides: one they untick stays off at every start, though it is offered still.
    #[must_use]
    pub fn with_offered(mut self, source: Box<dyn PluginSource>) -> Self {
        self.found.push(FoundSource {
            source,
            offered: true,
        });
        self
    }

    /// Queue every package the explicit source offers for loading at the next frame, and the
    /// found ones that are ticked.
    pub fn load_all(&mut self) {
        let own = self.source.list();
        self.explicit = own
            .iter()
            .filter_map(|p| p.manifest().ok())
            .map(|m| m.id.to_string())
            .collect();
        for p in own {
            self.pending.push_back(Pending::Load(p, Why::Other));
        }
        self.ticks = self.read_ticks();
        let found = self.read_found();
        self.queue_ticked(found);
    }

    /// Read the found plugins afresh: each one listed (the explicit source's own ids and a second
    /// of an id left out), in the order the sources were given. One the player has no tick on is
    /// ticked where it replaces a plugin they had ticked, and where a launcher offers its id and it
    /// replaces no plugin they had a tick on. Answers the packages listed, in order.
    fn read_found(&mut self) -> Vec<Package> {
        let mut packages = Vec::new();
        let mut offered = BTreeSet::new();
        for f in &mut self.found {
            for p in f.source.list() {
                if f.offered {
                    if let Ok(m) = p.manifest() {
                        offered.insert(m.id.to_string());
                    }
                }
                packages.push(p);
            }
        }
        self.found_known.clear();
        let mut out = Vec::new();
        for p in packages {
            let Ok(m) = p.manifest() else { continue };
            let id = m.id.to_string();
            if self.explicit.contains(&id) || self.found_known.iter().any(|(k, ..)| *k == id) {
                continue;
            }
            self.found_known
                .push((id.clone(), m.name.clone(), m.version.to_string()));
            if !self.ticks.contains_key(&id) {
                let earlier: Vec<bool> = m
                    .replaces
                    .iter()
                    .filter_map(|old| self.ticks.get(old.as_str()).copied())
                    .collect();
                if earlier.contains(&true) || (earlier.is_empty() && offered.contains(&id)) {
                    self.ticks.insert(id.clone(), true);
                    self.remember_tick(&id, true);
                }
            }
            out.push(p);
        }
        out
    }

    /// Whether the found plugin `id` is ticked.
    fn ticked(&self, id: &str) -> bool {
        self.ticks.get(id).copied().unwrap_or(false)
    }

    /// Queue the ticked ones of `found` that are not loaded.
    fn queue_ticked(&mut self, found: Vec<Package>) {
        for p in found {
            let Ok(m) = p.manifest() else { continue };
            if self.ticked(m.id.as_str()) && self.index(m.id.as_str()).is_none() {
                self.pending.push_back(Pending::Load(p, Why::Other));
            }
        }
    }

    /// The player's tick on each found plugin, read from the host's store.
    fn read_ticks(&mut self) -> std::collections::BTreeMap<String, bool> {
        self.storage
            .load(HOST_ID, TICKED_SCOPE)
            .and_then(|t| Value::from_json(&t).ok())
            .and_then(|v| v.as_map().cloned())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|(id, on)| on.as_bool().map(|on| (id, on)))
            .collect()
    }

    /// The plugins the host found, in the order they are offered, each with whether it is ticked.
    #[must_use]
    pub fn found(&self) -> Vec<FoundInfo> {
        self.found_known
            .iter()
            .map(|(id, name, version)| FoundInfo {
                id: id.clone(),
                name: name.clone(),
                version: version.clone(),
                on: self.ticked(id),
            })
            .collect()
    }

    /// Enable plugin `id` (`on`: tick it, and load it at the next frame, or start it again where it
    /// is loaded but stopped on a fault) or disable it (untick it, and unload it at the next
    /// frame); the host remembers a found plugin's tick, an untick as much as a tick. A plugin
    /// named to load is enabled again but never disabled: it is not ticked. The chat says what
    /// happened as it happens.
    ///
    /// # Errors
    /// No such plugin was found or named to load, its package could not be read, or it was named
    /// to load and so is not disabled: the answer is for the player.
    pub fn set_ticked(&mut self, id: &str, on: bool) -> Result<(), String> {
        let found = self.found_known.iter().any(|(k, ..)| k == id);
        if !found && !self.explicit.contains(id) {
            return Err(format!("no plugin {id} was found"));
        }
        if !on && !found {
            return Err(format!(
                "{} was named to load, and is not disabled",
                self.name_of(id)
            ));
        }
        if found {
            self.remember_tick(id, on);
            self.ticks.insert(id.to_owned(), on);
        }
        // What was asked for this plugin and not yet done gives way to this.
        self.pending.retain(|p| p.id().as_deref() != Some(id));
        let running = self.slot(id).map(|s| s.state.status == Status::Running);
        match (on, running) {
            (true, Some(true)) => {
                let name = self.name_of(id);
                self.tell(&format!("{name} is enabled already."));
            }
            (true, Some(false)) => self
                .pending
                .push_back(Pending::Reload(id.to_owned(), Why::Enable)),
            (true, None) => {
                let package = dereth_plugin_api::PluginId::parse(id)
                    .and_then(|pid| {
                        if found {
                            self.fetch_found(&pid)
                        } else {
                            self.source.fetch(&pid)
                        }
                    })
                    .ok_or_else(|| format!("{id} could not be read"))?;
                self.pending.push_back(Pending::Load(package, Why::Enable));
            }
            (false, Some(_)) => self
                .pending
                .push_back(Pending::Unload(id.to_owned(), Why::Disable)),
            (false, None) => {
                let name = self.name_of(id);
                self.tell_life(&name, "disabled");
            }
        }
        Ok(())
    }

    /// Reload every plugin that runs, at the next frame, each as the panel's Reload does.
    pub fn reload_all(&mut self) {
        let running: Vec<String> = self
            .plugins
            .iter()
            .filter(|s| s.state.status == Status::Running)
            .map(|s| s.state.manifest.id.to_string())
            .collect();
        for id in running {
            self.pending.push_back(Pending::Reload(id, Why::Reload));
        }
    }

    /// Plugin `id`'s name as the host's lines tell it: its manifest's (a loaded plugin's, else the
    /// one found), else its id.
    fn name_of(&self, id: &str) -> String {
        if let Some(s) = self.slot(id) {
            return told_name(&s.state.manifest);
        }
        self.found_known
            .iter()
            .find(|(k, ..)| k == id)
            .map(|(_, name, _)| shown::bounded(name, shown::NAME))
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| id.to_owned())
    }

    /// Tell the chat that the plugin named `name` was `done`: `[Plugins] XP Meter loaded.`
    fn tell_life(&mut self, name: &str, done: &str) {
        self.tell(&format!("{name} {done}."));
    }

    /// Keep the player's tick on the found plugin `id`, in the host's store.
    fn remember_tick(&mut self, id: &str, on: bool) {
        let mut all = self
            .storage
            .load(HOST_ID, TICKED_SCOPE)
            .and_then(|t| Value::from_json(&t).ok())
            .and_then(|v| v.as_map().cloned())
            .unwrap_or_default();
        all.insert(id.to_owned(), Value::Bool(on));
        self.storage
            .save(HOST_ID, TICKED_SCOPE, &Value::Map(all).to_json());
    }

    /// The current files of found plugin `id`: the first source's that has it, as it is listed.
    fn fetch_found(&mut self, id: &dereth_plugin_api::PluginId) -> Option<Package> {
        self.found.iter_mut().find_map(|f| f.source.fetch(id))
    }

    /// The current files of plugin `id`: the explicit source's, else the found sources'.
    fn fetch_any(&mut self, id: &str) -> Option<Package> {
        let pid = dereth_plugin_api::PluginId::parse(id)?;
        self.source.fetch(&pid).or_else(|| self.fetch_found(&pid))
    }

    /// Queue one package for loading at the next frame.
    pub fn request_load(&mut self, package: Package) {
        self.pending.push_back(Pending::Load(package, Why::Other));
    }

    /// Queue an unload at the next frame (the plugin stays ticked: this is the client's own stop,
    /// not the player's Disable).
    pub fn request_unload(&mut self, id: &str) {
        self.pending
            .push_back(Pending::Unload(id.to_owned(), Why::Other));
    }

    /// Queue a reload (unload, read the package again, load, and hand the old state to the new
    /// instance) at the next frame.
    pub fn request_reload(&mut self, id: &str) {
        self.pending
            .push_back(Pending::Reload(id.to_owned(), Why::Reload));
    }

    /// The loaded plugins, in load order.
    #[must_use]
    pub fn plugins(&self) -> Vec<PluginInfo> {
        self.plugins
            .iter()
            .map(|s| {
                let st = &s.state;
                PluginInfo {
                    id: st.manifest.id.to_string(),
                    name: st.manifest.name.clone(),
                    version: st.manifest.version.to_string(),
                    status: st.status.word().to_owned(),
                    reason: match &st.status {
                        Status::Running => None,
                        Status::Faulted(r) | Status::Suspended(r) => Some(r.clone()),
                    },
                    permissions: st.manifest.permissions.keys().map(|p| p.name()).collect(),
                    active: st.active,
                    contexts: st
                        .manifest
                        .contexts
                        .iter()
                        .map(|c| c.name().to_owned())
                        .collect(),
                }
            })
            .collect()
    }

    /// What plugin `id` printed and the errors it met, oldest first: its record, for a test or a
    /// tool to read (the player reads each line in the chat as it comes).
    #[must_use]
    pub fn console(&self, id: &str) -> Option<Vec<String>> {
        self.slot(id)
            .map(|s| s.state.console.iter().cloned().collect())
    }

    /// The host's own lines, oldest first.
    #[must_use]
    pub fn host_console(&self) -> Vec<String> {
        self.console.iter().cloned().collect()
    }

    /// Every declared window of every active plugin, and the host's own: `(plugin id, window id,
    /// window)`. An inactive plugin's windows are left out (hidden) and come back as they were
    /// when it is active again.
    #[must_use]
    pub fn windows(&self) -> Vec<(String, String, &Window)> {
        self.plugins
            .iter()
            .filter(|s| s.state.active)
            .flat_map(|s| {
                let id = s.state.manifest.id.to_string();
                s.state
                    .windows
                    .iter()
                    .map(move |(w, win)| (id.clone(), w.clone(), win))
            })
            .chain(
                self.panel
                    .iter()
                    .map(|p| (HOST_ID.to_owned(), PANEL.to_owned(), p)),
            )
            .chain(
                self.toasts
                    .iter()
                    .map(|p| (HOST_ID.to_owned(), TOASTS.to_owned(), p)),
            )
            .collect()
    }

    /// Whether a window is the host's own, drawn above every plugin window: the plugin panel,
    /// whose buttons enable, disable and reload plugins, so that no plugin can cover or imitate
    /// it.
    #[must_use]
    pub fn is_host_window(plugin: &str, window: &str) -> bool {
        plugin == HOST_ID && window == PANEL
    }

    /// One window.
    #[must_use]
    pub fn window(&self, plugin: &str, window: &str) -> Option<&Window> {
        if plugin == HOST_ID && window == PANEL {
            return self.panel.as_ref();
        }
        if plugin == HOST_ID && window == TOASTS {
            return self.toasts.as_ref();
        }
        self.slot(plugin)?.state.windows.get(window)
    }

    /// Whether the player has unlocked the plugins' HUDs: while they are, a press on a HUD moves
    /// it (the interface keeps its place for the character, on the screen), and its controls take
    /// no input.
    #[must_use]
    pub fn huds_unlocked(&self) -> bool {
        self.huds_unlocked
    }

    /// Unlock or lock the plugins' HUDs, as `/plugin huds` does.
    pub fn set_huds_unlocked(&mut self, on: bool) {
        self.huds_unlocked = on;
    }

    /// Whether the host's panel switch `id` is on.
    #[must_use]
    pub fn switch(&self, id: &str) -> bool {
        self.switches_on.contains(id)
    }

    /// Turn the host's panel switch `id` on or off, as the player's tick on the panel does.
    pub fn set_switch(&mut self, id: &str, on: bool) {
        let changed = if on {
            self.switches_on.insert(id.to_owned())
        } else {
            self.switches_on.remove(id)
        };
        if changed {
            self.refresh_panel();
        }
    }

    /// Change which actions this host cannot carry out: each of `plain` fails at once as
    /// `unavailable`, and each of `now` as `unavailable (why)`.
    pub fn set_unavailable(&mut self, plain: Vec<&'static str>, now: Vec<(&'static str, String)>) {
        self.config.unavailable = plain;
        self.config.unavailable_now = now;
    }

    /// Open the plugin panel (it is built at the next frame).
    pub fn show_panel(&mut self) {
        let tree = self.panel_tree();
        let revision = self.panel.as_ref().map_or(1, |p| p.revision + 1);
        self.panel = Some(Window {
            tree,
            open: true,
            revision,
            size: None,
            by_player: true,
        });
    }

    /// Close every plugin window and HUD that is open, and the plugin panel: the player logged out.
    /// Each opens again only when the player asks, as at the start.
    fn close_windows(&mut self) {
        for s in &mut self.plugins {
            for w in s.state.windows.values_mut() {
                if w.open {
                    w.open = false;
                    w.by_player = false;
                    w.revision += 1;
                }
            }
        }
        if let Some(p) = &mut self.panel {
            if p.open {
                p.open = false;
                p.revision += 1;
            }
        }
    }

    /// The panel's rows: the loaded plugins, in load order, then the found plugins that are not
    /// loaded: `disabled` where the player unticked them (or never ticked them), `not loaded`
    /// where they are ticked but did not load.
    fn panel_rows(&self) -> Vec<PanelRow> {
        let found = self.found();
        let tick = |id: &str| -> Option<bool> { found.iter().find(|b| b.id == id).map(|b| b.on) };
        let mut list: Vec<PanelRow> = self
            .plugins()
            .into_iter()
            .map(|info| PanelRow {
                running: info.status == "running",
                ticked: tick(&info.id),
                info,
            })
            .collect();
        for b in found {
            if !list.iter().any(|r| r.info.id == b.id) {
                list.push(PanelRow {
                    info: PluginInfo {
                        id: b.id,
                        name: b.name,
                        version: b.version,
                        status: if b.on { "not loaded" } else { "disabled" }.to_owned(),
                        reason: None,
                        permissions: Vec::new(),
                        active: false,
                        contexts: Vec::new(),
                    },
                    running: false,
                    ticked: Some(b.on),
                });
            }
        }
        list
    }

    /// The panel's tree from the host's state: a list of the loaded plugins and the found ones
    /// (their names taking the width their version and state leave), the chosen one's reason
    /// (why it stopped), the host's own switches, and the buttons: Enable (lit for a plugin that
    /// is not running), Disable (one that is ticked), Reload (one running) and Reload All (lit
    /// while any runs).
    fn panel_tree(&self) -> dereth_plugin_api::UiNode {
        let list = self.panel_rows();
        let chosen = self
            .panel_chosen
            .as_deref()
            .and_then(|id| list.iter().position(|r| r.info.id == id));
        let row = chosen.map(|i| &list[i]);
        // What a plugin wrote (its name, the error it faulted on) is shown as its words, quoted
        // and in their own colour, never as the client's.
        let rows: Vec<Value> = list
            .iter()
            .map(|r| {
                let p = &r.info;
                Value::List(vec![
                    shown::plugin_cell(&p.name, shown::NAME),
                    Value::from(p.version.as_str()),
                    // A running plugin outside its contexts waits for one of them.
                    if p.status == "running" && !p.active {
                        Value::from(format!("idle ({})", p.contexts.join(", ")))
                    } else {
                        Value::from(p.status.as_str())
                    },
                ])
            })
            .collect();
        let faulted = row.is_some_and(|r| r.info.status == "faulted");
        let node = |kind: &str| Value::map().with("kind", kind);
        // With no plugin to list, the panel says so.
        let none: Vec<Value> = if list.is_empty() {
            vec![node("label").with("text", NO_PLUGINS)]
        } else {
            Vec::new()
        };
        let button = |id: &str, text: &str, lit: bool| {
            node("button")
                .with("id", id)
                .with("text", text)
                .with("enabled", lit)
        };
        let reason = {
            let why = row.and_then(|r| r.info.reason.clone()).unwrap_or_default();
            let label = node("label").with("id", "reason");
            // A fault's reason is the plugin's error, its own words.
            if faulted {
                label
                    .with("text", shown::quoted(&why, shown::LINE))
                    .with("color", shown::PLUGIN_COLOUR)
            } else {
                label.with("text", shown::bounded(&why, shown::LINE))
            }
        };
        let buttons = node("row").with(
            "children",
            vec![
                button("enable", "Enable", row.is_some_and(|r| !r.running)),
                button(
                    "disable",
                    "Disable",
                    row.is_some_and(|r| r.ticked == Some(true)),
                ),
                button("reload", "Reload", row.is_some_and(|r| r.running)),
                button("reload_all", "Reload All", list.iter().any(|r| r.running)),
            ],
        );
        let v = node("window")
            .with("id", PANEL)
            .with("title", "Plugins")
            .with("width", 60_i64)
            .with(
                "children",
                none.into_iter()
                    .chain([
                        node("list")
                            .with("id", "plugins")
                            .with(
                                "columns",
                                vec![
                                    Value::map().with("title", "Plugin").with("grow", 1_i64),
                                    Value::map().with("title", "Version").with("width", 8_i64),
                                    Value::map().with("title", "State").with("width", 16_i64),
                                ],
                            )
                            .with("rows", rows)
                            .with(
                                "selected",
                                chosen.map_or(Value::Nil, |i| {
                                    Value::Int(i64::try_from(i).unwrap_or(0) + 1)
                                }),
                            )
                            .with("height", 10_i64)
                            .with("grow", 1_i64),
                        reason,
                    ])
                    .chain(self.config.panel_switches.iter().map(|(id, text)| {
                        node("toggle")
                            .with("id", *id)
                            .with("text", text.as_str())
                            .with("value", self.switch(id))
                    }))
                    .chain([buttons])
                    .collect::<Vec<_>>(),
            );
        dereth_plugin_api::UiNode::window_from_value(&v)
            .unwrap_or_else(|e| panic!("the plugin panel's own tree is valid: {e}"))
    }

    /// Rebuild the open panel when what it shows has changed.
    fn refresh_panel(&mut self) {
        let Some(p) = &self.panel else { return };
        if !p.open {
            return;
        }
        let tree = self.panel_tree();
        if let Some(p) = &mut self.panel {
            if p.tree != tree {
                p.tree = tree;
                p.revision += 1;
            }
        }
    }

    /// The player's input on the plugin panel.
    fn panel_input(&mut self, input: UiInput) -> Result<(), String> {
        match input {
            UiInput::Select { node, rows } if node == "plugins" => {
                let list = self.panel_rows();
                self.panel_chosen = rows
                    .first()
                    .and_then(|r| usize::try_from(*r).ok())
                    .and_then(|r| list.get(r.wrapping_sub(1)))
                    .map(|r| r.info.id.clone());
            }
            UiInput::Click { node } if node == "enable" || node == "disable" => {
                if let Some(id) = self.panel_chosen.clone() {
                    if let Err(why) = self.set_ticked(&id, node == "enable") {
                        self.tell(&why);
                    }
                }
            }
            UiInput::Click { node } if node == "reload" => {
                if let Some(id) = self.panel_chosen.clone() {
                    self.request_reload(&id);
                }
            }
            UiInput::Click { node } if node == "reload_all" => self.reload_all(),
            UiInput::Change { node, value }
                if self.config.panel_switches.iter().any(|(id, _)| *id == node) =>
            {
                if value.as_bool().unwrap_or(false) {
                    self.switches_on.insert(node);
                } else {
                    self.switches_on.remove(&node);
                }
            }
            UiInput::Close => {
                if let Some(p) = &mut self.panel {
                    p.open = false;
                    p.revision += 1;
                }
                return Ok(());
            }
            other => return Err(format!("the plugin panel takes no {other:?}")),
        }
        self.refresh_panel();
        Ok(())
    }

    /// A player's input on a plugin window, delivered to the plugin at the next frame. A change
    /// or a close is applied to the window at once, so it is drawn as the player left it.
    ///
    /// # Errors
    /// No such plugin, window or node.
    pub fn ui_input(&mut self, plugin: &str, window: &str, input: UiInput) -> Result<(), String> {
        if plugin == HOST_ID && window == PANEL {
            return self.panel_input(input);
        }
        let unlocked = self.huds_unlocked;
        let slot = self
            .plugins
            .iter_mut()
            .find(|s| s.state.manifest.id.as_str() == plugin)
            .ok_or_else(|| format!("no plugin {plugin:?}"))?;
        if !slot.state.active {
            return Err(format!("{plugin} is not active here"));
        }
        let win = slot
            .state
            .windows
            .get_mut(window)
            .ok_or_else(|| format!("no window {window:?}"))?;
        // While the HUDs are unlocked a press on one moves it: its controls take no input.
        if unlocked
            && win.tree.kind == dereth_plugin_api::NodeKind::Hud
            && matches!(
                input,
                UiInput::Click { .. } | UiInput::Change { .. } | UiInput::Select { .. }
            )
        {
            return Ok(());
        }
        let base = Value::map().with("window", window);
        let event = match input {
            UiInput::Click { node } => {
                win.tree
                    .find(&node)
                    .ok_or_else(|| format!("no node {node:?}"))?;
                Event::new("ui.clicked", base.with("id", node))
            }
            UiInput::Change { node, value } => {
                win.tree
                    .set_prop(&node, "value", value.clone())
                    .map_err(|e| e.to_string())?;
                // Text the player is typing is already shown as typed: the window is not built
                // again under the caret.
                let typed = win.tree.find(&node).is_some_and(|n| {
                    matches!(
                        n.kind,
                        dereth_plugin_api::NodeKind::Input | dereth_plugin_api::NodeKind::TextBox
                    )
                });
                if !typed {
                    win.revision += 1;
                }
                Event::new("ui.changed", base.with("id", node).with("value", value))
            }
            UiInput::Select { node, rows } => {
                let first = rows.first().copied().map_or(Value::Nil, Value::Int);
                win.tree
                    .set_prop(&node, "selected", first)
                    .map_err(|e| e.to_string())?;
                win.revision += 1;
                Event::new(
                    "ui.selected",
                    base.with("id", node).with(
                        "rows",
                        Value::List(rows.into_iter().map(Value::Int).collect()),
                    ),
                )
            }
            UiInput::Close => {
                win.open = false;
                win.revision += 1;
                Event::new("ui.closed", base)
            }
            UiInput::Resize { width, height } => {
                if win.tree.props.get("resizable").and_then(Value::as_bool) != Some(true) {
                    return Err(format!("window {window:?} is not resizable"));
                }
                // Within the bounds the plugin declared, and never below one cell and one line.
                let bound = |k: &str| win.tree.props.get(k).and_then(Value::as_i64);
                let clamp = |v: i64, lo: Option<i64>, hi: Option<i64>| {
                    let v = hi.map_or(v, |h| v.min(h));
                    lo.map_or(v, |l| v.max(l)).max(1)
                };
                let w = clamp(width, bound("min_width"), bound("max_width"));
                let h = clamp(height, bound("min_height"), bound("max_height"));
                win.size = Some((w, h));
                win.revision += 1;
                let key = format!("size/{plugin}/{window}");
                self.shared
                    .layout
                    .insert(key, Value::map().with("width", w).with("height", h));
                self.shared.layout_dirty = true;
                Event::new("ui.resized", base.with("width", w).with("height", h))
            }
        };
        self.ui_inbox.push_back((Some(plugin.to_owned()), event));
        Ok(())
    }

    /// The player pressed the game's world map: at the next frame every plugin active here that
    /// listens hears `map.clicked`, with the place under the pointer, the button and the
    /// modifiers held. It is no gesture, the map being the game's and no plugin's own. A host
    /// without [`HostCapability::MapClicks`] tells no plugin, and a press off the world (on the
    /// map's margin) is not told.
    pub fn map_click(&mut self, click: &MapClick) {
        let round = |v: f64| (v * 1000.0).round() / 1000.0;
        let (ns, ew) = (round(click.ns), round(click.ew));
        if !self
            .config
            .capabilities
            .contains(&HostCapability::MapClicks)
            || places::from_coords(ns, ew, 0.0).is_none()
        {
            return;
        }
        let event = Event::new(
            "map.clicked",
            Value::map()
                .with("position", Value::map().with("ns", ns).with("ew", ew))
                .with("button", click.button.name())
                .with(
                    "modifiers",
                    Value::map()
                        .with("ctrl", click.ctrl)
                        .with("shift", click.shift)
                        .with("alt", click.alt),
                ),
        );
        self.ui_inbox.push_back((None, event));
    }

    /// The player's answer to the file a plugin asks for: the file picked given, or the request
    /// refused.
    fn import_reply(
        &mut self,
        reply: &dereth_client_runtime::dialogs::HostReply,
    ) -> Result<(), String> {
        let Some(r) = self.import.queue.front().cloned() else {
            return Err("no plugin is asking for a file".into());
        };
        let answer = if reply.yes {
            let Some(name) = reply.picked.and_then(|i| self.import.files.get(i)).cloned() else {
                return Err("no file is chosen".into());
            };
            match self
                .files
                .read(HOST_ID, &format!("{}{name}", import::FOLDER))
            {
                Some(b) if b.len() <= import::MAX_BYTES => Ok(Value::map()
                    .with("name", name.as_str())
                    .with("text", String::from_utf8_lossy(&b).into_owned())),
                Some(_) => Err("the file is too large".to_owned()),
                None => Err("the file could not be read".to_owned()),
            }
        } else {
            self.import.refused(&r.plugin, self.now);
            Err("refused".to_owned())
        };
        self.import.queue.pop_front();
        self.host_log(format!(
            "{} {} a file",
            r.plugin,
            if answer.is_ok() {
                "was given"
            } else {
                "was refused"
            }
        ));
        self.shared.actions.settle_later(actions::Settled {
            plugin: r.plugin,
            id: r.id,
            action: "files.import",
            result: answer,
        });
        self.put_import();
        Ok(())
    }

    /// Put the request in front before the player with the files it may be given, or take the
    /// question down when none waits.
    fn put_import(&mut self) {
        let files = match self.import.queue.front() {
            Some(r) => import::Import::matching(&self.files.list(HOST_ID), r),
            None => Vec::new(),
        };
        self.import.put(files);
    }

    /// Whether the interface shown draws the plugin windows. While it does not, each toast is told
    /// in the chat once instead.
    pub fn set_windows_shown(&mut self, shown: bool) {
        self.windows_shown = shown;
    }

    /// The question the host is putting to the player now: a file a plugin asked for, picked
    /// from the list. Only the host raises it, and only [`Self::reply`] answers it.
    #[must_use]
    pub fn question(&self) -> Option<dereth_client_runtime::dialogs::HostQuestion> {
        let r = self.import.queue.front()?;
        let asker = match self.slot(&r.plugin) {
            Some(s) => format!(
                "{} {} ({})",
                shown::bounded(&s.state.manifest.name, shown::NAME),
                shown::bounded(&s.state.manifest.version.to_string(), shown::NAME),
                r.plugin,
            ),
            None => r.plugin.clone(),
        };
        self.import.question(asker)
    }

    /// The player's answer to the question [`Self::question`] put as `reply.id`: the file picked
    /// given, or the request refused. An answer to a question no longer put answers nothing.
    ///
    /// # Errors
    /// `reply.id` is not the question put, or it gives a file with none picked.
    pub fn reply(
        &mut self,
        reply: &dereth_client_runtime::dialogs::HostReply,
    ) -> Result<(), String> {
        if self.question().map(|q| q.id) != Some(reply.id) {
            return Err("that question is no longer asked".to_owned());
        }
        self.import_reply(reply)
    }

    /// [`Self::reply`] with the question numbered `id` answered as it was put: `yes` gives no
    /// file (none is picked), `false` refuses it.
    ///
    /// # Errors
    /// As [`Self::reply`].
    pub fn answer_question(&mut self, id: u64, yes: bool) -> Result<(), String> {
        let q = self
            .question()
            .filter(|q| q.id == id)
            .ok_or_else(|| "that question is no longer asked".to_owned())?;
        self.reply(&dereth_client_runtime::dialogs::HostReply::to(&q, yes))
    }

    /// The audit's view of the loaded plugins.
    #[must_use]
    pub fn audit_entries(&self) -> Vec<AuditEntry> {
        self.plugins
            .iter()
            .map(|s| {
                let m = &s.state.manifest;
                AuditEntry {
                    id: m.id.to_string(),
                    name: m.name.clone(),
                    version: m.version.to_string(),
                    authors: m.authors.clone(),
                    homepage: m.homepage.clone(),
                    permissions: m.permissions.keys().map(|p| p.name()).collect(),
                }
            })
            .collect()
    }

    /// Write every changed store out now.
    pub fn flush(&mut self) {
        let Self {
            plugins, storage, ..
        } = self;
        for s in plugins.iter_mut() {
            flush_slot(&mut s.state, storage.as_mut());
        }
        self.save_layout();
    }

    /// Every hotkey of every running plugin active here, in load order: the `hotkey` nodes of its
    /// windows, open or closed.
    #[must_use]
    pub fn hotkeys(&self) -> Vec<Hotkey> {
        fn walk(
            n: &dereth_plugin_api::UiNode,
            out: &mut Vec<(String, String, String, Option<String>)>,
        ) {
            if n.kind == dereth_plugin_api::NodeKind::Hotkey {
                if let Some(id) = &n.id {
                    let key = n.text("key");
                    out.push((
                        id.clone(),
                        n.text("command").to_owned(),
                        n.text("text").to_owned(),
                        (!key.is_empty()).then(|| key.to_owned()),
                    ));
                }
            }
            for c in &n.children {
                walk(c, out);
            }
        }
        let mut out = Vec::new();
        for s in &self.plugins {
            if s.state.status != Status::Running || !s.state.active {
                continue;
            }
            for (w, win) in &s.state.windows {
                let mut found = Vec::new();
                walk(&win.tree, &mut found);
                out.extend(found.into_iter().map(|(node, command, text, key)| Hotkey {
                    plugin: s.state.manifest.id.to_string(),
                    window: w.clone(),
                    node,
                    command,
                    text,
                    key,
                }));
            }
        }
        out
    }

    /// The player pressed a plugin's hotkey: at the next frame its command runs as if the player
    /// had typed it, a gesture.
    ///
    /// # Errors
    /// No such hotkey of a plugin active here.
    pub fn press_hotkey(&mut self, plugin: &str, node: &str) -> Result<(), String> {
        if !self
            .hotkeys()
            .iter()
            .any(|h| h.plugin == plugin && h.node == node)
        {
            return Err(format!("{plugin} has no hotkey {node:?} here"));
        }
        self.hotkeys_pressed
            .push((plugin.to_owned(), node.to_owned()));
        Ok(())
    }

    /// Every command claimed by a running plugin active here, in load order and by verb.
    #[must_use]
    pub fn commands(&self) -> Vec<Command> {
        self.plugins
            .iter()
            .filter(|s| s.state.status == Status::Running && s.state.active)
            .flat_map(|s| {
                let plugin = s.state.manifest.id.to_string();
                s.state.commands.iter().map(move |(verb, help)| Command {
                    plugin: plugin.clone(),
                    verb: verb.clone(),
                    help: help.clone(),
                })
            })
            .collect()
    }

    /// The player ran plugin `plugin`'s command `verb` from one of the interface's own controls:
    /// at the next frame it runs as if the player had typed `/verb`, a gesture.
    ///
    /// # Errors
    /// No such command of a plugin running and active here.
    pub fn run_command(&mut self, plugin: &str, verb: &str) -> Result<(), String> {
        let verb = verb.to_ascii_lowercase();
        if !self
            .commands()
            .iter()
            .any(|c| c.plugin == plugin && c.verb == verb)
        {
            return Err(format!("{plugin} has no command /{verb} here"));
        }
        self.commands_run.push((plugin.to_owned(), verb));
        Ok(())
    }

    /// The plugin bar: each plugin active here, in load order, with its main window: `main`, else
    /// its first window, else its first HUD.
    #[must_use]
    pub fn bar(&self) -> Vec<BarEntry> {
        self.plugins
            .iter()
            .filter(|s| s.state.active)
            .map(|s| {
                let st = &s.state;
                let first = |kind: dereth_plugin_api::NodeKind| {
                    st.windows
                        .iter()
                        .find(|(_, w)| w.tree.kind == kind)
                        .map(|(k, _)| k.clone())
                };
                let window = if st.windows.contains_key("main") {
                    Some("main".to_owned())
                } else {
                    first(dereth_plugin_api::NodeKind::Window)
                        .or_else(|| first(dereth_plugin_api::NodeKind::Hud))
                };
                let open = window
                    .as_ref()
                    .and_then(|w| st.windows.get(w))
                    .is_some_and(|w| w.open);
                BarEntry {
                    id: st.manifest.id.to_string(),
                    name: st.manifest.name.clone(),
                    state: if st.status == Status::Running {
                        st.bar_state.clone()
                    } else {
                        "error".to_owned()
                    },
                    window,
                    open,
                }
            })
            .collect()
    }

    /// The player clicked a plugin on the bar: its main window opens, or closes when open. The
    /// plugin hears `ui.opened` or `ui.closed`.
    ///
    /// # Errors
    /// No such plugin active here, or it has no window.
    pub fn toggle_window(&mut self, plugin: &str) -> Result<(), String> {
        let entry = self
            .bar()
            .into_iter()
            .find(|e| e.id == plugin)
            .ok_or_else(|| format!("{plugin} is not on the bar"))?;
        let window = entry
            .window
            .ok_or_else(|| format!("{plugin} has no window"))?;
        let slot = self
            .plugins
            .iter_mut()
            .find(|s| s.state.manifest.id.as_str() == plugin)
            .ok_or_else(|| format!("no plugin {plugin:?}"))?;
        let w = slot
            .state
            .windows
            .get_mut(&window)
            .ok_or_else(|| format!("no window {window:?}"))?;
        w.open = !w.open;
        w.by_player = w.open;
        w.revision += 1;
        let name = if w.open { "ui.opened" } else { "ui.closed" };
        self.ui_inbox.push_back((
            Some(plugin.to_owned()),
            Event::new(name, Value::map().with("window", window.as_str())),
        ));
        Ok(())
    }

    /// A remembered place or size of the interface, for the character in the world.
    #[must_use]
    pub fn layout(&self, key: &str) -> Option<&Value> {
        self.shared.layout.get(key)
    }

    /// Remember a place or size of the interface for the character in the world (kept with the
    /// stores, at the next save).
    pub fn remember(&mut self, key: &str, value: Value) {
        if self.shared.layout.get(key) != Some(&value) {
            self.shared.layout.insert(key.to_owned(), value);
            self.shared.layout_dirty = true;
        }
    }

    fn layout_key(&self) -> Option<String> {
        Some(format!(
            "{}:{}:{}",
            self.shared.world.as_deref()?,
            self.shared.account.as_deref().unwrap_or(""),
            self.shared.character.as_deref()?
        ))
    }

    fn load_layout(&mut self) {
        let Some(key) = self.layout_key() else {
            return;
        };
        self.shared.layout = self
            .storage
            .load(HOST_ID, LAYOUT_SCOPE)
            .and_then(|t| Value::from_json(&t).ok())
            .and_then(|v| v.get(&key).as_map().cloned())
            .unwrap_or_default();
        self.shared.layout_dirty = false;
    }

    fn save_layout(&mut self) {
        if !self.shared.layout_dirty {
            return;
        }
        let Some(key) = self.layout_key() else {
            return;
        };
        let mut all = self
            .storage
            .load(HOST_ID, LAYOUT_SCOPE)
            .and_then(|t| Value::from_json(&t).ok())
            .and_then(|v| v.as_map().cloned())
            .unwrap_or_default();
        all.insert(key, Value::Map(self.shared.layout.clone()));
        self.storage
            .save(HOST_ID, LAYOUT_SCOPE, &Value::Map(all).to_json());
        self.shared.layout_dirty = false;
    }

    fn slot(&self, id: &str) -> Option<&Slot> {
        self.plugins
            .iter()
            .find(|s| s.state.manifest.id.as_str() == id)
    }

    fn index(&self, id: &str) -> Option<usize> {
        self.plugins
            .iter()
            .position(|s| s.state.manifest.id.as_str() == id)
    }

    fn host_log(&mut self, line: String) {
        // A line can carry a plugin's own text (a failure's message, a manifest's key).
        let line = dereth_plugin_api::engine::console_line(&line).into_owned();
        tracing::info!(target: "dereth::plugins", "{line}");
        if self.console.len() == slot::CONSOLE_LINES {
            self.console.pop_front();
        }
        self.console.push_back(line);
    }

    /// A line in the chat window from the host itself.
    fn tell(&mut self, text: &str) {
        self.shared.printed.push(host_line(text));
    }

    // ---------------------------------------------------------------------------------------------
    // The frame
    // ---------------------------------------------------------------------------------------------

    /// One frame (see the crate documentation).
    pub fn step(&mut self, frame: &mut HookFrame<'_>) {
        self.now = frame.local_time;
        self.frame_spent = web_time::Duration::ZERO;
        for s in &mut self.plugins {
            s.prints = 0;
            s.state.frame_spent = web_time::Duration::ZERO;
        }
        if !frame.outcomes.is_empty() {
            tracing::debug!(target: "dereth::plugin::actions", "at {:.2}: {:?}", self.now, frame.outcomes);
        }
        if self.config.watch && self.now - self.last_watch >= 1.0 {
            self.last_watch = self.now;
            let mut changed = self.source.changed();
            for f in &mut self.found {
                changed.extend(f.source.changed());
            }
            for id in changed {
                if self.index(id.as_str()).is_some() {
                    self.pending
                        .push_back(Pending::Reload(id.to_string(), Why::Reload));
                }
            }
        }
        while let Some(op) = self.pending.pop_front() {
            match op {
                Pending::Load(p, why) => {
                    if let Some(name) = self.load(p, Some(frame), None) {
                        let done = if why == Why::Enable {
                            "enabled"
                        } else {
                            "loaded"
                        };
                        self.tell_life(&name, done);
                    }
                }
                Pending::Unload(id, why) => {
                    if let Some(i) = self.index(&id) {
                        let name = self.name_of(&id);
                        self.unload(i, Some(frame));
                        let done = if why == Why::Disable {
                            "disabled"
                        } else {
                            "unloaded"
                        };
                        self.tell_life(&name, done);
                    } else if why != Why::Disable {
                        self.tell(&format!("no plugin {id} is loaded"));
                    }
                }
                Pending::Reload(id, why) => self.reload(&id, Some(frame), why),
            }
        }
        self.reconcile_contexts(frame);
        self.note_appraisals(frame);

        // The frame's messages first: the state changes below are what they did.
        let mut events: Vec<(Target, Event)> = self.message_events(frame);
        events.extend(
            self.shared
                .session
                .observe(frame)
                .into_iter()
                .map(|e| (Target::All, e)),
        );
        let subscribed: std::collections::BTreeSet<String> = self
            .plugins
            .iter()
            .filter(|s| s.state.status == Status::Running && s.state.active)
            .flat_map(|s| s.state.subscriptions.iter().cloned())
            .collect();
        events.extend(
            self.observer
                .observe(frame, &|name| subscribed.contains(name))
                .into_iter()
                .map(|e| (Target::All, e)),
        );
        // The actions this frame settled, after the state changes that settled them.
        let mut settled = self.shared.actions.settle(frame);
        settled.extend(self.shared.session.settle(frame));
        // The travel's news: journeys' progress to the plugins that asked, and their ends.
        let (journeys, progress) = self.shared.nav.travel_news(frame);
        settled.extend(journeys);
        for (plugin, name, data) in progress {
            if let Some(i) = self.index(&plugin) {
                events.push((Target::One(i), Event::new(name, data)));
            }
        }
        settled.extend(self.net_answers());
        for s in settled {
            let Some(i) = self.index(&s.plugin) else {
                continue;
            };
            let base = Value::map().with("id", s.id).with("action", s.action);
            let e = match s.result {
                Ok(v) => Event::new("action.completed", base.with("value", v)),
                Err(r) => Event::new("action.failed", base.with("reason", r)),
            };
            events.push((Target::One(i), e));
        }
        for (_, e) in &events {
            match e.name.as_str() {
                "session.login" => {
                    self.shared.character = e.data.get("character").as_str().map(str::to_owned);
                    self.shared.world = Some(frame.world_name.unwrap_or("unknown").to_owned());
                    self.shared.account = frame.account.map(str::to_owned);
                    self.open_session_scopes();
                    self.load_layout();
                }
                "session.logout" => {
                    self.flush();
                    self.close_windows();
                    self.shared.character = None;
                    self.shared.layout.clear();
                    for s in &mut self.plugins {
                        s.state.stores.retain(|k, _| {
                            !k.starts_with("character:") && !k.starts_with("world:")
                        });
                    }
                }
                _ => {}
            }
        }
        for (i, line) in frame.chat.iter().enumerate() {
            events.push((Target::All, observe::chat_event(i, line)));
        }
        let typed: Vec<_> = frame.typed.to_vec();
        for c in typed {
            if c.verb == HOST_VERB {
                self.host_command(&c.words, Some(frame));
                continue;
            }
            if let Some(i) = self
                .plugins
                .iter()
                .position(|s| s.state.commands.contains_key(&c.verb))
            {
                if !self.plugins[i].state.active {
                    let id = self.plugins[i].state.manifest.id.to_string();
                    self.tell(&format!(
                        "/{} belongs to {id}, which is not active here",
                        c.verb
                    ));
                    continue;
                }
                self.plugins[i].state.activation_until = self.now + actions::ACTIVATION_SECS;
                events.push((
                    Target::One(i),
                    Event::new(
                        "command",
                        Value::map()
                            .with("name", c.verb.as_str())
                            .with("args", c.args.as_str())
                            .with(
                                "words",
                                Value::List(
                                    c.words.iter().map(|w| Value::from(w.as_str())).collect(),
                                ),
                            ),
                    ),
                ));
            }
        }
        self.shared.service_index = self
            .plugins
            .iter()
            .filter(|s| s.state.status == Status::Running && s.state.active)
            .flat_map(|s| {
                let id = s.state.manifest.id.to_string();
                s.state
                    .services
                    .iter()
                    .map(move |(n, a)| (id.clone(), n.clone(), a.clone()))
            })
            .collect();
        events.extend(self.service_events());
        let keys = self.hotkeys();
        for (plugin, node) in std::mem::take(&mut self.hotkeys_pressed) {
            let Some(h) = keys.iter().find(|h| h.plugin == plugin && h.node == node) else {
                continue;
            };
            let line = h.command.trim().trim_start_matches(['/', '@']);
            let (verb, args) = match line.find([' ', '\t']) {
                Some(i) => (&line[..i], line[i..].trim()),
                None => (line, ""),
            };
            let verb = verb.to_ascii_lowercase();
            let Some(i) = self.index(&plugin) else {
                continue;
            };
            if !self.plugins[i].state.commands.contains_key(&verb) {
                self.tell(&format!(
                    "{plugin}'s hotkey runs /{verb}, which it has not claimed"
                ));
                continue;
            }
            self.plugins[i].state.activation_until = self.now + actions::ACTIVATION_SECS;
            let words = dereth_client_model::cmd::interp::find_all_words(args);
            events.push((
                Target::One(i),
                Event::new(
                    "command",
                    Value::map()
                        .with("name", verb.as_str())
                        .with("args", args)
                        .with(
                            "words",
                            Value::List(words.iter().map(|w| Value::from(w.as_str())).collect()),
                        ),
                ),
            ));
        }
        for (plugin, verb) in std::mem::take(&mut self.commands_run) {
            let Some(i) = self.index(&plugin) else {
                continue;
            };
            if !self.plugins[i].state.commands.contains_key(&verb) {
                continue;
            }
            self.plugins[i].state.activation_until = self.now + actions::ACTIVATION_SECS;
            events.push((
                Target::One(i),
                Event::new(
                    "command",
                    Value::map()
                        .with("name", verb.as_str())
                        .with("args", "")
                        .with("words", Value::List(Vec::new())),
                ),
            ));
        }
        if std::mem::take(&mut self.policy_changed) {
            events.push((Target::All, self.policy_event()));
        }
        events.extend(self.peer_events());
        while let Some((id, e)) = self.ui_inbox.pop_front() {
            match id {
                Some(id) => {
                    if let Some(i) = self.index(&id) {
                        // Input on the plugin's own window is a gesture.
                        self.plugins[i].state.activation_until =
                            self.now + actions::ACTIVATION_SECS;
                        events.push((Target::One(i), e));
                    }
                }
                // A press on the game's map reaches every plugin listening, and is no gesture.
                None => events.push((Target::All, e)),
            }
        }
        events.extend(self.due_timers(frame.local_time));

        // What waited from earlier frames first, then this frame's.
        self.drain_backlogs(frame);
        for (target, e) in &events {
            self.dispatch(*target, e, frame);
        }
        let attacking = {
            let c = &frame.world.combat;
            c.repeat_attacking
                || c.attack_in_progress
                || c.attack_server_response_pending
                || c.build_in_progress
        };
        let released_attack =
            std::mem::take(&mut self.shared.abort_attack_if_attacking) && attacking;
        // An attack whose target stopped being selected: its swing must not land elsewhere.
        let misdirected = self.shared.actions.take_abort_swing();
        if std::mem::take(&mut self.shared.abort_attack) || released_attack || misdirected {
            frame.request(dereth_client_contract::UiRequest::CombatAbortAttack);
        }
        self.shared.actions.start(frame);
        if let Some(s) = self.shared.nav.step(frame) {
            self.shared.actions.settle_later(s);
        }
        self.shared.nav.flush_travel(frame);
        for text in std::mem::take(&mut self.shared.chat_out) {
            // Sent on the player's behalf: the client never takes it for a line they typed.
            frame.request(dereth_client_contract::UiRequest::ChatLineOnBehalf { text, window: 0 });
        }
        for r in self.shared.session.take_requests() {
            frame.session_request(r);
        }
        for m in std::mem::take(&mut self.shared.peers_out) {
            self.peers.send(&m);
        }
        for (id, r) in std::mem::take(&mut self.shared.net_out) {
            self.net.start(id, r);
        }
        self.resolve_overlays(frame);

        if self
            .last_flush
            .is_none_or(|t| frame.local_time - t >= FLUSH_INTERVAL)
        {
            if self.last_flush.is_some() {
                self.flush();
            }
            self.last_flush = Some(frame.local_time);
        }

        for i in std::mem::take(&mut self.shared.hidden) {
            frame.hide_chat(i);
        }
        for line in std::mem::take(&mut self.shared.printed) {
            frame.print_chat(line);
        }
        self.refresh_panel();
        self.refresh_toasts();
        let asked = std::mem::take(&mut self.shared.import_requests);
        let before = self.import.queue.len();
        let turned = self.import.take(asked, self.now);
        for (r, why) in turned {
            self.shared.actions.settle_later(actions::Settled {
                plugin: r.plugin,
                id: r.id,
                action: "files.import",
                result: Err(why),
            });
        }
        if self.import.release_held(self.now) || self.import.queue.len() != before {
            self.put_import();
        }
        if self.shared.verbs_changed || !self.claimed_once {
            self.shared.verbs_changed = false;
            self.claimed_once = true;
            let mut verbs: Vec<String> = self.shared.verbs.keys().cloned().collect();
            verbs.push(HOST_VERB.to_owned());
            frame.claim_verbs(verbs);
        }
    }

    /// Note when each appraisal arrived (a plugin reads it as `obj.appraised_at`).
    fn note_appraisals(&mut self, frame: &HookFrame<'_>) {
        let a = &frame.world.appraisal;
        if a.delivered == self.shared.appraisals_seen {
            return;
        }
        self.shared.appraisals_seen = a.delivered;
        for id in frame.world.tables.weenies.keys() {
            let d = a.delivery(id);
            if d != 0 && self.shared.appraised_at.get(&id.0).map(|x| x.0) != Some(d) {
                self.shared.appraised_at.insert(id.0, (d, self.now));
            }
        }
    }

    /// Whether a running, active plugin listens to the message stream.
    #[must_use]
    pub fn wants_messages(&self) -> bool {
        self.config.capabilities.contains(&HostCapability::Messages)
            && self.plugins.iter().any(|s| {
                s.state.status == Status::Running
                    && s.state.active
                    && s.state
                        .subscriptions
                        .iter()
                        .any(|e| e.starts_with(messages::PREFIX))
            })
    }

    /// This frame's messages as events, each under its own name and again as `messages.any`.
    fn message_events(&self, frame: &HookFrame<'_>) -> Vec<(Target, Event)> {
        if frame.messages.is_empty() || !self.wants_messages() {
            return Vec::new();
        }
        let mut out = Vec::new();
        for m in frame.messages {
            if let Some(e) = messages::event_of(m) {
                let any = Event::new(messages::ANY, e.data.clone());
                out.push((Target::All, e));
                out.push((Target::All, any));
            }
        }
        out
    }

    /// The service calls made last frame, each to its provider as a `service.call` event (or
    /// failed at once when there is no such service it may call); and the calls unanswered past
    /// their time, failed as `timeout`.
    fn service_events(&mut self) -> Vec<(Target, Event)> {
        let now = self.now;
        let late: Vec<i64> = self
            .shared
            .service_pending
            .iter()
            .filter(|(_, p)| p.deadline <= now)
            .map(|(id, _)| *id)
            .collect();
        for id in late {
            if let Some(p) = self.shared.service_pending.remove(&id) {
                self.shared.actions.settle_later(actions::Settled {
                    plugin: p.caller,
                    id,
                    action: "services.call",
                    result: Err("timeout".to_owned()),
                });
            }
        }
        let mut out = Vec::new();
        for c in std::mem::take(&mut self.shared.service_calls) {
            let provider = self.index(&c.provider).filter(|&i| {
                let st = &self.plugins[i].state;
                st.status == Status::Running && st.active
            });
            let allowed = provider
                .and_then(|i| self.plugins[i].state.services.get(&c.service))
                .map(|allow| allow.as_ref().is_none_or(|a| a.contains(&c.caller)));
            let refusal = match (provider, allowed) {
                (None, _) => Some("no_plugin"),
                (_, None) => Some("no_service"),
                (_, Some(false)) => Some("not_allowed"),
                _ => None,
            };
            if let Some(why) = refusal {
                self.shared.actions.settle_later(actions::Settled {
                    plugin: c.caller,
                    id: c.id,
                    action: "services.call",
                    result: Err(why.to_owned()),
                });
                continue;
            }
            let i = provider.expect("checked above");
            self.shared.service_pending.insert(
                c.id,
                services::Pending {
                    caller: c.caller.clone(),
                    provider: c.provider.clone(),
                    deadline: now + services::REPLY_SECS,
                },
            );
            out.push((
                Target::One(i),
                Event::new(
                    "service.call",
                    Value::map()
                        .with("id", c.id)
                        .with("from", c.caller.as_str())
                        .with("service", c.service.as_str())
                        .with("args", c.args)
                        .with("gesture", c.gesture),
                ),
            ));
        }
        out
    }

    /// Take down toasts whose time is up, and build the toasts' window again when they changed: a
    /// HUD at the top of the screen, a line a toast, each with its plugin's name.
    fn refresh_toasts(&mut self) {
        let now = self.now;
        // Where no plugin window is drawn, each toast is a line in the chat, once.
        let new: Vec<String> = self
            .shared
            .toasts
            .iter()
            .filter(|t| !self.toasts_told.contains(t))
            .map(|(name, text, _)| format!("{name}: {text}"))
            .collect();
        self.toasts_told.clone_from(&self.shared.toasts);
        if !self.windows_shown {
            for line in new {
                self.tell(&line);
            }
        }
        let before = self.shared.toasts.len();
        self.shared.toasts.retain(|t| t.2 > now);
        let changed =
            std::mem::take(&mut self.shared.toasts_changed) || self.shared.toasts.len() != before;
        if !changed {
            return;
        }
        if self.shared.toasts.is_empty() {
            self.toasts = None;
            return;
        }
        let node = |kind: &str| Value::map().with("kind", kind);
        let lines: Vec<Value> = self
            .shared
            .toasts
            .iter()
            .map(|(name, text, _)| node("label").with("text", format!("{name}: {text}")))
            .collect();
        let v = node("hud")
            .with("id", TOASTS)
            .with("anchor", "top")
            .with("children", lines);
        let Ok(tree) = dereth_plugin_api::UiNode::window_from_value(&v) else {
            return;
        };
        let revision = self.toasts.as_ref().map_or(1, |w| w.revision + 1);
        self.toasts = Some(Window {
            tree,
            open: true,
            revision,
            size: None,
            by_player: false,
        });
    }

    /// The policy in force, as plugins hear it.
    fn policy_event(&self) -> Event {
        let p = &self.config.policy;
        let mut rules = Value::map();
        for d in AutoDomain::ALL {
            rules = rules.with(d.name(), p.rule(d).name());
        }
        Event::new(
            "session.world_policy",
            Value::map()
                .with("published", p.published)
                .with("rules", rules.with("session", p.session.name())),
        )
    }

    /// Messages from the other clients, each to the running plugins of the id that sent it which
    /// may talk to peers, within the world's cap on how many clients a plugin coordinates.
    fn peer_events(&mut self) -> Vec<(Target, Event)> {
        let cap = self.config.policy.peers_local;
        let mut out = Vec::new();
        for m in self.peers.receive() {
            let Some(i) = self.index(&m.plugin) else {
                continue;
            };
            let st = &mut self.plugins[i].state;
            if st.status != Status::Running {
                continue;
            }
            if !st.peers_heard.contains(&m.from) {
                let full = cap.is_some_and(|c| st.peers_heard.len() >= c as usize);
                if full {
                    continue;
                }
                st.peers_heard.insert(m.from.clone());
            }
            let data = Value::from_json(&m.data).unwrap_or_default();
            out.push((
                Target::One(i),
                Event::new(
                    "peers.message",
                    Value::map()
                        .with("from", m.from.as_str())
                        .with("channel", m.channel.as_str())
                        .with("data", data),
                ),
            ));
        }
        out
    }

    /// The HTTPS requests that ended, as settlements for the plugins that made them.
    fn net_answers(&mut self) -> Vec<actions::Settled> {
        let mut out = Vec::new();
        for (id, result) in self.net.poll() {
            let Some((plugin, image)) = self.shared.net_pending.remove(&id) else {
                continue;
            };
            let Some(i) = self.index(&plugin) else {
                continue;
            };
            let st = &mut self.plugins[i].state;
            st.net_in_flight = st.net_in_flight.saturating_sub(1);
            let result = match result {
                Err(e) => Err(e),
                Ok(r) if r.body.len() > net::MAX_BODY => {
                    Err(format!("the answer is over {} bytes", net::MAX_BODY))
                }
                Ok(r) if image => {
                    if r.body.starts_with(b"\x89PNG\r\n\x1a\n") {
                        self.shared.next_image += 1;
                        let name = format!("net:{}", self.shared.next_image);
                        // Looked at once as it arrives: kept within its bounds, hashed now.
                        match st.pictures.take(&name, &r.body) {
                            Ok(()) => {
                                st.images.insert(name.clone(), r.body);
                                Ok(Value::from(name))
                            }
                            Err(why) => Err(why),
                        }
                    } else {
                        Err("the answer is not a PNG image".to_owned())
                    }
                }
                Ok(r) => Ok(Value::map()
                    .with("status", i64::from(r.status))
                    .with("type", r.content_type)
                    .with("body", String::from_utf8_lossy(&r.body).into_owned())),
            };
            out.push(actions::Settled {
                plugin,
                id,
                action: "net.fetch",
                result,
            });
        }
        out
    }

    /// Drop markers whose time is up, then resolve every marker against this frame's places.
    fn resolve_overlays(&mut self, frame: &HookFrame<'_>) {
        let now = self.now;
        for s in &mut self.plugins {
            s.state
                .markers
                .retain(|_, m| m.until.is_none_or(|u| u > now));
        }
        self.overlays = Overlays::resolve(
            frame.places,
            frame.world.player,
            self.plugins
                .iter()
                .filter(|s| s.state.status == Status::Running && s.state.active)
                .map(|s| (s.state.manifest.id.as_str(), &s.state.markers)),
        );
        // The nametags layers, one tag per object of the kinds each names within its range, the
        // nearest first and no more than its limit.
        let me = frame.places.player();
        for s in &self.plugins {
            if s.state.status != Status::Running || !s.state.active {
                continue;
            }
            for (&id, m) in &s.state.markers {
                let Some(n) = m.nametags else { continue };
                let mut tags: Vec<(f64, overlay::OverlayView)> = Vec::new();
                for (oid, w) in frame.world.tables.weenies.iter() {
                    if Some(oid) == frame.world.player {
                        continue;
                    }
                    let (class, _) = crate::reads::class_of(w);
                    let wanted = match class {
                        24 => n.players,
                        5 => n.monsters,
                        37 | 25 => n.npcs,
                        14 => n.portals,
                        _ => false,
                    };
                    if !wanted {
                        continue;
                    }
                    let Some(at) = frame.places.object(oid) else {
                        continue;
                    };
                    let Some(distance) = me.map(|p| places::distance(&p, &at)) else {
                        continue;
                    };
                    if distance > n.within {
                        continue;
                    }
                    let mut label = w.pwd.name.clone();
                    if n.health {
                        if let Some(f) = reads::health(frame.world, frame.view, oid)
                            .get("fraction")
                            .as_f64()
                        {
                            label = format!("{label} {:.0}%", f * 100.0);
                        }
                    }
                    let mut at = at;
                    at.frame.origin.z += 2.2;
                    tags.push((
                        distance,
                        overlay::OverlayView {
                            plugin: s.state.manifest.id.to_string(),
                            id,
                            kind: overlay::Kind::Nametag,
                            label,
                            color: m.color,
                            icon: None,
                            object: Some(oid),
                            at: Some(at),
                            points: Vec::new(),
                            line: false,
                            shape: None,
                            path: Vec::new(),
                            width: 1,
                            on_top: m.on_top,
                            reach: Some(n.within),
                        },
                    ));
                }
                tags.sort_by(|a, b| a.0.total_cmp(&b.0));
                self.overlays
                    .markers
                    .extend(tags.into_iter().take(n.limit).map(|(_, v)| v));
            }
        }
        self.journey_overlays(frame);
    }

    /// The journey a plugin shows, drawn as its markers: an arrow to where the leg being carried
    /// out goes, the way still to go on the map, and each portal and recall ahead as a point on
    /// the map and a blip on the radar with the portal's or the spell's icon.
    fn journey_overlays(&mut self, frame: &HookFrame<'_>) {
        use dereth_client_runtime::travel::TravelLeg;
        /// The colour a journey is drawn in.
        const JOURNEY: u32 = 0x40_C0_FF;
        let Some((plugin, travel)) = self.shared.nav.shown_journey() else {
            return;
        };
        let Some(trip) = frame.travel.trip.as_ref().filter(|t| t.id == travel) else {
            return;
        };
        let Some(slot) = self.plugins.iter().find(|s| {
            s.state.manifest.id.as_str() == plugin
                && s.state.status == Status::Running
                && s.state.active
        }) else {
            return;
        };
        let plugin = slot.state.manifest.id.to_string();
        let can = |k: overlay::Kind| self.config.capabilities.contains(&k.capability());
        let view = |id: i64,
                    kind: overlay::Kind,
                    label: String,
                    at: Option<dereth_primitives::Position>| {
            overlay::OverlayView {
                plugin: plugin.clone(),
                id,
                kind,
                label,
                color: JOURNEY,
                icon: None,
                object: None,
                at,
                points: Vec::new(),
                line: false,
                shape: None,
                path: Vec::new(),
                width: 2,
                on_top: false,
                reach: None,
            }
        };
        let label = |leg: &TravelLeg| match leg {
            TravelLeg::Walk { .. } => String::from("Walk"),
            TravelLeg::Door { .. } => String::from("Door"),
            TravelLeg::Portal { name, .. } => format!("Portal: {name}"),
            TravelLeg::Recall { name, .. } => format!("Recall: {name}"),
            TravelLeg::HandBack { name, .. } => format!("Hand back: {name}"),
        };
        let mut out = Vec::new();
        if can(overlay::Kind::Arrow) {
            if let Some(target) = trip.target {
                let current = trip.legs.get(trip.leg).map_or_else(String::new, label);
                out.push(view(-1, overlay::Kind::Arrow, current, Some(target)));
            }
        }
        if can(overlay::Kind::Map) && !trip.route.is_empty() {
            let mut line = view(-2, overlay::Kind::Map, String::from("Journey"), None);
            line.points = frame
                .places
                .player()
                .into_iter()
                .chain(trip.route.iter().copied())
                .collect();
            line.line = true;
            out.push(line);
        }
        let mut next_id = -3;
        for leg in trip.legs.iter().skip(trip.leg) {
            let (at, icon) = match leg {
                TravelLeg::Portal { from, wcid, .. } => (
                    *from,
                    wcid.and_then(|w| {
                        frame
                            .world
                            .tables
                            .weenies
                            .iter()
                            .find(|(_, x)| x.pwd.wcid == w)
                            .map(|(_, x)| x.pwd.icon_id)
                    }),
                ),
                TravelLeg::Recall { to, recall, .. } => (
                    *to,
                    match recall {
                        dereth_navigation::Recall::Spell(s) => frame
                            .world
                            .magic
                            .spell_table
                            .as_deref()
                            .and_then(|t| t.spells.get(s))
                            .map(|b| b.icon),
                        _ => None,
                    },
                ),
                _ => continue,
            };
            for kind in [overlay::Kind::Map, overlay::Kind::Radar] {
                if can(kind) {
                    let mut v = view(next_id, kind, label(leg), Some(at));
                    v.icon = icon.filter(|i| *i != 0);
                    if kind == overlay::Kind::Map {
                        v.points = vec![at];
                    }
                    out.push(v);
                    next_id -= 1;
                }
            }
        }
        self.overlays.markers.extend(out);
    }

    fn due_timers(&mut self, now: f64) -> Vec<(Target, Event)> {
        let mut due: Vec<(f64, usize, i64)> = Vec::new();
        for (i, s) in self.plugins.iter_mut().enumerate() {
            if s.state.status != Status::Running || !s.state.active {
                continue;
            }
            let mut fired = Vec::new();
            for (&id, t) in &mut s.state.timers {
                if t.due <= now {
                    due.push((t.due, i, id));
                    match t.every {
                        // At most one catch-up: a stalled repeating timer fires once and resumes
                        // its period from now.
                        Some(every) => {
                            t.due = if t.due + every <= now {
                                now + every
                            } else {
                                t.due + every
                            }
                        }
                        None => fired.push(id),
                    }
                }
            }
            for id in fired {
                s.state.timers.remove(&id);
            }
        }
        due.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
        due.into_iter()
            .map(|(_, i, id)| {
                (
                    Target::One(i),
                    Event::new("task.timer", Value::map().with("id", id)),
                )
            })
            .collect()
    }

    fn dispatch(&mut self, target: Target, e: &Event, frame: &HookFrame<'_>) {
        let internal = matches!(
            e.name.as_str(),
            "command" | "task.timer" | "action.completed" | "action.failed" | "service.call"
        );
        // An inactive plugin hears its life and the session's, and its promises settle.
        let heard_inactive = dereth_plugin_api::event::LIFECYCLE.contains(&e.name.as_str())
            || matches!(e.name.as_str(), "action.completed" | "action.failed");
        for i in 0..self.plugins.len() {
            let wanted = match target {
                Target::One(j) => i == j,
                Target::All => true,
            };
            if !wanted || !(internal || self.plugins[i].state.subscriptions.contains(&e.name)) {
                continue;
            }
            if !self.plugins[i].state.active && !heard_inactive {
                continue;
            }
            if self.plugins[i].state.status != Status::Running {
                continue;
            }
            // Past its share of the frame, or the plugins past theirs, or with events still
            // waiting before this one: it waits for a later frame.
            if !self.plugins[i].state.backlog.is_empty() || !self.may_enter(i) {
                self.hold(i, e.clone());
                continue;
            }
            self.shared.answering = answers_the_player(e);
            let _ = self.enter(i, Some(frame), |vm, call| vm.deliver(call, e));
            self.shared.answering = false;
        }
    }

    /// A plugin's share of one frame, and all plugins' together.
    fn frame_budgets(&self) -> (web_time::Duration, web_time::Duration) {
        let entry = self.config.limits.call_budget_ms.max(0.0) / 1000.0;
        (
            web_time::Duration::from_secs_f64(entry * PLUGIN_FRAME_BUDGETS),
            web_time::Duration::from_secs_f64(entry * FRAME_BUDGETS),
        )
    }

    /// Whether plugin `i` may enter its VM again this frame: neither it nor the plugins together
    /// are past their share of it.
    fn may_enter(&self, i: usize) -> bool {
        let (mine, all) = self.frame_budgets();
        self.plugins[i].state.frame_spent < mine && self.frame_spent < all
    }

    /// Keep `e` for plugin `i` for a later frame; past [`MAX_BACKLOG`] the oldest goes, and the
    /// log says so once.
    fn hold(&mut self, i: usize, e: Event) {
        let st = &mut self.plugins[i].state;
        if st.backlog.len() >= MAX_BACKLOG {
            st.backlog.pop_front();
            if !std::mem::replace(&mut st.backlog_dropped, true) {
                st.warn(format!(
                    "more than {MAX_BACKLOG} events waited for a later frame; the oldest were let go"
                ));
            }
        }
        st.backlog.push_back(e);
    }

    /// Deliver the events that waited from earlier frames, each plugin's in order, while the frame
    /// has time for them.
    fn drain_backlogs(&mut self, frame: &HookFrame<'_>) {
        for i in 0..self.plugins.len() {
            while self.plugins[i].state.status == Status::Running && self.may_enter(i) {
                let Some(e) = self.plugins[i].state.backlog.pop_front() else {
                    break;
                };
                self.shared.answering = answers_the_player(&e);
                let _ = self.enter(i, Some(frame), |vm, call| vm.deliver(call, &e));
                self.shared.answering = false;
            }
            if self.plugins[i].state.backlog.is_empty() {
                self.plugins[i].state.backlog_dropped = false;
            }
        }
    }

    /// Run one entry into plugin `i`'s VM, and account for a failure.
    fn enter<R>(
        &mut self,
        i: usize,
        frame: Option<&HookFrame<'_>>,
        body: impl FnOnce(&mut Box<dyn ScriptInstance>, &mut Call<'_, '_>) -> Result<R, ScriptError>,
    ) -> Option<R> {
        let (mine, _) = self.frame_budgets();
        let (outcome, suspended) = {
            let Self {
                plugins,
                shared,
                config,
                now,
                files,
                game_data,
                frame_spent,
                vm_clock,
                loading,
                entry_chunk,
                ..
            } = self;
            let slot = &mut plugins[i];
            if slot.state.status != Status::Running {
                return None;
            }
            let mut vm = slot.vm.take()?;
            let mut call = Call {
                slot: &mut slot.state,
                frame,
                shared,
                config,
                prints: slot.prints,
                chat_lines: frame.map_or(0, |f| f.chat.len()),
                files: files.as_mut(),
                data: game_data.as_ref(),
            };
            let started = vm_clock.now();
            let result = body(&mut vm, &mut call);
            let took = vm_clock.now().saturating_sub(started);
            slot.prints = call.prints;
            slot.vm = Some(vm);
            let st = &mut slot.state;
            // The entry's time, the host's answers to its calls included, is the plugin's share
            // of the frame.
            let before = st.frame_spent;
            st.frame_spent += took;
            *frame_spent += took;
            if before < mine && st.frame_spent >= mine && !*loading {
                st.frame_overruns.push_back(*now);
                tracing::debug!(target: "dereth::plugin", "{}: over its share of a frame ({:.1} ms); what is still due waits for the next",
                    st.manifest.id, mine.as_secs_f64() * 1000.0);
            }
            let outcome = match result {
                Ok(r) => Some(r),
                Err(err) => {
                    let text = err.to_string();
                    // Told in the chat, one line, the traceback in the log; a failure of the entry
                    // chunk is told as the plugin's failure to load.
                    if !*entry_chunk {
                        let budget = err.kind == ScriptErrorKind::Budget;
                        shared.printed.push(error_line(&st.manifest, &text, budget));
                    }
                    st.log(text);
                    if err.kind == ScriptErrorKind::Budget {
                        st.overruns.push_back(*now);
                    } else {
                        st.errors.push_back(*now);
                    }
                    None
                }
            };
            while st.errors.front().is_some_and(|t| *now - t > FAULT_WINDOW) {
                st.errors.pop_front();
            }
            while st.overruns.front().is_some_and(|t| *now - t > FAULT_WINDOW) {
                st.overruns.pop_front();
            }
            while st
                .frame_overruns
                .front()
                .is_some_and(|t| *now - t > FAULT_WINDOW)
            {
                st.frame_overruns.pop_front();
            }
            let reason = if st.errors.len() >= MAX_ERRORS {
                Some(format!("{MAX_ERRORS} errors within a minute"))
            } else if st.overruns.len() >= MAX_OVERRUNS {
                Some(format!(
                    "over its time budget {MAX_OVERRUNS} times within a minute"
                ))
            } else if st.frame_overruns.len() >= MAX_FRAME_OVERRUNS {
                Some(format!(
                    "over its share of a frame {MAX_FRAME_OVERRUNS} times within a minute"
                ))
            } else {
                None
            };
            let suspended = reason.map(|reason| {
                st.status = Status::Suspended(reason.clone());
                st.backlog.clear();
                st.log(format!("suspended: {reason}"));
                (st.manifest.id.to_string(), reason)
            });
            (outcome, suspended)
        };
        if let Some((id, reason)) = suspended {
            self.release(&id);
            self.tell(&format!(
                "{id} was suspended ({reason}). /plugin reload {id} starts it again."
            ));
            self.host_log(format!("{id} suspended: {reason}"));
        }
        outcome
    }

    // ---------------------------------------------------------------------------------------------
    // Load, unload, reload
    // ---------------------------------------------------------------------------------------------

    /// Refuse to load `what`, telling the player why. Answers nothing: nothing loaded.
    fn refuse(&mut self, what: &str, why: &str) -> Option<String> {
        self.host_log(format!("refused {what}: {why}"));
        self.tell(&format!("{what} was not loaded: {why}"));
        None
    }

    /// Load a package, if it passes the checks; answers the plugin's name as the chat tells it
    /// when it loaded and runs.
    fn load(
        &mut self,
        package: Package,
        frame: Option<&HookFrame<'_>>,
        state: Option<Value>,
    ) -> Option<String> {
        let origin = package.origin.clone();
        let m = match package.manifest() {
            Ok(m) => m,
            Err(e) => return self.refuse(&origin, &e.to_string()),
        };
        let what = m.id.to_string();
        if !ApiVersion::CURRENT.runs(m.api) {
            return self.refuse(
                &what,
                &format!(
                    "it needs plugin API {}, and this host has {}",
                    m.api,
                    ApiVersion::CURRENT
                ),
            );
        }
        let missing: Vec<String> = m
            .required
            .iter()
            .filter(|c| !self.config.capabilities.contains(c))
            .map(ToString::to_string)
            .collect();
        if !missing.is_empty() {
            return self.refuse(
                &what,
                &format!(
                    "this host ({}) has no {}",
                    self.config.name,
                    missing.join(", ")
                ),
            );
        }
        if m.exclude_hosts.contains(&self.config.name) {
            return self.refuse(&what, &format!("it does not run in {}", self.config.name));
        }
        if self.index(m.id.as_str()).is_some() {
            return self.refuse(&what, "a plugin with that id is already loaded");
        }
        self.load_checked(package, frame, state)
    }

    /// Load a package that passed the checks; answers as [`Self::load`].
    fn load_checked(
        &mut self,
        package: Package,
        frame: Option<&HookFrame<'_>>,
        state: Option<Value>,
    ) -> Option<String> {
        let origin = package.origin.clone();
        let m = match package.manifest() {
            Ok(m) => m,
            Err(e) => return self.refuse(&origin, &e.to_string()),
        };
        let what = m.id.to_string();
        if self.index(m.id.as_str()).is_some() {
            return self.refuse(&what, "a plugin with that id is already loaded");
        }
        let entry = m.entry.clone();
        let Some(engine) = self.engines.iter().find(|e| e.handles(&entry)) else {
            return self.refuse(&what, &format!("no engine here runs its entry {entry}"));
        };
        let vm = match engine.instantiate(self.config.limits) {
            Ok(vm) => vm,
            Err(e) => return self.refuse(&what, &e.to_string()),
        };
        let source = if m.ubscript {
            ubscript_bootstrap(&entry, &m.name)
        } else {
            package.files.get(&entry).cloned().unwrap_or_default()
        };
        let mut state_slot = SlotState::new(m, package, self.now);
        state_slot.reopen = self.carried_open.remove(&what).unwrap_or_default();
        // Its package's pictures, each looked at once: kept within their bounds, hashed now.
        let pngs: Vec<(String, Vec<u8>)> = state_slot
            .package
            .files
            .iter()
            .filter(|(name, _)| name.to_ascii_lowercase().ends_with(".png"))
            .map(|(name, bytes)| (name.clone(), bytes.clone()))
            .collect();
        for (name, bytes) in pngs {
            if let Err(why) = state_slot.pictures.take(&name, &bytes) {
                state_slot.warn(format!("{why}: it is not shown"));
            }
        }
        for key in self.shared.scope_keys() {
            open_scope(self.storage.as_mut(), &mut state_slot, &key);
        }
        self.plugins.push(Slot {
            state: state_slot,
            vm: Some(vm),
            prints: 0,
        });
        let i = self.plugins.len() - 1;
        self.loading = true;
        self.entry_chunk = true;
        let loaded = self.enter(i, frame, |vm, call| vm.load_bytes(call, &entry, &source));
        self.entry_chunk = false;
        self.loading = false;
        let slot = &mut self.plugins[i];
        if loaded.is_none() && slot.state.status == Status::Running {
            let why = slot.state.console.back().cloned().unwrap_or_default();
            slot.state.status = Status::Faulted(why.clone());
            slot.vm = None;
            // What the entry chunk set up before it failed goes with it.
            slot.state.windows.clear();
            slot.state.timers.clear();
            for verb in std::mem::take(&mut slot.state.commands).into_keys() {
                self.shared.verbs.remove(&verb);
                self.shared.verbs_changed = true;
            }
            self.host_log(format!("{what} failed to load: {why}"));
            // The error's first line; the whole of it, its traceback too, is in the log.
            let first = why.lines().next().unwrap_or_default();
            self.tell(&format!(
                "{what} failed to load: {}",
                shown::bounded(first, shown::LINE)
            ));
            return None;
        }
        self.host_log(format!("loaded {what}"));
        if let Some(state) = state {
            let e = Event::new("plugin.reload", state);
            let _ = self.enter(i, frame, |vm, call| vm.deliver(call, &e));
        }
        if self.plugins[i]
            .state
            .subscriptions
            .contains("plugin.loaded")
        {
            let e = Event::new("plugin.loaded", Value::map());
            let _ = self.enter(i, frame, |vm, call| vm.deliver(call, &e));
        }
        Some(told_name(&self.plugins[i].state.manifest))
    }

    /// The part of the session the last frame was in.
    #[must_use]
    pub fn context(&self) -> Option<Context> {
        self.context
    }

    /// Bring each running plugin's activity in line with the frame's context: a plugin whose
    /// contexts hold it becomes active and hears `context.begin`; one that was active and is not
    /// now hears `context.end` and lets go of the character (its queued actions, its attack, its
    /// movement and run lock), as a stopped plugin does.
    fn reconcile_contexts(&mut self, frame: &HookFrame<'_>) {
        let now = context_of(frame);
        let previous = self.context.replace(now);
        // A plugin becoming active in the world finds its character's store open, though the
        // login is only heard later in the frame.
        if now == Context::World && self.shared.character.is_none() {
            if let Some(name) = frame.view.character_name() {
                self.shared.character = Some(name.to_owned());
                self.shared.world = Some(frame.world_name.unwrap_or("unknown").to_owned());
                self.shared.account = frame.account.map(str::to_owned);
                self.open_session_scopes();
                self.load_layout();
            }
        }
        for i in 0..self.plugins.len() {
            let st = &self.plugins[i].state;
            if st.status != Status::Running {
                continue;
            }
            let wants = st.manifest.contexts.contains(&now);
            if st.active == wants {
                continue;
            }
            let id = st.manifest.id.to_string();
            self.plugins[i].state.active = wants;
            let e = if wants {
                let before = previous.filter(|p| *p != now).map(|p| p.name().to_owned());
                self.host_log(format!("{id} is active ({})", now.name()));
                Event::new(
                    "context.begin",
                    Value::map()
                        .with("context", now.name())
                        .with("previous", before),
                )
            } else {
                self.stop_character(&id);
                self.host_log(format!("{id} is inactive ({})", now.name()));
                Event::new(
                    "context.end",
                    Value::map()
                        .with("context", previous.unwrap_or(now).name())
                        .with("next", now.name()),
                )
            };
            if self.plugins[i].state.subscriptions.contains(&e.name) {
                // Becoming active, a plugin sets itself up as it does loading: it is not held to
                // its share of the frame.
                self.loading = wants;
                let _ = self.enter(i, Some(frame), |vm, call| vm.deliver(call, &e));
                self.loading = false;
            }
        }
    }

    /// Stop what `id` (going inactive) holds of the character: its queued and running actions and
    /// its movement fail `stopped`, its attack is cancelled, the run lock it set is let go.
    fn stop_character(&mut self, id: &str) {
        let (stopped, attacking) = self.shared.actions.stop_all(id);
        if attacking {
            self.shared.abort_attack_if_attacking = true;
        }
        for s in stopped {
            self.shared.actions.settle_later(s);
        }
        if let Some(s) = self.shared.nav.stop(id) {
            self.shared.actions.settle_later(s);
        }
    }

    /// Let go of what `id` holds of the character: its queued and running actions (cancelling its
    /// attack) and its movement and run lock, without settling them (it is going away).
    fn release_character(&mut self, id: &str) {
        if self.shared.actions.forget(id) {
            self.shared.abort_attack_if_attacking = true;
        }
        self.shared.nav.forget(id);
    }

    /// Let go of everything `id` holds in the game, because it is unloading or was suspended and
    /// can no longer let go itself: its queued actions, its attack (cancelled), its movement and
    /// the run lock it set, its pre-game requests, its HTTPS requests and its markers.
    fn release(&mut self, id: &str) {
        // Calls waiting on it fail; its own calls are forgotten.
        let waiting: Vec<i64> = self
            .shared
            .service_pending
            .iter()
            .filter(|(_, p)| p.provider == id)
            .map(|(k, _)| *k)
            .collect();
        for k in waiting {
            if let Some(p) = self.shared.service_pending.remove(&k) {
                self.shared.actions.settle_later(actions::Settled {
                    plugin: p.caller,
                    id: k,
                    action: "services.call",
                    result: Err("no_plugin".to_owned()),
                });
            }
        }
        self.shared.service_pending.retain(|_, p| p.caller != id);
        self.shared.service_calls.retain(|c| c.caller != id);
        self.shared.retail_ui.retain(|_, (p, ..)| p != id);
        self.shared.import_requests.retain(|r| r.plugin != id);
        self.import.forget(id);
        let front = self.import.queue.front().map(|r| r.plugin.clone());
        self.import.queue.retain(|r| r.plugin != id);
        if front.as_deref() == Some(id) {
            self.put_import();
        }
        self.release_character(id);
        self.shared.session.forget(id);
        self.shared.net_pending.retain(|_, (p, _)| p != id);
        self.overlays.markers.retain(|m| m.plugin != id);
    }

    /// Unload plugin `i`: its `on_unload` runs, its store is written, its windows, timers and
    /// commands go; answers what `on_unload` returned.
    fn unload(&mut self, i: usize, frame: Option<&HookFrame<'_>>) -> Option<Value> {
        let e = Event::new("plugin.unload", Value::Nil);
        let state = self.enter(i, frame, |vm, call| vm.deliver(call, &e));
        let mut slot = self.plugins.remove(i);
        let id = slot.state.manifest.id.to_string();
        self.release(&id);
        flush_slot(&mut slot.state, self.storage.as_mut());
        for verb in slot.state.commands.keys() {
            self.shared.verbs.remove(verb);
            self.shared.verbs_changed = true;
        }
        let id = slot.state.manifest.id.to_string();
        self.host_log(format!("unloaded {id}"));
        state.filter(|v| !v.is_nil())
    }

    /// Reload plugin `id` (`why`: the player's Reload, or their Enable of a plugin stopped on a
    /// fault), telling the chat once it runs again.
    fn reload(&mut self, id: &str, frame: Option<&HookFrame<'_>>, why: Why) {
        let done = if why == Why::Enable {
            "enabled"
        } else {
            "reloaded"
        };
        let Some(i) = self.index(id) else {
            // Not loaded: a reload loads it, if the source has it.
            match dereth_plugin_api::PluginId::parse(id).and_then(|pid| self.source.fetch(&pid)) {
                Some(p) => {
                    if let Some(name) = self.load(p, frame, None) {
                        self.tell_life(&name, done);
                    }
                }
                None => self.tell(&format!("no plugin {id} is loaded or available")),
            }
            return;
        };
        let old_package = self.plugins[i].state.package.clone();
        // The windows the player had open open again as the new instance declares them.
        let open: BTreeSet<String> = self.plugins[i]
            .state
            .windows
            .iter()
            .filter(|(_, w)| w.open)
            .map(|(k, _)| k.clone())
            .collect();
        let state = self.unload(i, frame);
        let package = self.fetch_any(id).unwrap_or(old_package);
        self.carried_open.insert(id.to_owned(), open);
        let loaded = self.load(package, frame, state);
        self.carried_open.remove(id);
        if let Some(name) = loaded {
            self.tell_life(&name, done);
        }
    }

    fn open_session_scopes(&mut self) {
        let keys = self.shared.scope_keys();
        let Self {
            plugins, storage, ..
        } = self;
        for s in plugins.iter_mut() {
            for key in &keys {
                open_scope(storage.as_mut(), &mut s.state, key);
            }
        }
    }

    // ---------------------------------------------------------------------------------------------
    // The host's command
    // ---------------------------------------------------------------------------------------------

    fn host_command(&mut self, words: &[String], frame: Option<&HookFrame<'_>>) {
        let arg = words.get(1).map(String::as_str);
        match (words.first().map(String::as_str), arg) {
            (None | Some("show"), _) => self.show_panel(),
            (Some("list"), _) => {
                let list = self.plugins();
                let found = self.found();
                let off: Vec<FoundInfo> = found
                    .into_iter()
                    .filter(|b| !list.iter().any(|p| p.id == b.id))
                    .collect();
                if list.is_empty() && off.is_empty() {
                    let none = NO_PLUGINS.to_owned();
                    self.tell(&none);
                }
                for b in off {
                    let state = if b.on { "not loaded" } else { "disabled" };
                    self.tell(&format!(
                        "{} {} {state} (/plugin enable {})",
                        b.id, b.version, b.id
                    ));
                }
                for p in list {
                    let why = p.reason.map(|r| format!(": {r}")).unwrap_or_default();
                    self.tell(&format!("{} {} {}{why}", p.id, p.version, p.status));
                }
            }
            (Some("reload"), Some(id)) => self.reload(id, frame, Why::Reload),
            // `on` and `off` are the words these had before they were Enable and Disable.
            (Some(word @ ("enable" | "disable" | "on" | "off")), Some(id)) => {
                if let Err(why) = self.set_ticked(id, matches!(word, "enable" | "on")) {
                    self.tell(&why);
                }
            }
            (Some("policy"), None) => {
                for line in self.config.policy.describe() {
                    self.tell(&line);
                }
            }
            (Some("huds"), word @ (None | Some("unlock" | "lock"))) => {
                let on = word.map_or(!self.huds_unlocked, |w| w == "unlock");
                self.set_huds_unlocked(on);
                self.tell(if on {
                    "the plugins' HUDs are unlocked: drag one to move it (/plugin huds lock when done)"
                } else {
                    "the plugins' HUDs are locked"
                });
            }
            _ => self.tell(
                "usage: /plugin [show | list | enable <id> | disable <id> | reload <id> | policy | huds [unlock | lock]]",
            ),
        }
    }
}

/// The entry chunk of a script written for UtilityBelt's scripting: the host's library runs the
/// script's own entry file with UtilityBelt's script API as its globals, under the plugin's name.
fn ubscript_bootstrap(entry: &str, name: &str) -> Vec<u8> {
    format!(
        "require(\"dereth.ubscript\").run({}, {})",
        Value::from(entry).to_json(),
        Value::from(name).to_json()
    )
    .into_bytes()
}

fn open_scope(storage: &mut dyn PluginStorage, slot: &mut SlotState, key: &str) {
    if slot.stores.contains_key(key) {
        return;
    }
    let read = |storage: &mut dyn PluginStorage, id: &str| {
        storage
            .load(id, key)
            .and_then(|t| Value::from_json(&t).ok())
            .and_then(|v| v.as_map().cloned())
    };
    // A plugin that replaces an earlier one, with nothing of its own kept here yet, takes the
    // earlier one's store, and keeps it under its own id from then on.
    let mut carried = false;
    let values = read(storage, slot.manifest.id.as_str())
        .or_else(|| {
            let old = slot
                .manifest
                .replaces
                .iter()
                .find_map(|old| read(storage, old.as_str()));
            carried = old.is_some();
            old
        })
        .unwrap_or_default();
    let mut scope = Scope::new(values);
    scope.dirty = carried;
    slot.stores.insert(key.to_owned(), scope);
}

fn flush_slot(slot: &mut SlotState, storage: &mut dyn PluginStorage) {
    let id = slot.manifest.id.to_string();
    for (key, scope) in &mut slot.stores {
        if scope.dirty {
            storage.save(&id, key, &Value::Map(scope.values.clone()).to_json());
            scope.dirty = false;
        }
    }
}

/// The host as the runtime's frame hook.
#[derive(Debug)]
pub struct PluginHook {
    pub host: PluginHost,
}

impl FrameHook for PluginHook {
    fn step(&mut self, frame: &mut HookFrame<'_>) {
        self.host.step(frame);
    }

    fn plugin_list(&self) -> Option<String> {
        Some(audit::list_text(&self.host.audit_entries()))
    }

    fn wants_messages(&self) -> bool {
        self.host.wants_messages()
    }

    fn plugin_query(&self, asked: &str) -> Option<PluginQueryAnswer> {
        let (success, name, author, email, webpage) =
            audit::query_answer(&self.host.audit_entries(), asked);
        Some(PluginQueryAnswer {
            success,
            name,
            author,
            email,
            webpage,
        })
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests;
