//! The plugin script engine: one Luau VM per plugin, sandboxed, with a memory cap and a time budget
//! per entry, and the plugin API's bindings in Lua.
//!
//! **Depends on** `dereth-plugin-api` and the Luau VM (through `mlua`). **Used by** the products
//! that load plugins, through the client SDK.
//!
//! **Must never** depend on another crate of this workspace or reach the game, the platform or a
//! file (`cargo xtask seams`, `seam: plugin-lua deps`). Everything a plugin does goes through
//! [`HostCalls`], which the host lends for the length of one entry into the VM.
//!
//! # How a VM is built
//!
//! [`LuauEngine::instantiate`] makes a VM with Luau's safe libraries, runs the bindings
//! (`prelude.lua`) with three private functions (the host call, a compiler for the plugin's own
//! modules and the traceback), installs the table the bindings return as globals, and seals the
//! sandbox: from then on the globals are read-only and the plugin's own globals live in a table of
//! its own. The host call is the only way out of the VM. It is a function that exists only while
//! the host is inside the VM ([`LuauInstance::load`], [`LuauInstance::deliver`]): outside, calling
//! it fails.
//!
//! # Budgets
//!
//! Luau calls the interrupt at every loop back-edge and function call. The interrupt compares the
//! clock (the machine's monotonic clock, unless the VM is given another:
//! [`LuauInstance::with_clock`]) with the entry's deadline and stops the VM past it, so a plugin
//! that loops forever costs one budget and is then reported. The time is the entry's, the host's
//! answers to its calls included: a call that makes the host work (a search, a pattern, a file
//! written) is the plugin's time. Past the deadline the entry is stopped once it has also done a
//! least amount of work ([`MIN_WORK`] interrupts), so that a thread the system set aside for a
//! moment in the middle of a short entry has not overrun; and past the deadline by [`STALL`] it is
//! stopped whatever work it did, so that a few long calls cannot carry it on. The memory cap is
//! the VM allocator's: an allocation past it fails as a Lua memory error, which the plugin cannot
//! turn into success.

//!
//! # In a browser
//!
//! For `wasm32-unknown-unknown` the VM is compiled with Wasm exception handling and links a small C
//! and C++ runtime of its own (the build script, and `wasm_rt` for its Rust half). Its C++
//! static constructors are run once, before the first VM.

#![deny(unsafe_code)]

#[cfg(target_arch = "wasm32")]
#[allow(unsafe_code)]
pub mod wasm_rt;

pub mod profile;

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

use dereth_plugin_api::{
    Event, HostCalls, Limits, ScriptEngine, ScriptError, ScriptErrorKind, ScriptInstance, Value,
    VmClock,
};
use mlua::chunk::ChunkMode;
use mlua::{Function, Lua, MultiValue, Table, VmState};
use web_time::{Duration, Instant};

/// The bindings, run once per VM.
const PRELUDE: &str = include_str!("prelude.lua");

/// The host's libraries written in Lua over the plugin interface, each compiled the first time a
/// plugin's `require` names it: UtilityBelt's script API (`dereth.ubscript`) and its parts.
pub const LIBRARIES: &[(&str, &str)] = &[
    ("dereth.ubscript", include_str!("ubscript/base.lua")),
    ("dereth.ubscript.enums", include_str!("ubscript/enums.lua")),
    (
        "dereth.ubscript.actions",
        include_str!("ubscript/actions.lua"),
    ),
    ("dereth.ubscript.world", include_str!("ubscript/world.lua")),
    ("dereth.ubscript.views", include_str!("ubscript/views.lua")),
    (
        "dereth.ubscript.modules",
        include_str!("ubscript/modules.lua"),
    ),
];

/// The registry key of the host call that exists while the host is inside the VM.
const HOST_KEY: &str = "dereth.host";

/// Values nested deeper than this do not cross between a plugin and its host.
const MAX_DEPTH: usize = 64;

/// The Luau engine.
#[derive(Debug, Default, Clone, Copy)]
pub struct LuauEngine;

impl ScriptEngine for LuauEngine {
    fn name(&self) -> &'static str {
        "luau"
    }

    fn instantiate(&self, limits: Limits) -> Result<Box<dyn ScriptInstance>, ScriptError> {
        LuauInstance::new(limits)
            .map(|i| Box::new(i) as Box<dyn ScriptInstance>)
            .map_err(|e| {
                ScriptError::new(
                    ScriptErrorKind::Runtime,
                    format!("the VM could not be made: {e}"),
                )
            })
    }
}

