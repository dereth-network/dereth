//! Auberean's plugin host: loads plugin packages into VMs, turns the game's events into the events
//! plugins hear and their requests into the client's actions, runs their timers, and keeps their
//! stores, commands, windows and services.
//!
//! **Depends on** the plugin interface (`dereth-plugin-api`) and its packages
//! (`dereth-plugin-package`), which name no game, and on the game as a front end reaches it: the
//! contract's actions and events (`auberean-client-contract`) and the runtime's frame hook
//! (`auberean-client-runtime`). With the `lua` feature, the Luau engine (`dereth-plugin-lua`) for
//! Auberean's bindings ([`bindings`]). **Used by** the products that load plugins, through the
//! client SDK.
//!
//! **Must never** name a crate under `auberean/client/` or a device's input (`cargo xtask seams`,
//! `seam: Auberean plugin host`), do I/O, or read a clock of its own: packages arrive as bytes
//! ([`Package`], [`PluginSource`]), stores leave through a [`PluginStorage`], the time a plugin
//! takes is read from the [`VmClock`] the product gives, and the engine is a [`ScriptEngine`].
//!
//! # A frame
//!
//! The host is installed as the client's frame hook ([`PluginHook`]). Each frame, in order:
//!
//! 1. the loads, unloads and reloads asked for since the last frame are carried out;
//! 2. each plugin no longer active in the session's part hears `context.end`, each now active
//!    `context.begin` ([`Context`]: `world` in the world, `pregame` before it);
//! 3. the frame's game events become plugin events ([`events`]), and after them what the
//!    player's scripts told the panels ([`notices`]), and are delivered, each in the order the
//!    client told them, to every plugin listening, in load order; then the actions that
//!    ended (`action.completed`, `action.failed`, each to its plugin), the commands the player
//!    typed and the presses on plugin windows, the service calls, and the timers that came due;
//! 4. the actions the plugins asked for go to the client, which carries them out as the frame
//!    ends, so what they send goes out with the frame's datagrams.
//!
//! Lines for the player's chat (what the plugins print, and the host's own) wait for the product
//! to take them ([`PluginHost::take_printed`]).
//!
//! # Gestures
//!
//! A press on a plugin's window or a command it claimed is the player's gesture: for
//! [`ACTIVATION_SECS`] of the frame clock after it, what the plugin does follows the gesture, and
//! a window it shows opens. A window opens otherwise only when the player opens it
//! ([`PluginHost::open_window`]).
//!
//! # Faults and time
//!
//! A failing handler's error is told in the chat, one line, and kept in the plugin's record. Ten
//! failures within a minute, three entries stopped by their time budget within a minute, or thirty
//! frames past its share within a minute suspend the plugin; the player is told, and
//! `/plugin reload <id>` starts it again. A plugin's entries in one frame together have a share of
//! it ([`PLUGIN_FRAME_BUDGETS`] entries' budgets), and all plugins' together a larger one
//! ([`FRAME_BUDGETS`]); past either, the events still due for a plugin wait, in order, for the next
//! frame.

#![forbid(unsafe_code)]

pub mod bindings;
pub mod calls;
pub mod events;
pub mod notices;
pub mod ops;
pub mod package;
pub mod reads;
pub mod slot;
pub mod storage;

use core::time::Duration;
use std::any::Any;
use std::collections::{BTreeMap, VecDeque};

use auberean_client_contract::event::{GameEvent, SessionEvent};
use auberean_client_contract::orbit::OrbitSettings;
use auberean_client_contract::session::Phase;
use auberean_client_contract::view::View;
use auberean_client_contract::Action;
use auberean_client_runtime::frame_hook::{FrameHook, HookFrame};
pub use dereth_plugin_api::VmClock;
use dereth_plugin_api::{
    ApiVersion, Context, Event, Game, HostCapability, Limits, ScriptEngine, ScriptError,
    ScriptErrorKind, ScriptInstance, Value,
};

pub use crate::calls::Line;
use crate::calls::{Call, Settled, Shared};
pub use crate::package::{Package, PackageError, PluginSource, StaticSource};
use crate::slot::{Scope, SlotState, Status, Window};
pub use crate::storage::{MemoryFiles, MemoryStorage, NoFiles, PluginFiles, PluginStorage};

/// The host's own command verb: `/plugin`.
pub const HOST_VERB: &str = "plugin";

/// How long, in seconds of the frame clock, the player's gesture on a plugin's surface covers what
/// the plugin does.
pub const ACTIVATION_SECS: f64 = 10.0;

/// A plugin's share of one frame, in entries' budgets.
pub const PLUGIN_FRAME_BUDGETS: f64 = 2.0;

/// All plugins' share of one frame together, in entries' budgets.
pub const FRAME_BUDGETS: f64 = 3.0;

/// The span, in seconds, the fault ceilings count over.
const FAULT_WINDOW: f64 = 60.0;
const MAX_ERRORS: usize = 10;
const MAX_OVERRUNS: usize = 3;
const MAX_FRAME_OVERRUNS: usize = 30;
/// The most events one plugin's backlog holds; past it the oldest go.
const MAX_BACKLOG: usize = 1000;
/// How often, in seconds, changed stores are written.
const FLUSH_INTERVAL: f64 = 60.0;

