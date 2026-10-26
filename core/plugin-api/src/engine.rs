//! The boundary between the plugin host and a script engine.
//!
//! The host owns everything a plugin can touch; the engine owns the language. The host asks the
//! engine for a VM per plugin ([`ScriptEngine::instantiate`]), loads the plugin's entry chunk into
//! it and delivers events to it. The plugin reaches back only through [`HostCalls`]: one entry
//! point taking an operation name and plain values, which the host checks against its own
//! rules. An engine therefore needs no knowledge of the game, and another engine is a new
//! implementation of these two traits and the Lua-level bindings, not a new host.

use std::fmt;

use crate::event::Event;
use crate::value::Value;

/// What a VM may use.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Limits {
    /// The most memory the VM may hold, in bytes.
    pub memory_bytes: usize,
    /// How long one entry into the VM (loading a chunk, or delivering one event) may run, in
    /// milliseconds, before the VM stops it.
    pub call_budget_ms: f64,
    /// Keep a profile of where the VM's time goes and write it to the log now and then (a tool
    /// for a plugin's author; off by default).
    pub profile: bool,
}

impl Default for Limits {
    /// 64 MiB and 4 ms per call, the spec's defaults.
    fn default() -> Self {
        Self {
            memory_bytes: 64 << 20,
            call_budget_ms: 4.0,
            profile: false,
        }
    }
}

/// The most values (each table, entry, string, number and boolean) one value a plugin hands the
/// host may hold, counted as it is copied: a table that shares its parts is counted once for every
/// place it appears, so a value small in the VM cannot become a large one in the host.
pub const COPY_NODES: usize = 1 << 18;

/// The most bytes one value a plugin hands the host may hold, counted as it is copied: its strings
/// and keys, and a few bytes for each other value.
pub const COPY_BYTES: usize = 16 << 20;

/// Loading a plugin (its entry file's top level, a compiled plugin's `init`) has this many entries'
/// budgets: it sets the plugin up once.
pub const LOAD_BUDGETS: f64 = 25.0;

/// The budgets an entry delivering event `name` has: the load's for `context.begin`, where a plugin
/// sets itself up for the part of the session it is now active in, else one.
#[must_use]
pub fn entry_budgets(name: &str) -> f64 {
    if name == "context.begin" {
        LOAD_BUDGETS
    } else {
        1.0
    }
}

/// The most bytes of one line a plugin writes to its console; a longer line is cut there.
pub const CONSOLE_LINE_BYTES: usize = 4096;

/// What one copy of a plugin's value into the host has used of [`COPY_NODES`] and
/// [`COPY_BYTES`].
#[derive(Debug, Clone, Default)]
pub struct CopyBudget {
    nodes: usize,
    bytes: usize,
}

impl CopyBudget {
    /// Count one value of `bytes` bytes, or fail once the copy is over either bound.
    ///
    /// # Errors
    /// The value being copied is larger than one copy may be.
    pub fn take(&mut self, bytes: usize) -> Result<(), String> {
        self.nodes += 1;
        self.bytes = self.bytes.saturating_add(bytes);
        if self.nodes > COPY_NODES || self.bytes > COPY_BYTES {
            return Err(format!(
                "a value too large to hand to the host (more than {COPY_NODES} values or {} MiB)",
                COPY_BYTES >> 20
            ));
        }
        Ok(())
    }
}

/// `line` cut to at most [`CONSOLE_LINE_BYTES`] bytes, on a character boundary, with `…` where it
/// was cut.
#[must_use]
pub fn console_line(line: &str) -> std::borrow::Cow<'_, str> {
    if line.len() <= CONSOLE_LINE_BYTES {
        return std::borrow::Cow::Borrowed(line);
    }
    let mut end = CONSOLE_LINE_BYTES - '…'.len_utf8();
    while !line.is_char_boundary(end) {
        end -= 1;
    }
    std::borrow::Cow::Owned(format!("{}…", &line[..end]))
}

/// Why a host operation failed. It becomes a Lua error at the call site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallError {
    /// No such operation.
    Unknown(String),
    /// The arguments are wrong.
    BadArgs(String),
    /// The operation failed for a reason of its own.
    Failed(String),
}

impl fmt::Display for CallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unknown(op) => write!(f, "no host operation {op:?}"),
            Self::BadArgs(s) | Self::Failed(s) => f.write_str(s),
        }
    }
}