/// The interrupts an entry must have reached before its deadline can stop it: far less work than
/// any budget holds, more than a short entry does.
pub const MIN_WORK: u32 = 256;

/// How far past its deadline an entry is stopped whatever work it did: longer than the system sets
/// a thread aside for, short enough that a few long calls cannot run on past it.
pub const STALL: Duration = Duration::from_millis(20);

/// Loading a plugin (its entry file's top level) has this many entries' budgets: it sets the plugin
/// up once. Compiling the plugin's own source, there or by `require`, is not counted at all.
pub const LOAD_BUDGETS: f64 = dereth_plugin_api::engine::LOAD_BUDGETS;

/// Where the script was when its budget ran out, innermost first: " (at engine:12 in tick <
/// engine:40)", or nothing when no script frame is on the stack. The plugin's own lines, so its
/// author can tell which work is too long for one entry.
fn where_running(lua: &Lua) -> String {
    let mut frames = Vec::new();
    for level in 0..16 {
        let Some(frame) = lua.inspect_stack(level, |d| {
            let line = d.current_line()?;
            let src = d.source().short_src.map(|s| s.into_owned())?;
            let name = d.names().name.map(|n| n.into_owned());
            Some(match name {
                Some(n) => format!("{src}:{line} in {n}"),
                None => format!("{src}:{line}"),
            })
        }) else {
            break;
        };
        if let Some(f) = frame {
            frames.push(f);
            if frames.len() == 4 {
                break;
            }
        }
    }
    if frames.is_empty() {
        String::new()
    } else {
        format!(" (at {})", frames.join(" < "))
    }
}

/// After the budget first runs out, how long the script may go on to handle the error (a task's
/// traceback reaching the plugin's console, a loop's `pcall` noting it and sleeping): one overrun
/// then costs the plugin an overrun, not the task it happened in. Past this the entry is stopped
/// whatever it catches.
const GRACE: Duration = Duration::from_millis(1);

impl Budget {
    /// Move the entry's deadline on by `by`: time that is not the plugin's.
    fn extend(&self, by: Duration) {
        if let Some(d) = self.deadline.get() {
            self.deadline.set(Some(d + by));
        }
    }
}

/// The deadline of the entry in progress, on the VM's clock.
#[derive(Debug, Default)]
struct Budget {
    deadline: Cell<Option<Duration>>,
    /// Interrupts since the entry began.
    work: Cell<u32>,
    tripped: Cell<bool>,
    /// The end of the grace after the first overrun of this entry.
    grace: Cell<Option<Duration>>,
}

/// The machine's monotonic clock: a VM's clock unless it is given another.
#[derive(Debug)]
struct MachineClock {
    start: Instant,
}

impl VmClock for MachineClock {
    fn now(&self) -> Duration {
        self.start.elapsed()
    }
}

/// One plugin's VM.
pub struct LuauInstance {
    lua: Lua,
    dispatch: Function,
    /// Run as each entry begins: the bindings forget what they read of the game in the last.
    new_entry: Function,
    budget: Rc<Budget>,
    /// What the budget and the profile read the time from.
    clock: Rc<dyn VmClock>,
    limits: Limits,
    /// Where the time goes, when [`Limits::profile`] asks.
    profile: Option<Rc<profile::Profile>>,
}

impl std::fmt::Debug for LuauInstance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LuauInstance")
            .field("memory_used", &self.lua.used_memory())
            .field("limits", &self.limits)
            .finish_non_exhaustive()
    }
}

impl LuauInstance {
    fn new(limits: Limits) -> mlua::Result<Self> {
        Self::with_clock(
            limits,
            Rc::new(MachineClock {
                start: Instant::now(),
            }),
        )
    }

