//! One loaded plugin's state on the host side: everything the host keeps for it except the VM.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use dereth_plugin_api::{Manifest, Permission, UiNode, Value};

use crate::package::Package;

/// Where a loaded plugin is in its life.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// Loaded and receiving events.
    Running,
    /// Its entry chunk failed; it holds no VM.
    Faulted(String),
    /// Stopped by the host (too many errors, or over its budget too often); it holds its VM but
    /// hears nothing until reloaded.
    Suspended(String),
}

impl Status {
    #[must_use]
    pub fn word(&self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Faulted(_) => "faulted",
            Self::Suspended(_) => "suspended",
        }
    }
}

/// A timer: when it next fires, on the host's local clock in seconds, and its period if it repeats.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Timer {
    pub due: f64,
    pub every: Option<f64>,
}

/// One declared window.
#[derive(Debug, Clone, PartialEq)]
pub struct Window {
    pub tree: UiNode,
    /// Whether the player sees it. Closing a window keeps its tree.
    pub open: bool,
    /// Raised on every change, so a renderer rebuilds only what changed.
    pub revision: u64,
    /// The size the player gave a resizable window, in character cells and lines; kept when the
    /// plugin declares the window again.
    pub size: Option<(i64, i64)>,
    /// The player opened it, the last time it opened: from the plugin bar, or by the plugin while
    /// answering one of the player's gestures (a press on its window, one of its commands typed,
    /// bound to a key or put on a slot). Only the host's own toasts open otherwise: a plugin's
    /// window opens only when the player asks.
    pub by_player: bool,
}

/// One store scope's values.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Scope {
    pub values: BTreeMap<String, Value>,
    pub dirty: bool,
    /// About the bytes the scope takes as JSON: each entry's key and value text, kept up to date
    /// as entries change so a quota check does not write the whole scope out.
    pub size: usize,
}

impl Scope {
    /// A scope holding `values`, with its size counted.
    #[must_use]
    pub fn new(values: BTreeMap<String, Value>) -> Self {
        let size = values.iter().map(|(k, v)| entry_size(k, v)).sum();
        Self {
            values,
            dirty: false,
            size,
        }
    }
}

/// The bytes one entry adds to a scope's JSON: its key, its value and the punctuation.
#[must_use]
pub fn entry_size(key: &str, value: &Value) -> usize {
    key.len() + value.json_len() + 4
}

/// The most lines of a plugin's record (what it printed, the errors it met) kept.
pub const CONSOLE_LINES: usize = 200;

/// The most bytes a plugin's record keeps: the oldest lines go first.
pub const CONSOLE_BYTES: usize = 256 << 10;

/// The host's state for one plugin, apart from its VM.
#[derive(Debug)]
pub struct SlotState {
    /// What the plugin bar shows of it while it runs: `running`, `paused` or `idle` (its own word,
    /// `ui.status`).
    pub bar_state: String,
    pub manifest: Manifest,
    pub package: Package,
    pub status: Status,
    pub subscriptions: BTreeSet<String>,
    pub timers: BTreeMap<i64, Timer>,
    /// Claimed command verbs, with their help text.
    pub commands: BTreeMap<String, String>,
    pub windows: BTreeMap<String, Window>,
    pub stores: BTreeMap<String, Scope>,
    /// The local-clock times of recent errors, of recent budget overruns, and of recent frames
    /// the plugin ran past its share of.
    pub errors: VecDeque<f64>,
    pub overruns: VecDeque<f64>,
    pub frame_overruns: VecDeque<f64>,
    /// Its time in the VM this frame, all its entries together.
    pub frame_spent: web_time::Duration,
    /// Events held for a later frame, in order, while it is over its share of a frame or the
    /// plugins together are over theirs; and whether some were let go because there were too
    /// many.
    pub backlog: VecDeque<dereth_plugin_api::Event>,
    pub backlog_dropped: bool,
    /// Its record: what it printed and the errors it met, for a test or a tool to read.
    pub console: VecDeque<String>,
    /// The bytes of the record's lines.
    pub console_bytes: usize,
    /// When the plugin was loaded, on the local clock.
    pub loaded_at: f64,
    /// Until when, on the local clock, the player's last gesture on the plugin's own surface
    /// (a click on its window, a command it registered) covers its actions.
    pub activation_until: f64,
    /// The loot profiles the plugin has read, by handle (index).
    pub loot_profiles: Vec<crate::loot::Profile>,
    /// Its markers, by id.
    pub markers: BTreeMap<i64, crate::overlay::Marker>,
    /// Images it fetched, by the name `ui.image` shows them by (`net:<n>`).
    pub images: BTreeMap<String, Vec<u8>>,
    /// The pictures kept, its package's and those it fetched, each with its hash.
    pub pictures: crate::pictures::Pictures,
    /// Its HTTPS requests outstanding.
    pub net_in_flight: usize,
    /// When it sent its recent messages to its peers, on the local clock.
    pub peer_sends: VecDeque<f64>,
    /// The other clients it has heard from, by character.
    pub peers_heard: BTreeSet<String>,
    /// When it sent its recent chat lines, on the local clock.
    pub chat_sends: VecDeque<f64>,
    /// The patterns it has matched with (`text.match`), compiled.
    pub patterns: crate::pattern::Patterns,
    /// Whether it is active: the session is in one of its contexts. An inactive plugin hears
    /// only the lifecycle events, shows no windows and fires no timers.
    pub active: bool,
    /// The services it offers other plugins, each with the plugins it lets call it (`None`:
    /// any).
    pub services: BTreeMap<String, Option<BTreeSet<String>>>,
    /// When it last called a service, for the rate limit.
    pub service_calls: VecDeque<f64>,
    /// The windows the player had open when it was last reloaded: each opens again as it is
    /// declared again.
    pub reopen: BTreeSet<String>,
}