impl std::error::Error for CallError {}

/// The host side of a plugin's calls, lent to the engine for the length of one entry.
pub trait HostCalls {
    /// Perform one operation for the plugin.
    ///
    /// # Errors
    /// [`CallError`], which the engine raises in the plugin as a Lua error.
    fn call(&mut self, op: &str, args: &[Value]) -> Result<Value, CallError>;

    /// A line for the plugin's console (`print`).
    fn console(&mut self, line: &str);

    /// The source of a module in the plugin's own package, for `require`: `name` is the module name
    /// (`"lib.util"`), the answer the file's text.
    fn module_source(&mut self, name: &str) -> Option<String>;
}

/// What kind of failure a VM reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptErrorKind {
    /// The source does not compile.
    Syntax,
    /// A Lua error raised while running.
    Runtime,
    /// The entry ran past its time budget and was stopped.
    Budget,
    /// The VM reached its memory limit.
    Memory,
}

/// A failure inside a VM, with the message and, for runtime errors, a traceback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptError {
    pub kind: ScriptErrorKind,
    pub message: String,
}

impl ScriptError {
    #[must_use]
    pub fn new(kind: ScriptErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

impl fmt::Display for ScriptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let what = match self.kind {
            ScriptErrorKind::Syntax => "syntax error",
            ScriptErrorKind::Runtime => "error",
            ScriptErrorKind::Budget => "stopped: over its time budget",
            ScriptErrorKind::Memory => "stopped: out of memory",
        };
        write!(f, "{what}: {}", self.message)
    }
}

impl std::error::Error for ScriptError {}

/// A script engine: makes VMs.
pub trait ScriptEngine: fmt::Debug {
    /// The engine's name, for the plugin console and bug reports.
    fn name(&self) -> &'static str;

    /// Whether it runs a package whose manifest's entry is `entry`: by default a Lua or Luau
    /// file.
    fn handles(&self, entry: &str) -> bool {
        let e = entry.to_ascii_lowercase();
        e.ends_with(".lua") || e.ends_with(".luau")
    }

    /// A fresh, sandboxed VM with the plugin API's Lua-level bindings installed.
    ///
    /// # Errors
    /// The VM could not be made.
    fn instantiate(&self, limits: Limits) -> Result<Box<dyn ScriptInstance>, ScriptError>;
}

/// One plugin's VM.
pub trait ScriptInstance: fmt::Debug {
    /// Compile and run a chunk (the plugin's entry file) at the VM's top level.
    ///
    /// # Errors
    /// A compile or run failure.
    fn load(
        &mut self,
        host: &mut dyn HostCalls,
        chunk_name: &str,
        source: &str,
    ) -> Result<(), ScriptError>;

    /// Load the entry file from its bytes: a text engine takes them as source (by default, as
    /// UTF-8 through [`ScriptInstance::load`]); a compiled one as its module.
    ///
    /// # Errors
    /// A compile or run failure.
    fn load_bytes(
        &mut self,
        host: &mut dyn HostCalls,
        chunk_name: &str,
        bytes: &[u8],
    ) -> Result<(), ScriptError> {
        let source = std::str::from_utf8(bytes).map_err(|_| {
            ScriptError::new(
                ScriptErrorKind::Syntax,
                format!("{chunk_name} is not UTF-8 text"),
            )
        })?;
        self.load(host, chunk_name, source)
    }

    /// Deliver one event to the handlers the plugin registered for it, and answer what they
    /// returned (the state an unload hands to the next load, for example).
    ///
    /// # Errors
    /// A handler failed. Handlers after a failing one still run; the first failure is reported.
    fn deliver(&mut self, host: &mut dyn HostCalls, event: &Event) -> Result<Value, ScriptError>;

    /// The bytes the VM holds.
    fn memory_used(&self) -> usize;
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the budget rule as numbers; the conformance suite's `core/budget` pins an
    //! entry stopped at its budget).
    use super::{entry_budgets, LOAD_BUDGETS};

    #[test]
    fn the_entry_telling_a_plugin_it_is_active_has_the_loads_budget() {
        assert!((entry_budgets("context.begin") - LOAD_BUDGETS).abs() < f64::EPSILON);
        for other in ["context.end", "task.timer", "chat.line", "ui.clicked"] {
            assert!((entry_budgets(other) - 1.0).abs() < f64::EPSILON, "{other}");
        }
    }
}