    /// A VM whose budget, and profile, read the time from `clock` rather than the machine's
    /// monotonic clock.
    ///
    /// # Errors
    /// The VM could not be made.
    pub fn with_clock(limits: Limits, clock: Rc<dyn VmClock>) -> mlua::Result<Self> {
        #[cfg(target_arch = "wasm32")]
        wasm_rt::init();
        let lua = Lua::new();
        let budget = Rc::new(Budget::default());
        let b = budget.clone();
        let profile = limits.profile.then(|| Rc::new(profile::Profile::default()));
        let p = profile.clone();
        let c = clock.clone();
        lua.set_interrupt(move |lua| {
            if let Some(deadline) = b.deadline.get() {
                let now = c.now();
                if let Some(p) = &p {
                    p.sample(lua, now);
                }
                let work = b.work.get().saturating_add(1);
                b.work.set(work);
                if now > deadline && (work >= MIN_WORK || now > deadline + STALL) {
                    match b.grace.get() {
                        None => b.grace.set(Some(now + GRACE)),
                        Some(end) if now < end => return Ok(VmState::Continue),
                        Some(_) => {}
                    }
                    b.tripped.set(true);
                    return Err(mlua::Error::runtime(format!(
                        "the plugin ran past its time budget{}",
                        where_running(lua)
                    )));
                }
            }
            Ok(VmState::Continue)
        });
        // The host call's permanent half: it finds the scoped half, which exists only while the
        // host is inside the VM.
        let hostcall = lua.create_function(|lua, args: MultiValue| {
            let f: Option<Function> = lua.named_registry_value(HOST_KEY)?;
            let f = f.ok_or_else(|| {
                mlua::Error::runtime("the host is not reachable outside an entry")
            })?;
            f.call::<MultiValue>(args)
        })?;
        // Compiling one of the plugin's own modules is the host's work on its package, bounded by
        // the package and done once a module: the deadline moves on by it.
        let compiling = budget.clone();
        let c = clock.clone();
        // A host library may compile a module with globals of its own (`env`): a script written
        // for UtilityBelt's scripting sees that API's globals in place of the plugin interface's.
        let compile = lua.create_function(
            move |lua, (name, source, env): (String, mlua::LuaString, Option<Table>)| {
                let started = c.now();
                let mut chunk = lua
                    .load(source.as_bytes().to_vec())
                    .set_name(format!("={name}"))
                    .set_mode(ChunkMode::Text);
                if let Some(env) = env {
                    chunk = chunk.set_environment(env);
                }
                let f = chunk.into_function();
                compiling.extend(c.now().saturating_sub(started));
                f
            },
        )?;
        let libraries = lua.create_table()?;
        for (name, source) in LIBRARIES {
            libraries.set(*name, *source)?;
        }
        let traceback: Function = lua.globals().get::<Table>("debug")?.get("traceback")?;
        let (api, dispatch, new_entry): (Table, Function, Function) = lua
            .load(PRELUDE)
            .set_name("=bindings")
            .set_mode(ChunkMode::Text)
            .call((hostcall, compile, traceback, libraries))?;
        let globals = lua.globals();
        let mut api_names = Vec::new();
        for pair in api.pairs::<String, mlua::Value>() {
            let (k, v) = pair?;
            api_names.push(k.clone());
            globals.set(k, v)?;
        }
        // The API's tables answer from the game as it is now (`game.player.level`,
        // `store.character.x`): the compiler is told they change, or it would read such a path
        // once, when the code is loaded, and keep that value.
        lua.set_compiler(mlua::chunk::Compiler::new().set_mutable_globals(api_names));
        // Source is compiled only through `require`, and only from the plugin's own package.
        globals.set("loadstring", mlua::Value::Nil)?;
        lua.sandbox(true)?;
        lua.set_memory_limit(limits.memory_bytes)?;
        Ok(Self {
            lua,
            dispatch,
            new_entry,
            budget,
            clock,
            limits,
            profile,
        })
    }