/// What the host offers and holds plugins to.
#[derive(Debug, Clone)]
pub struct HostConfig {
    /// The host's name, as plugins read it (`host.name`) and manifests exclude it by.
    pub name: String,
    /// The product's version.
    pub version: String,
    /// The capabilities it offers.
    pub capabilities: Vec<HostCapability>,
    /// Each VM's memory and each entry's time.
    pub limits: Limits,
    /// The most bytes one store scope may hold, as JSON.
    pub store_quota: usize,
}

impl HostConfig {
    /// A host named `name` at `version`, offering the plugin core, windows and HUDs and the session
    /// before the world, with the spec's limits.
    #[must_use]
    pub fn new(name: &str, version: &str) -> Self {
        Self {
            name: name.to_owned(),
            version: version.to_owned(),
            capabilities: vec![
                HostCapability::Core,
                HostCapability::UiWindow,
                HostCapability::UiHud,
                HostCapability::SessionPregame,
            ],
            limits: Limits::default(),
            store_quota: 16 << 20,
        }
    }
}

/// A loaded plugin, as the plugin list shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    /// `running`, `faulted` or `suspended`.
    pub status: &'static str,
    /// Why it is not running, when it is not.
    pub why: Option<String>,
    /// Whether it is active in the part of the session the last frame was in.
    pub active: bool,
}

/// A command claimed by a plugin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    pub verb: String,
    pub plugin: String,
    pub help: String,
}

#[derive(Debug)]
enum Request {
    Load(Package),
    Unload(String),
    Reload(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Target {
    One(usize),
    All,
}

/// An event the product passed in for one plugin (a command, a press on its window), and whether
/// it is the player's gesture.
#[derive(Debug)]
struct Input {
    plugin: String,
    event: Event,
}

/// An action carried out for a plugin that ends when the session reaches a phase.
#[derive(Debug)]
struct Pending {
    plugin: String,
    id: i64,
    action: String,
    until: Phase,
}

/// A service call delivered and not yet answered: who asked.
#[derive(Debug)]
struct ServicePending {
    caller: String,
    provider: String,
}

struct Slot {
    state: SlotState,
    vm: Option<Box<dyn ScriptInstance>>,
}

impl core::fmt::Debug for Slot {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Slot")
            .field("id", &self.state.manifest.id)
            .field("status", &self.state.status)
            .finish_non_exhaustive()
    }
}

/// The part of the session a phase is: the world, or before it.
#[must_use]
pub const fn context_of(phase: Phase) -> Context {
    match phase {
        Phase::World => Context::World,
        _ => Context::Pregame,
    }
}

/// Auberean's plugin host (see the crate documentation).
#[derive(Debug)]
pub struct PluginHost {
    config: HostConfig,
    engines: Vec<Box<dyn ScriptEngine>>,
    source: Option<Box<dyn PluginSource>>,
    storage: Box<dyn PluginStorage>,
    files: Box<dyn PluginFiles>,
    vm_clock: Box<dyn VmClock>,
    plugins: Vec<Slot>,
    shared: Shared,
    requests: VecDeque<Request>,
    inputs: VecDeque<Input>,
    settled: Vec<Settled>,
    pending: Vec<Pending>,
    service_calls: Vec<calls::ServiceCall>,
    service_pending: BTreeMap<i64, ServicePending>,
    context: Option<Context>,
    frame_spent: Duration,
    last_flush: f64,
    /// An entry file is being run, which is told as the plugin's failure to load.
    entry_chunk: bool,
    /// A plugin is setting itself up: not held to its share of the frame.
    loading: bool,
}

impl PluginHost {
    /// A host with no engine yet, timing plugins' entries on `vm_clock` and keeping their stores
    /// in `storage`.
    #[must_use]
    pub fn new(
        config: HostConfig,
        vm_clock: Box<dyn VmClock>,
        storage: Box<dyn PluginStorage>,
    ) -> Self {
        Self {
            config,
            engines: Vec::new(),
            source: None,
            storage,
            files: Box::new(NoFiles),
            vm_clock,
            plugins: Vec::new(),
            shared: Shared::default(),
            requests: VecDeque::new(),
            inputs: VecDeque::new(),
            settled: Vec::new(),
            pending: Vec::new(),
            service_calls: Vec::new(),
            service_pending: BTreeMap::new(),
            context: None,
            frame_spent: Duration::ZERO,
            last_flush: 0.0,
            entry_chunk: false,
            loading: false,
        }
    }

    /// Run packages whose entry `engine` handles; engines are asked in the order added.
    #[must_use]
    pub fn with_engine(mut self, engine: Box<dyn ScriptEngine>) -> Self {
        self.engines.push(engine);
        self
    }

    /// Where a reload reads a package's current files.
    #[must_use]
    pub fn with_source(mut self, source: Box<dyn PluginSource>) -> Self {
        self.source = Some(source);
        self
    }

