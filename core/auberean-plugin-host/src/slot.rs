//! One loaded plugin's state on the host side: everything the host keeps for it but its VM.

use core::time::Duration;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use dereth_plugin_api::{Event, Manifest, Permission, UiNode, Value};

use crate::package::Package;

/// Where a loaded plugin is in its life.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// Loaded and hearing events.
    Running,
    /// Its entry file failed; it holds no VM.
    Faulted(String),
    /// Stopped by the host (too many errors, or over its budget too often); it hears nothing
    /// until it is reloaded.
    Suspended(String),
}

impl Status {
    /// The word the plugin list shows.
    #[must_use]
    pub const fn word(&self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Faulted(_) => "faulted",
            Self::Suspended(_) => "suspended",
        }
    }

    /// Why it is not running, when it is not.
    #[must_use]
    pub fn why(&self) -> Option<&str> {
        match self {
            Self::Running => None,
            Self::Faulted(s) | Self::Suspended(s) => Some(s),
        }
    }
}

/// A timer: when it is next due, on the host's frame clock in seconds, and its period if it
/// repeats.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Timer {
    pub due: f64,
    pub every: Option<f64>,
}

/// One declared window or HUD.
#[derive(Debug, Clone, PartialEq)]
pub struct Window {
    pub tree: UiNode,
    /// Whether the player sees it. Closing a window keeps its tree.
    pub open: bool,
    /// Raised on every change, so whatever draws it rebuilds only what changed.
    pub revision: u64,
}

/// One store scope's values.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Scope {
    pub values: BTreeMap<String, Value>,
    /// Changed since it was last written.
    pub dirty: bool,
    /// About the bytes the scope takes as JSON, kept up to date as entries change.
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

/// The host's state for one plugin, apart from its VM.
#[derive(Debug)]
pub struct SlotState {
    pub manifest: Manifest,
    pub package: Package,
    pub status: Status,
    /// What it says it is doing (`ui.status`): `running`, `paused` or `idle`.
    pub bar_state: String,
    pub subscriptions: BTreeSet<String>,
    pub timers: BTreeMap<i64, Timer>,
    /// The command verbs it claimed, with their help text.
    pub commands: BTreeMap<String, String>,
    pub windows: BTreeMap<String, Window>,
    /// Its open store scopes, by the host's key (`global`, `world:<world>`,
    /// `character:<world>:<account>:<character>`).
    pub stores: BTreeMap<String, Scope>,
    /// The frame-clock times of its recent errors, of its recent entries stopped at their budget,
    /// and of the recent frames it ran past its share of.
    pub errors: VecDeque<f64>,
    pub overruns: VecDeque<f64>,
    pub frame_overruns: VecDeque<f64>,
    /// Its time in its VM this frame, all its entries together.
    pub frame_spent: Duration,
    /// Events held for a later frame, in order, while it is past its share of a frame.
    pub backlog: VecDeque<Event>,
    /// Its record: what it printed and the errors it met, oldest first.
    pub console: VecDeque<String>,
    /// When it was loaded, on the frame clock.
    pub loaded_at: f64,
    /// Until when, on the frame clock, the player's last gesture on its own surface (a click on
    /// one of its windows, one of its commands typed) covers what it does.
    pub activation_until: f64,
    /// Whether it is active: the session is in one of its contexts. An inactive plugin hears only
    /// its own life and the session's, and its timers wait.
    pub active: bool,
    /// The services it offers the other plugins, each with the plugins it lets call it (`None`:
    /// any).
    pub services: BTreeMap<String, Option<BTreeSet<String>>>,
    /// When it sent its recent lines to the server, on the frame clock.
    pub chat_sends: VecDeque<f64>,
    /// The local lines it printed this frame.
    pub prints: usize,
}

impl SlotState {
    #[must_use]
    pub fn new(manifest: Manifest, package: Package, now: f64) -> Self {
        Self {
            manifest,
            package,
            status: Status::Running,
            bar_state: "running".to_owned(),
            subscriptions: BTreeSet::new(),
            timers: BTreeMap::new(),
            commands: BTreeMap::new(),
            windows: BTreeMap::new(),
            stores: BTreeMap::new(),
            errors: VecDeque::new(),
            overruns: VecDeque::new(),
            frame_overruns: VecDeque::new(),
            frame_spent: Duration::ZERO,
            backlog: VecDeque::new(),
            console: VecDeque::new(),
            loaded_at: now,
            activation_until: f64::NEG_INFINITY,
            active: false,
            services: BTreeMap::new(),
            chat_sends: VecDeque::new(),
            prints: 0,
        }
    }

    /// Add a line to the plugin's record, cut to
    /// [`dereth_plugin_api::engine::CONSOLE_LINE_BYTES`]; past [`CONSOLE_LINES`] the oldest goes.
    pub fn log(&mut self, line: impl Into<String>) {
        let line = line.into();
        let line = dereth_plugin_api::engine::console_line(&line).into_owned();
        self.console.push_back(line);
        while self.console.len() > CONSOLE_LINES {
            self.console.pop_front();
        }
    }

    /// Whether the plugin may do what `p` covers: everything, but HTTPS only when its manifest
    /// asks for it.
    #[must_use]
    pub fn has(&self, p: Permission) -> bool {
        p != Permission::NetHttp || self.manifest.permissions.contains_key(&p)
    }

    /// The name the chat tells it by: its manifest's name, or its id when it gives none.
    #[must_use]
    pub fn told_name(&self) -> String {
        if self.manifest.name.trim().is_empty() {
            self.manifest.id.to_string()
        } else {
            self.manifest.name.clone()
        }
    }
}