    /// Run `body` inside the VM with `host` reachable through the host call, under the budget.
    fn enter<R>(
        &mut self,
        host: &mut dyn HostCalls,
        label: &str,
        body: impl FnOnce(&Lua) -> mlua::Result<R>,
    ) -> Result<R, ScriptError> {
        let clock = &*self.clock;
        let entered = clock.now();
        if let Some(p) = &self.profile {
            p.begin(entered);
        }
        let cell = RefCell::new(host);
        let lua = &self.lua;
        self.budget.tripped.set(false);
        self.budget.grace.set(None);
        self.budget.work.set(0);
        let entries = if label == "load" {
            LOAD_BUDGETS
        } else {
            dereth_plugin_api::engine::entry_budgets(label)
        };
        self.budget.deadline.set(Some(
            clock.now()
                + Duration::from_secs_f64(self.limits.call_budget_ms.max(0.0) * entries / 1000.0),
        ));
        let profile = self.profile.as_deref();
        let new_entry = self.new_entry.clone();
        let result = lua.scope(|scope| {
            // The host's time answering a call is the entry's time: the deadline stays where it
            // is, and the next interrupt past it stops the entry.
            let call = scope.create_function(|lua, args: MultiValue| {
                let start = clock.now();
                let op = profile.map(|_| match args.front() {
                    Some(mlua::Value::String(s)) => s.to_string_lossy(),
                    _ => String::new(),
                });
                let out = host_call(lua, &cell, args);
                if let (Some(p), Some(op)) = (profile, op) {
                    let now = clock.now();
                    p.host_call(&op, now.saturating_sub(start), now);
                }
                out
            })?;
            lua.set_named_registry_value(HOST_KEY, call)?;
            new_entry.call::<()>(())?;
            body(lua)
        });
        self.budget.deadline.set(None);
        let _ = lua.set_named_registry_value(HOST_KEY, mlua::Value::Nil);
        let table = self.profile.as_ref().and_then(|p| {
            let now = clock.now();
            p.end(label, now.saturating_sub(entered), lua.used_memory(), now)
        });
        if let Some(table) = table {
            // To the plugin's console, which the client also writes to its log.
            if let Ok(mut host) = cell.try_borrow_mut() {
                host.console(&format!("profile {table}"));
            }
        }
        let result = result.map_err(|e| self.error(&e));
        if matches!(&result, Err(e) if e.kind == ScriptErrorKind::Memory) {
            // What the failed entry allocated is garbage now; collect it so the next entry starts
            // from what the plugin actually keeps.
            let _ = self.lua.gc_collect();
        }
        result
    }

    fn error(&self, e: &mlua::Error) -> ScriptError {
        if self.budget.tripped.take() {
            return ScriptError::new(ScriptErrorKind::Budget, e.to_string());
        }
        match e {
            mlua::Error::SyntaxError { message, .. } => {
                ScriptError::new(ScriptErrorKind::Syntax, message.clone())
            }
            mlua::Error::MemoryError(m) => ScriptError::new(ScriptErrorKind::Memory, m.clone()),
            other => classify(other.to_string()),
        }
    }
}

/// A handler's failure message, as the kind it is.
fn classify(message: String) -> ScriptError {
    if message.contains("not enough memory") {
        ScriptError::new(ScriptErrorKind::Memory, message)
    } else {
        ScriptError::new(ScriptErrorKind::Runtime, message)
    }
}

/// One call from the plugin into the host.
fn host_call(
    lua: &Lua,
    host: &RefCell<&mut dyn HostCalls>,
    args: MultiValue,
) -> mlua::Result<MultiValue> {
    let mut host = host
        .try_borrow_mut()
        .map_err(|_| mlua::Error::runtime("a host call cannot re-enter itself"))?;
    let mut args = args.into_iter();
    let op = match args.next() {
        Some(mlua::Value::String(s)) => s.to_str()?.to_owned(),
        _ => {
            return Err(mlua::Error::runtime(
                "a host call names its operation first",
            ));
        }
    };
    match op.as_str() {
        "console.print" => {
            // Only what the console keeps of the line is copied out of the VM.
            let text = match args.next() {
                Some(mlua::Value::String(s)) => {
                    let b = s.as_bytes();
                    let cut = &b[..b
                        .len()
                        .min(dereth_plugin_api::engine::CONSOLE_LINE_BYTES + 4)];
                    String::from_utf8_lossy(cut).into_owned()
                }
                _ => String::new(),
            };
            host.console(&dereth_plugin_api::engine::console_line(&text));
            Ok(MultiValue::new())
        }
        "module.source" => {
            let name = match args.next() {
                Some(mlua::Value::String(s)) => s.to_str()?.to_owned(),
                _ => return Err(mlua::Error::runtime("require(name): name must be a string")),
            };
            let src = host.module_source(&name);
            Ok(MultiValue::from_vec(vec![match src {
                Some(s) => mlua::Value::String(lua.create_string(s)?),
                None => mlua::Value::Nil,
            }]))
        }
        _ => {
            // One budget for the call's arguments together.
            let mut budget = Copy::default();
            let vals = args
                .map(|v| budget.value(&v, 0))
                .collect::<mlua::Result<Vec<_>>>()?;
            let out = host
                .call(&op, &vals)
                .map_err(|e| mlua::Error::runtime(e.to_string()))?;
            Ok(MultiValue::from_vec(vec![from_value(lua, &out, 0)?]))
        }
    }
}