    /// The plugins' data folders.
    #[must_use]
    pub fn with_files(mut self, files: Box<dyn PluginFiles>) -> Self {
        self.files = files;
        self
    }

    /// The wall clock at the frame clock's zero, Unix seconds, for `os.time` (without it, a
    /// plugin's wall clock starts at zero with the frame clock).
    #[must_use]
    pub const fn with_wall_clock(mut self, unix_at_zero: f64) -> Self {
        self.shared.unix_at_zero = unix_at_zero;
        self
    }

    /// The host's configuration.
    #[must_use]
    pub const fn config(&self) -> &HostConfig {
        &self.config
    }

    /// The world and the account the session is in, as the product names them: the world's and
    /// the character's store scopes are kept under them.
    pub fn set_session(&mut self, world: Option<String>, account: Option<String>) {
        self.shared.world = world;
        self.shared.account = account;
    }

    /// The product entered the world as `character` (one of the listed characters, by id): the
    /// character's store scope opens under its name.
    pub fn playing(&mut self, character: u64) {
        self.shared.session.playing = Some(character);
    }

    /// Load a package at the start of the next frame.
    pub fn request_load(&mut self, package: Package) {
        self.requests.push_back(Request::Load(package));
    }

    /// Unload the plugin `id` at the start of the next frame.
    pub fn request_unload(&mut self, id: &str) {
        self.requests.push_back(Request::Unload(id.to_owned()));
    }

    /// Reload the plugin `id` at the start of the next frame, from the source when it has the
    /// package, else from the files it was loaded from.
    pub fn request_reload(&mut self, id: &str) {
        self.requests.push_back(Request::Reload(id.to_owned()));
    }

    /// The loaded plugins, in load order.
    #[must_use]
    pub fn plugins(&self) -> Vec<PluginInfo> {
        self.plugins
            .iter()
            .map(|s| PluginInfo {
                id: s.state.manifest.id.to_string(),
                name: s.state.manifest.name.clone(),
                version: s.state.manifest.version.to_string(),
                status: s.state.status.word(),
                why: s.state.status.why().map(str::to_owned),
                active: s.state.active,
            })
            .collect()
    }

    /// A plugin's record: what it printed and the errors it met, oldest first.
    #[must_use]
    pub fn console(&self, id: &str) -> Option<Vec<String>> {
        self.slot(id)
            .map(|s| s.state.console.iter().cloned().collect())
    }

    /// Every plugin's declared windows and HUDs: the plugin's id, the window's id, the window.
    #[must_use]
    pub fn windows(&self) -> Vec<(String, String, &Window)> {
        self.plugins
            .iter()
            .filter(|s| s.state.status == Status::Running)
            .flat_map(|s| {
                s.state
                    .windows
                    .iter()
                    .map(|(k, w)| (s.state.manifest.id.to_string(), k.clone(), w))
            })
            .collect()
    }

    /// One plugin's window.
    #[must_use]
    pub fn window(&self, plugin: &str, window: &str) -> Option<&Window> {
        self.slot(plugin).and_then(|s| s.state.windows.get(window))
    }

    /// The commands the plugins claimed, by verb.
    #[must_use]
    pub fn commands(&self) -> Vec<Command> {
        self.plugins
            .iter()
            .flat_map(|s| {
                s.state.commands.iter().map(|(verb, help)| Command {
                    verb: verb.clone(),
                    plugin: s.state.manifest.id.to_string(),
                    help: help.clone(),
                })
            })
            .collect()
    }

    /// Lines for the player's chat since they were last taken, in order.
    pub fn take_printed(&mut self) -> Vec<Line> {
        std::mem::take(&mut self.shared.printed)
    }

    /// The camera a plugin chose since this was last asked, the last one when several did: the
    /// orbit camera with its settings, or the game's own (`Some(None)`); `None` when none chose.
    /// The product carries it out as an interface's choice of camera is carried out.
    pub fn take_camera(&mut self) -> Option<Option<OrbitSettings>> {
        self.shared.camera.take()
    }

    /// A line the player typed: the host's own `/plugin`, or a command a plugin claimed, is the
    /// host's (answered, or delivered to its plugin at the next frame), and the answer is `true`;
    /// any other line is not, and the product carries it out as it would.
    pub fn command(&mut self, line: &str) -> bool {
        let Some(rest) = line.strip_prefix('/').or_else(|| line.strip_prefix('@')) else {
            return false;
        };
        let (verb, args) = rest.split_once(' ').unwrap_or((rest, ""));
        let verb = verb.to_ascii_lowercase();
        let words: Vec<String> = args.split_whitespace().map(str::to_owned).collect();
        if verb == HOST_VERB {
            self.host_command(&words);
            return true;
        }
        let Some(plugin) = self.shared.verbs.get(&verb).cloned() else {
            return false;
        };
        let words_v: Vec<Value> = words.into_iter().map(Value::from).collect();
        self.inputs.push_back(Input {
            plugin,
            event: Event::new(
                "command",
                Value::map()
                    .with("name", verb.as_str())
                    .with("args", args.trim())
                    .with("words", words_v),
            ),
        });
        true
    }