impl SlotState {
    #[must_use]
    pub fn new(manifest: Manifest, package: Package, now: f64) -> Self {
        Self {
            bar_state: "running".to_owned(),
            manifest,
            package,
            status: Status::Running,
            subscriptions: BTreeSet::new(),
            timers: BTreeMap::new(),
            commands: BTreeMap::new(),
            windows: BTreeMap::new(),
            stores: BTreeMap::new(),
            errors: VecDeque::new(),
            overruns: VecDeque::new(),
            frame_overruns: VecDeque::new(),
            frame_spent: web_time::Duration::ZERO,
            backlog: VecDeque::new(),
            backlog_dropped: false,
            console: VecDeque::new(),
            console_bytes: 0,
            loaded_at: now,
            activation_until: f64::NEG_INFINITY,
            loot_profiles: Vec::new(),
            markers: BTreeMap::new(),
            images: BTreeMap::new(),
            pictures: crate::pictures::Pictures::default(),
            net_in_flight: 0,
            peer_sends: VecDeque::new(),
            peers_heard: BTreeSet::new(),
            chat_sends: VecDeque::new(),
            patterns: crate::pattern::Patterns::default(),
            active: false,
            services: BTreeMap::new(),
            service_calls: VecDeque::new(),
            reopen: BTreeSet::new(),
        }
    }

    /// A line of the host's about the plugin (a picture it does not show, events let go): kept in
    /// its record as [`Self::log`] keeps one, and written to the client's log as a warning.
    pub fn warn(&mut self, line: impl Into<String>) {
        let line = line.into();
        tracing::warn!(target: "dereth::plugin", "{}: {line}", self.manifest.id);
        self.log(line);
    }

    /// Add a line to the plugin's record: cut to
    /// [`dereth_plugin_api::engine::CONSOLE_LINE_BYTES`], and the oldest lines let go past
    /// [`CONSOLE_LINES`] lines or [`CONSOLE_BYTES`] bytes.
    pub fn log(&mut self, line: impl Into<String>) {
        let line = line.into();
        let line = match dereth_plugin_api::engine::console_line(&line) {
            std::borrow::Cow::Borrowed(_) => line,
            std::borrow::Cow::Owned(cut) => cut,
        };
        // Also in the client's log, at debug level, for reading a plugin's run afterwards.
        tracing::debug!(target: "dereth::plugin", "{}: {line}", self.manifest.id);
        self.console_bytes += line.len();
        self.console.push_back(line);
        while self.console.len() > CONSOLE_LINES || self.console_bytes > CONSOLE_BYTES {
            let Some(old) = self.console.pop_front() else {
                break;
            };
            self.console_bytes -= old.len();
        }
    }

    /// Whether the plugin may do what `p` covers: everything, but HTTPS only when its manifest
    /// asks for `net.http` (and then only to the domains it lists).
    #[must_use]
    pub fn has(&self, p: Permission) -> bool {
        p != Permission::NetHttp || self.manifest.permissions.contains_key(&p)
    }
}