/// A Lua value as a plain value. A table whose keys are exactly `1..=n` is a list; any other
/// table is a map, its keys strings (a number key is written as its decimal text).
///
/// # Errors
/// A function, a coroutine or userdata, nesting past [`MAX_DEPTH`], a table that holds itself, or
/// a value larger than one copy into the host may be ([`dereth_plugin_api::engine::CopyBudget`]).
pub fn to_value(v: &mlua::Value, depth: usize) -> mlua::Result<Value> {
    Copy::default().value(v, depth)
}

/// One copy of a plugin's values into the host: what it has used of the copy's bounds, and the
/// tables it is inside, so that a table holding itself is refused rather than followed.
#[derive(Default)]
struct Copy {
    budget: dereth_plugin_api::engine::CopyBudget,
    inside: Vec<*const std::ffi::c_void>,
}

impl Copy {
    fn take(&mut self, bytes: usize) -> mlua::Result<()> {
        self.budget.take(bytes).map_err(mlua::Error::runtime)
    }

    fn value(&mut self, v: &mlua::Value, depth: usize) -> mlua::Result<Value> {
        if depth > MAX_DEPTH {
            return Err(mlua::Error::runtime(
                "a value nested too deeply to pass to the host",
            ));
        }
        Ok(match v {
            mlua::Value::Nil => {
                self.take(8)?;
                Value::Nil
            }
            mlua::Value::Boolean(b) => {
                self.take(8)?;
                Value::Bool(*b)
            }
            // A Luau integer is 32 bits in a browser, 64 elsewhere.
            #[allow(clippy::useless_conversion)]
            mlua::Value::Integer(i) => {
                self.take(8)?;
                Value::Int(i64::from(*i))
            }
            mlua::Value::Number(n) => {
                self.take(8)?;
                number(*n)
            }
            mlua::Value::String(s) => {
                // Counted before it is copied.
                self.take(s.as_bytes().len())?;
                Value::Str(s.to_string_lossy())
            }
            mlua::Value::Table(t) => {
                let at = t.to_pointer();
                if self.inside.contains(&at) {
                    return Err(mlua::Error::runtime(
                        "a table that holds itself cannot be passed to the host",
                    ));
                }
                self.take(16)?;
                self.inside.push(at);
                let out = self.table(t, depth);
                self.inside.pop();
                out?
            }
            other => {
                return Err(mlua::Error::runtime(format!(
                    "a {} cannot be passed to the host",
                    other.type_name()
                )));
            }
        })
    }

    fn table(&mut self, t: &mlua::Table, depth: usize) -> mlua::Result<Value> {
        let n = t.raw_len();
        let mut entries: Vec<(mlua::Value, mlua::Value)> = Vec::new();
        for pair in t.pairs::<mlua::Value, mlua::Value>() {
            // Each entry is counted as it is read, so a table too large to copy is refused
            // before it is gathered.
            self.take(0)?;
            entries.push(pair?);
        }
        if entries.is_empty() {
            return Ok(Value::List(Vec::new()));
        }
        if n > 0 && entries.len() == n {
            let mut list = Vec::with_capacity(n);
            for i in 1..=n {
                list.push(self.value(&t.raw_get::<mlua::Value>(i)?, depth + 1)?);
            }
            return Ok(Value::List(list));
        }
        let mut map = BTreeMap::new();
        for (k, val) in &entries {
            let key = match k {
                mlua::Value::String(s) => {
                    self.take(s.as_bytes().len())?;
                    s.to_string_lossy()
                }
                mlua::Value::Integer(i) => i.to_string(),
                mlua::Value::Number(n) => match number(*n) {
                    Value::Int(i) => i.to_string(),
                    _ => n.to_string(),
                },
                _ => {
                    return Err(mlua::Error::runtime(
                        "only string and number keys can be passed to the host",
                    ));
                }
            };
            map.insert(key, self.value(val, depth + 1)?);
        }
        Ok(Value::Map(map))
    }
}

/// A Luau number (every number is a double) as an integer when it is one.
fn number(n: f64) -> Value {
    if n.fract() == 0.0 && n.abs() <= 9_007_199_254_740_992.0 {
        #[allow(clippy::cast_possible_truncation)]
        // LINT-OK: integral and within 2^53, so the conversion is exact.
        let i = n as i64;
        Value::Int(i)
    } else {
        Value::Num(n)
    }
}