    /// The host's own `/plugin` command: `list`, `reload <id>`, `unload <id>`.
    fn host_command(&mut self, words: &[String]) {
        match words.first().map(String::as_str) {
            None | Some("list") => {
                let list = self.plugins();
                if list.is_empty() {
                    self.tell("No plugins are loaded.");
                }
                for p in list {
                    let why = p.why.map_or(String::new(), |w| format!(": {w}"));
                    self.tell(&format!("{} {} ({}){why}", p.id, p.version, p.status));
                }
            }
            Some("reload") if words.len() == 2 => self.request_reload(&words[1].clone()),
            Some("unload") if words.len() == 2 => self.request_unload(&words[1].clone()),
            _ => self.tell("/plugin list, /plugin reload <id> or /plugin unload <id>"),
        }
    }

    /// The player pressed node `node` of a plugin's window: `ui.clicked` reaches the plugin at the
    /// next frame, as the player's gesture.
    ///
    /// # Errors
    /// No such plugin, window or node.
    pub fn click(&mut self, plugin: &str, window: &str, node: &str) -> Result<(), String> {
        let w = self
            .window(plugin, window)
            .ok_or_else(|| format!("{plugin} has no window {window:?}"))?;
        if w.tree.find(node).is_none() {
            return Err(format!("{plugin}'s window {window:?} has no node {node:?}"));
        }
        self.inputs.push_back(Input {
            plugin: plugin.to_owned(),
            event: Event::new(
                "ui.clicked",
                Value::map().with("window", window).with("id", node),
            ),
        });
        Ok(())
    }

    /// The player opens one of a plugin's windows (from the plugin bar): `ui.opened` reaches the
    /// plugin at the next frame.
    ///
    /// # Errors
    /// No such plugin or window.
    pub fn open_window(&mut self, plugin: &str, window: &str) -> Result<(), String> {
        let i = self
            .index(plugin)
            .ok_or_else(|| format!("no plugin {plugin}"))?;
        let w = self.plugins[i]
            .state
            .windows
            .get_mut(window)
            .ok_or_else(|| format!("{plugin} has no window {window:?}"))?;
        if !w.open {
            w.open = true;
            w.revision += 1;
            self.inputs.push_back(Input {
                plugin: plugin.to_owned(),
                event: Event::new("ui.opened", Value::map().with("window", window)),
            });
        }
        Ok(())
    }

    /// The player closes one of a plugin's windows (its close box): `ui.closed` reaches the
    /// plugin at the next frame. A window already closed is left as it is.
    ///
    /// # Errors
    /// No such plugin or window.
    pub fn close_window(&mut self, plugin: &str, window: &str) -> Result<(), String> {
        let i = self
            .index(plugin)
            .ok_or_else(|| format!("no plugin {plugin}"))?;
        let w = self.plugins[i]
            .state
            .windows
            .get_mut(window)
            .ok_or_else(|| format!("{plugin} has no window {window:?}"))?;
        if w.open {
            w.open = false;
            w.revision += 1;
            self.inputs.push_back(Input {
                plugin: plugin.to_owned(),
                event: Event::new("ui.closed", Value::map().with("window", window)),
            });
        }
        Ok(())
    }