/// A plain value as a Lua value.
///
/// # Errors
/// Nesting past [`MAX_DEPTH`], or the VM's memory limit.
pub fn from_value(lua: &Lua, v: &Value, depth: usize) -> mlua::Result<mlua::Value> {
    if depth > MAX_DEPTH {
        return Err(mlua::Error::runtime(
            "a value nested too deeply to pass to the plugin",
        ));
    }
    Ok(match v {
        Value::Nil => mlua::Value::Nil,
        Value::Bool(b) => mlua::Value::Boolean(*b),
        #[allow(clippy::cast_precision_loss)]
        Value::Int(i) => mlua::Value::Number(*i as f64),
        Value::Num(n) => mlua::Value::Number(*n),
        Value::Str(s) => mlua::Value::String(lua.create_string(s)?),
        Value::List(l) => {
            let t = lua.create_table_with_capacity(l.len(), 0)?;
            for (i, e) in l.iter().enumerate() {
                t.raw_set(i + 1, from_value(lua, e, depth + 1)?)?;
            }
            mlua::Value::Table(t)
        }
        Value::Map(m) => {
            let t = lua.create_table_with_capacity(0, m.len())?;
            for (k, e) in m {
                t.raw_set(k.as_str(), from_value(lua, e, depth + 1)?)?;
            }
            mlua::Value::Table(t)
        }
    })
}

impl ScriptInstance for LuauInstance {
    fn load(
        &mut self,
        host: &mut dyn HostCalls,
        chunk_name: &str,
        source: &str,
    ) -> Result<(), ScriptError> {
        let budget = Rc::clone(&self.budget);
        let clock = Rc::clone(&self.clock);
        self.enter(host, "load", |lua| {
            let started = clock.now();
            let chunk = lua
                .load(source)
                .set_name(format!("={chunk_name}"))
                .set_mode(ChunkMode::Text)
                .into_function()?;
            budget.extend(clock.now().saturating_sub(started));
            chunk.call::<()>(())
        })?;
        if self.budget.tripped.take() {
            return Err(ScriptError::new(
                ScriptErrorKind::Budget,
                "the plugin ran past its time budget (the script caught the error)".to_owned(),
            ));
        }
        Ok(())
    }

    fn deliver(&mut self, host: &mut dyn HostCalls, event: &Event) -> Result<Value, ScriptError> {
        let dispatch = self.dispatch.clone();
        let name = event.name.clone();
        let data = event.data.clone();
        let label = event.name.clone();
        let outcome = self.enter(
            host,
            &label,
            move |lua| -> mlua::Result<Result<Value, String>> {
                // The two lifecycle hooks are the plugin's own globals, not handlers.
                let hook = match name.as_str() {
                    "plugin.unload" => Some("on_unload"),
                    "plugin.reload" => Some("on_reload"),
                    _ => None,
                };
                if let Some(hook) = hook {
                    let f: Option<Function> = lua.globals().get(hook)?;
                    let Some(f) = f else {
                        return Ok(Ok(Value::Nil));
                    };
                    let arg = from_value(lua, &data, 0)?;
                    return Ok(match f.call::<mlua::Value>(arg) {
                        Ok(v) => Ok(to_value(&v, 0).unwrap_or(Value::Nil)),
                        Err(e) => Err(e.to_string()),
                    });
                }
                let arg = from_value(lua, &data, 0)?;
                let first_error: Option<String> = dispatch.call((name.as_str(), arg))?;
                Ok(first_error.map_or(Ok(Value::Nil), Err))
            },
        )?;
        let outcome = outcome.map_err(|message| {
            if self.budget.tripped.take() {
                ScriptError::new(ScriptErrorKind::Budget, message)
            } else {
                let e = classify(message);
                if e.kind == ScriptErrorKind::Memory {
                    let _ = self.lua.gc_collect();
                }
                e
            }
        });
        // An overrun the script caught within its grace (a task that reported it, a `pcall` that
        // noted it) is still an overrun.
        if outcome.is_ok() && self.budget.tripped.take() {
            return Err(ScriptError::new(
                ScriptErrorKind::Budget,
                "the plugin ran past its time budget (the script caught the error)".to_owned(),
            ));
        }
        outcome
    }

    fn memory_used(&self) -> usize {
        self.lua.used_memory()
    }
}

#[cfg(test)]
mod tests;