    /// Write every changed store now.
    pub fn flush(&mut self) {
        for s in &mut self.plugins {
            flush_slot(&mut s.state, self.storage.as_mut());
        }
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

    /// A line of the host's own for the player's chat.
    fn tell(&mut self, text: &str) {
        self.shared.printed.push(Line {
            from: None,
            text: text.to_owned(),
        });
    }

    // ---------------------------------------------------------------------------------------------
    // The frame
    // ---------------------------------------------------------------------------------------------

    /// This frame's step (see the crate documentation).
    pub fn step(&mut self, frame: &mut HookFrame<'_>) {
        let view = frame.view;
        self.shared.now = frame.local_time;
        self.learn_session(frame.phase, view);
        self.frame_spent = Duration::ZERO;
        for s in &mut self.plugins {
            s.state.frame_spent = Duration::ZERO;
            s.state.prints = 0;
        }
        while let Some(r) = self.requests.pop_front() {
            match r {
                Request::Load(p) => {
                    if let Some(name) = self.load(p, None, view) {
                        self.tell(&format!("{name} is loaded."));
                    }
                }
                Request::Unload(id) => match self.index(&id) {
                    Some(i) => {
                        let name = self.plugins[i].state.told_name();
                        self.unload(i, view);
                        self.tell(&format!("{name} is unloaded."));
                    }
                    None => self.tell(&format!("No plugin {id} is loaded.")),
                },
                Request::Reload(id) => self.reload(&id, view),
            }
        }
        self.reconcile_contexts(frame.phase, view);
        let mut events = Vec::new();
        for e in frame.events {
            let world = self.shared.world.clone();
            let account = self.shared.account.clone();
            events.extend(events::translate(
                e,
                &mut self.shared.session,
                world.as_deref(),
                account.as_deref(),
            ));
            self.settle_session(e);
        }
        events.extend(frame.notices.iter().filter_map(notices::translate));
        self.open_scopes();
        self.index_services();
        self.drain_backlogs(view);
        for e in &events {
            self.dispatch(Target::All, e, false, view);
        }
        for s in std::mem::take(&mut self.settled) {
            if let Some(i) = self.index(&s.plugin) {
                let e = settled_event(&s);
                self.dispatch(Target::One(i), &e, false, view);
            }
        }
        for input in std::mem::take(&mut self.inputs) {
            if let Some(i) = self.index(&input.plugin) {
                self.dispatch(Target::One(i), &input.event, true, view);
            }
        }
        for c in std::mem::take(&mut self.service_calls) {
            let Some(i) = self.index(&c.provider) else {
                self.settle(&c.caller, c.id, "services.call", Err("no_plugin".into()));
                continue;
            };
            self.service_pending.insert(
                c.id,
                ServicePending {
                    caller: c.caller.clone(),
                    provider: c.provider.clone(),
                },
            );
            let e = Event::new(
                "service.call",
                Value::map()
                    .with("id", c.id)
                    .with("from", c.caller.as_str())
                    .with("service", c.service.as_str())
                    .with("args", c.args.clone())
                    .with("gesture", c.gesture),
            );
            self.dispatch(Target::One(i), &e, false, view);
        }
        for (i, e) in self.due_timers() {
            self.dispatch(Target::One(i), &e, false, view);
        }
        for a in std::mem::take(&mut self.shared.actions) {
            frame.act(a);
        }
        if self.shared.now - self.last_flush >= FLUSH_INTERVAL {
            self.last_flush = self.shared.now;
            self.flush();
        }
    }

    /// What the view says of the session: the character playing once the server named it, and the
    /// world it plays on when the product named none.
    fn learn_session(&mut self, phase: Phase, view: Option<&View>) {
        let Some(v) = view else {
            return;
        };
        if matches!(phase, Phase::Entering | Phase::World) {
            if let Some(id) = v.player_id {
                self.shared.session.playing = Some(id);
            }
        }
        if self.shared.world.is_none() {
            if let Some(p) = v.player.as_ref().filter(|p| !p.world_name.is_empty()) {
                self.shared.world = Some(p.world_name.clone());
            }
        }
    }

    /// The pending session actions this event ends.
    fn settle_session(&mut self, e: &GameEvent) {
        let ends: Option<(Option<Phase>, Result<Value, String>)> = match e {
            GameEvent::Session(SessionEvent::Phase(p)) => Some((Some(*p), Ok(Value::Nil))),
            GameEvent::Session(SessionEvent::EntryRefused { reason }) => Some((
                Some(Phase::World),
                Err(events::refusal_word(*reason).to_owned()),
            )),
            GameEvent::Session(SessionEvent::Ended(_)) => {
                Some((None, Err("disconnected".to_owned())))
            }
            _ => None,
        };
        let Some((phase, result)) = ends else {
            return;
        };
        if matches!(
            e,
            GameEvent::Session(SessionEvent::Phase(Phase::Characters))
        ) {
            // Back at the list after the world: the character's scope is written and closed.
            for s in &mut self.plugins {
                flush_slot(&mut s.state, self.storage.as_mut());
            }
        }
        let (done, keep): (Vec<Pending>, Vec<Pending>) = std::mem::take(&mut self.pending)
            .into_iter()
            .partition(|p| phase.is_none_or(|ph| ph == p.until));
        self.pending = keep;
        for p in done {
            self.settle(&p.plugin, p.id, &p.action, result.clone());
        }
    }

    fn settle(&mut self, plugin: &str, id: i64, action: &str, result: Result<Value, String>) {
        self.settled.push(Settled {
            plugin: plugin.to_owned(),
            id,
            action: action.to_owned(),
            result,
        });
    }

    /// Carry out what the last entry asked: its promised actions and its services' answers.
    fn carry_out(&mut self) {
        for a in std::mem::take(&mut self.shared.asked) {
            match a.action.as_str() {
                "session.select_character" => match events::parse_id(a.args.get("character")) {
                    Some(c) => {
                        self.shared
                            .actions
                            .push(Action::PlayCharacter { character: c });
                        self.shared.session.playing = Some(c);
                        self.pending.push(Pending {
                            plugin: a.plugin,
                            id: a.id,
                            action: a.action,
                            until: Phase::World,
                        });
                    }
                    None => self.settle(
                        &a.plugin,
                        a.id,
                        &a.action,
                        Err("a character is named by its id (\"0x…\")".into()),
                    ),
                },
                "session.logout" => {
                    self.shared.actions.push(Action::LogOff);
                    self.pending.push(Pending {
                        plugin: a.plugin,
                        id: a.id,
                        action: a.action,
                        until: Phase::Characters,
                    });
                }
                "services.call" => self.call_service(a),
                other => {
                    let why = format!("{other} is not offered by this host");
                    self.settle(&a.plugin, a.id, other, Err(why));
                }
            }
        }
        for (id, result) in std::mem::take(&mut self.shared.replies) {
            if let Some(p) = self.service_pending.remove(&id) {
                self.settle(&p.caller, id, "services.call", result);
            }
        }
    }

    fn call_service(&mut self, a: calls::Asked) {
        let provider = a.args.get("plugin").as_str().unwrap_or_default().to_owned();
        let service = a
            .args
            .get("service")
            .as_str()
            .unwrap_or_default()
            .to_owned();
        let offer = self
            .shared
            .service_index
            .iter()
            .find(|(p, s, _)| *p == provider && *s == service);
        let refusal = match offer {
            None => Some("no_service"),
            Some((_, _, Some(allow))) if !allow.contains(&a.plugin) => Some("not_allowed"),
            Some(_) => None,
        };
        if let Some(why) = refusal {
            self.settle(&a.plugin, a.id, &a.action, Err(why.into()));
            return;
        }
        self.service_calls.push(calls::ServiceCall {
            id: a.id,
            caller: a.plugin,
            provider,
            service,
            args: a.args.get("args").clone(),
            gesture: a.gesture,
        });
    }

    fn index_services(&mut self) {
        self.shared.service_index = self
            .plugins
            .iter()
            .filter(|s| s.state.status == Status::Running && s.state.active)
            .flat_map(|s| {
                s.state.services.iter().map(|(name, allow)| {
                    (s.state.manifest.id.to_string(), name.clone(), allow.clone())
                })
            })
            .collect();
    }

    fn reconcile_contexts(&mut self, phase: Phase, view: Option<&View>) {
        let now = context_of(phase);
        let previous = self.context.replace(now);
        for i in 0..self.plugins.len() {
            let st = &self.plugins[i].state;
            if st.status != Status::Running {
                continue;
            }
            let wants = st.manifest.contexts.contains(&now);
            if st.active == wants {
                continue;
            }
            self.plugins[i].state.active = wants;
            let e = if wants {
                Event::new(
                    "context.begin",
                    Value::map().with("context", now.name()).with(
                        "previous",
                        previous.filter(|p| *p != now).map(Context::name),
                    ),
                )
            } else {
                Event::new(
                    "context.end",
                    Value::map()
                        .with("context", previous.unwrap_or(now).name())
                        .with("next", now.name()),
                )
            };
            if self.plugins[i].state.subscriptions.contains(&e.name) {
                self.loading = wants;
                self.enter(i, view, |vm, call| vm.deliver(call, &e));
                self.loading = false;
            }
        }
    }

    fn due_timers(&mut self) -> Vec<(usize, Event)> {
        let now = self.shared.now;
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
                        // A stalled repeating timer fires once and goes on from now.
                        Some(every) => {
                            t.due = if t.due + every <= now {
                                now + every
                            } else {
                                t.due + every
                            };
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
            .map(|(_, i, id)| (i, Event::new("task.timer", Value::map().with("id", id))))
            .collect()
    }

    /// Deliver `e` to the plugins `target` names that hear it.
    fn dispatch(&mut self, target: Target, e: &Event, gesture: bool, view: Option<&View>) {
        let internal = matches!(
            e.name.as_str(),
            "command" | "task.timer" | "action.completed" | "action.failed" | "service.call"
        );
        let heard_inactive = dereth_plugin_api::event::LIFECYCLE.contains(&e.name.as_str())
            || matches!(e.name.as_str(), "action.completed" | "action.failed");
        for i in 0..self.plugins.len() {
            if target != Target::All && target != Target::One(i) {
                continue;
            }
            let st = &self.plugins[i].state;
            if st.status != Status::Running
                || !(internal || st.subscriptions.contains(&e.name))
                || (!st.active && !heard_inactive)
            {
                continue;
            }
            if gesture {
                self.plugins[i].state.activation_until = self.shared.now + ACTIVATION_SECS;
            }
            if !self.plugins[i].state.backlog.is_empty() || !self.may_enter(i) {
                self.hold(i, e.clone());
                continue;
            }
            self.shared.answering = gesture;
            self.enter(i, view, |vm, call| vm.deliver(call, e));
            self.shared.answering = false;
        }
    }

    /// A plugin's share of one frame, and all plugins' together.
    fn frame_budgets(&self) -> (Duration, Duration) {
        let entry = self.config.limits.call_budget_ms.max(0.0) / 1000.0;
        (
            Duration::from_secs_f64(entry * PLUGIN_FRAME_BUDGETS),
            Duration::from_secs_f64(entry * FRAME_BUDGETS),
        )
    }

    fn may_enter(&self, i: usize) -> bool {
        let (mine, all) = self.frame_budgets();
        self.plugins[i].state.frame_spent < mine && self.frame_spent < all
    }

    fn hold(&mut self, i: usize, e: Event) {
        let st = &mut self.plugins[i].state;
        if st.backlog.len() >= MAX_BACKLOG {
            st.backlog.pop_front();
        }
        st.backlog.push_back(e);
    }

    fn drain_backlogs(&mut self, view: Option<&View>) {
        for i in 0..self.plugins.len() {
            while self.plugins[i].state.status == Status::Running && self.may_enter(i) {
                let Some(e) = self.plugins[i].state.backlog.pop_front() else {
                    break;
                };
                self.enter(i, view, |vm, call| vm.deliver(call, &e));
            }
        }
    }

    /// Run one entry into plugin `i`'s VM, count its time, account for a failure, and carry out
    /// what it asked.
    fn enter<R>(
        &mut self,
        i: usize,
        view: Option<&View>,
        body: impl FnOnce(&mut Box<dyn ScriptInstance>, &mut Call<'_>) -> Result<R, ScriptError>,
    ) -> Option<R> {
        let (mine, _) = self.frame_budgets();
        let now = self.shared.now;
        let slot = &mut self.plugins[i];
        if slot.state.status != Status::Running {
            return None;
        }
        let mut vm = slot.vm.take()?;
        let started = self.vm_clock.now();
        let result = {
            let mut call = Call {
                slot: &mut slot.state,
                shared: &mut self.shared,
                config: &self.config,
                files: self.files.as_mut(),
                view,
            };
            body(&mut vm, &mut call)
        };
        let took = self.vm_clock.now().saturating_sub(started);
        slot.vm = Some(vm);
        let st = &mut slot.state;
        let before = st.frame_spent;
        st.frame_spent += took;
        self.frame_spent += took;
        if before < mine && st.frame_spent >= mine && !self.loading {
            st.frame_overruns.push_back(now);
        }
        let outcome = match result {
            Ok(r) => Some(r),
            Err(err) => {
                let text = err.to_string();
                let budget = err.kind == ScriptErrorKind::Budget;
                if !self.entry_chunk {
                    let first = text.lines().next().unwrap_or_default();
                    let first = if budget {
                        first.split(" (at ").next().unwrap_or_default()
                    } else {
                        first
                    };
                    self.shared.printed.push(Line {
                        from: Some(st.told_name()),
                        text: first.to_owned(),
                    });
                }
                st.log(text);
                if budget {
                    st.overruns.push_back(now);
                } else {
                    st.errors.push_back(now);
                }
                None
            }
        };
        for q in [&mut st.errors, &mut st.overruns, &mut st.frame_overruns] {
            while q.front().is_some_and(|t| now - t > FAULT_WINDOW) {
                q.pop_front();
            }
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
        self.carry_out();
        if let Some((id, reason)) = suspended {
            self.release(&id);
            self.tell(&format!(
                "{id} was suspended ({reason}). /plugin reload {id} starts it again."
            ));
        }
        outcome
    }

    // ---------------------------------------------------------------------------------------------
    // Load, unload, reload
    // ---------------------------------------------------------------------------------------------

    fn refuse(&mut self, what: &str, why: &str) -> Option<String> {
        self.tell(&format!("{what} was not loaded: {why}"));
        None
    }

    /// Load a package if it passes the checks; answers the name the chat tells it by.
    fn load(
        &mut self,
        package: Package,
        state: Option<Value>,
        view: Option<&View>,
    ) -> Option<String> {
        let origin = package.origin.clone();
        let m = match package.manifest() {
            Ok(m) => m,
            Err(e) => return self.refuse(&origin, &e.to_string()),
        };
        let what = m.id.to_string();
        if let Some(why) = Game::Auberean.refuses(m.game) {
            return self.refuse(&what, &why);
        }
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
        if m.ubscript {
            return self.refuse(&what, "scripts written for UtilityBelt run only in Dereth");
        }
        if self.index(&what).is_some() {
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
        let source = package.files.get(&entry).cloned().unwrap_or_default();
        let mut st = SlotState::new(m, package, self.shared.now);
        for key in self.shared.scope_keys() {
            open_scope(self.storage.as_mut(), &mut st, &key);
        }
        self.plugins.push(Slot {
            state: st,
            vm: Some(vm),
        });
        let i = self.plugins.len() - 1;
        self.loading = true;
        self.entry_chunk = true;
        let loaded = self.enter(i, view, |vm, call| vm.load_bytes(call, &entry, &source));
        self.entry_chunk = false;
        self.loading = false;
        let slot = &mut self.plugins[i];
        if loaded.is_none() && slot.state.status == Status::Running {
            let why = slot.state.console.back().cloned().unwrap_or_default();
            slot.state.status = Status::Faulted(why.clone());
            slot.vm = None;
            slot.state.windows.clear();
            slot.state.timers.clear();
            for verb in std::mem::take(&mut slot.state.commands).into_keys() {
                self.shared.verbs.remove(&verb);
            }
            let first = why.lines().next().unwrap_or_default().to_owned();
            self.tell(&format!("{what} failed to load: {first}"));
            return None;
        }
        if let Some(state) = state {
            let e = Event::new("plugin.reload", state);
            self.enter(i, view, |vm, call| vm.deliver(call, &e));
        }
        if self.plugins[i]
            .state
            .subscriptions
            .contains("plugin.loaded")
        {
            let e = Event::new("plugin.loaded", Value::map());
            self.enter(i, view, |vm, call| vm.deliver(call, &e));
        }
        Some(self.plugins[i].state.told_name())
    }

    /// Let go of what `id` holds that others wait on: the calls it was asked to answer fail, and
    /// its own calls and actions are forgotten.
    fn release(&mut self, id: &str) {
        let waiting: Vec<(i64, String)> = self
            .service_pending
            .iter()
            .filter(|(_, p)| p.provider == id)
            .map(|(k, p)| (*k, p.caller.clone()))
            .collect();
        for (k, caller) in waiting {
            self.service_pending.remove(&k);
            self.settle(&caller, k, "services.call", Err("no_plugin".into()));
        }
        self.service_pending.retain(|_, p| p.caller != id);
        self.service_calls.retain(|c| c.caller != id);
        self.pending.retain(|p| p.plugin != id);
        self.settled.retain(|s| s.plugin != id);
        self.inputs.retain(|s| s.plugin != id);
    }

    /// Unload plugin `i`: it hears `plugin.unload`, its stores are written, its commands go;
    /// answers what it handed on for a reload.
    fn unload(&mut self, i: usize, view: Option<&View>) -> Option<Value> {
        let e = Event::new("plugin.unload", Value::Nil);
        let state = self.enter(i, view, |vm, call| vm.deliver(call, &e));
        let mut slot = self.plugins.remove(i);
        let id = slot.state.manifest.id.to_string();
        self.release(&id);
        flush_slot(&mut slot.state, self.storage.as_mut());
        for verb in slot.state.commands.keys() {
            self.shared.verbs.remove(verb);
        }
        state.filter(|v| !v.is_nil())
    }

    fn reload(&mut self, id: &str, view: Option<&View>) {
        let Some(i) = self.index(id) else {
            let fetched = dereth_plugin_api::PluginId::parse(id)
                .and_then(|pid| self.source.as_mut().and_then(|s| s.fetch(&pid)));
            match fetched {
                Some(p) => {
                    if let Some(name) = self.load(p, None, view) {
                        self.tell(&format!("{name} is loaded."));
                    }
                }
                None => self.tell(&format!("No plugin {id} is loaded or available.")),
            }
            return;
        };
        let old = self.plugins[i].state.package.clone();
        let state = self.unload(i, view);
        let package = dereth_plugin_api::PluginId::parse(id)
            .and_then(|pid| self.source.as_mut().and_then(|s| s.fetch(&pid)))
            .unwrap_or(old);
        if let Some(name) = self.load(package, state, view) {
            self.tell(&format!("{name} is reloaded."));
        }
    }

    /// Open the store scopes the session has now in every plugin, and write and close the ones it
    /// no longer has.
    fn open_scopes(&mut self) {
        let keys = self.shared.scope_keys();
        for s in &mut self.plugins {
            let gone: Vec<String> = s
                .state
                .stores
                .keys()
                .filter(|k| !keys.contains(k))
                .cloned()
                .collect();
            for k in gone {
                if let Some(scope) = s.state.stores.remove(&k) {
                    save_scope(
                        self.storage.as_mut(),
                        &s.state.manifest.id.to_string(),
                        &k,
                        &scope,
                    );
                }
            }
            for k in &keys {
                open_scope(self.storage.as_mut(), &mut s.state, k);
            }
        }
    }
}

/// The event that settles a promise.
fn settled_event(s: &Settled) -> Event {
    match &s.result {
        Ok(v) => Event::new(
            "action.completed",
            Value::map()
                .with("id", s.id)
                .with("action", s.action.as_str())
                .with("value", v.clone()),
        ),
        Err(why) => Event::new(
            "action.failed",
            Value::map()
                .with("id", s.id)
                .with("action", s.action.as_str())
                .with("reason", why.as_str()),
        ),
    }
}

/// Open `key` for a plugin from its saved text, unless it is open.
fn open_scope(storage: &mut dyn PluginStorage, st: &mut SlotState, key: &str) {
    if st.stores.contains_key(key) {
        return;
    }
    let id = st.manifest.id.to_string();
    let values = storage
        .load(&id, key)
        .and_then(|t| Value::from_json(&t).ok())
        .and_then(|v| v.as_map().cloned())
        .unwrap_or_default();
    st.stores.insert(key.to_owned(), Scope::new(values));
}

fn save_scope(storage: &mut dyn PluginStorage, plugin: &str, key: &str, scope: &Scope) {
    if scope.dirty {
        storage.save(plugin, key, &Value::Map(scope.values.clone()).to_json());
    }
}

/// Write a plugin's changed stores.
fn flush_slot(st: &mut SlotState, storage: &mut dyn PluginStorage) {
    let id = st.manifest.id.to_string();
    for (k, scope) in &mut st.stores {
        save_scope(storage, &id, k, scope);
        scope.dirty = false;
    }
}

/// The host as the client's frame hook.
#[derive(Debug)]
pub struct PluginHook(pub PluginHost);

impl PluginHook {
    /// The host.
    #[must_use]
    pub const fn host(&self) -> &PluginHost {
        &self.0
    }

    /// The host, mutably.
    pub fn host_mut(&mut self) -> &mut PluginHost {
        &mut self.0
    }
}

impl FrameHook for PluginHook {
    fn step(&mut self, frame: &mut HookFrame<'_>) {
        self.0.step(frame);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
