//! Compiled plugins: WebAssembly components against the plugin interface's WIT description, run
//! by a host's own WebAssembly runtime, with no game, no platform and no I/O in it.
//!
//! **Depends on** the interface (`dereth-plugin-api`) for the engine boundary and the API schema;
//! `wasmparser` and `wasm-encoder` to take a component's core module out, check it and rewrite it;
//! `scoped-tls-hkt` to lend the host to the module's imports for one entry; `web-time` for the
//! time the host spends answering them, which is the entry's; and, with the
//! `interpreter` feature (on by default), `wasmi` as the runtime. **Used by** the products that
//! load plugins: the desktop and headless clients run modules in [`interp::Interpreter`], the
//! browser client in the browser's own WebAssembly through its [`Backend`].
//!
//! **Must never** depend on another crate of this workspace or reach a platform (`cargo xtask
//! seams`, `seam: plugin-wasm deps`): a runtime that needs one is a [`Backend`] its product
//! implements.
//!
//! A compiled plugin is a package whose manifest's `entry` is a `.wasm` file: a component (or its
//! core module) targeting the world `plugin` of `docs/plugins/wit/dereth-plugin.wit`. [`module`]
//! prepares it, [`abi`] carries values across, and [`WasmEngine`] is the host's
//! [`ScriptEngine`] for it: `init` runs as a Lua plugin's entry file would, and each event is one
//! call of `deliver`. The same host operations answer both languages.

use std::cell::Cell;
use std::fmt;
use std::rc::Rc;

use web_time::{Duration, Instant};

use dereth_plugin_api::engine::{
    CallError, HostCalls, Limits, ScriptEngine, ScriptError, ScriptErrorKind, ScriptInstance,
    VmClock,
};
use dereth_plugin_api::event::Event;
use dereth_plugin_api::schema::OPS;
use dereth_plugin_api::Value;

pub mod abi;
#[cfg(feature = "interpreter")]
pub mod interp;
pub mod module;

pub use abi::{Flat, FlatTy, Guest};

/// Units of work (function entries and loop iterations) a module may do per millisecond of an
/// entry's budget.
pub const FUEL_PER_MS: i64 = 100_000;

/// A WebAssembly runtime: turns a prepared module into a running instance.
pub trait Backend: fmt::Debug {
    /// The runtime's name, for the plugin console.
    fn name(&self) -> &'static str;

    /// Instantiate `module` (already prepared: [`module::prepare`]). Its imports are the
    /// interface's operations, `imports[i]` the index in [`OPS`] of its `i`-th import; each call
    /// of import `i` must reach [`host_import`]`(imports[i], guest, args)`.
    ///
    /// # Errors
    /// The runtime refused the module.
    fn instantiate(&self, module: &[u8], imports: &[usize]) -> Result<Box<dyn Instance>, String>;
}

/// A running module.
pub trait Instance: Guest {
    /// Call export `name` with `args`.
    ///
    /// # Errors
    /// The module trapped (the text says how).
    fn call(&mut self, name: &str, args: &[Flat]) -> Result<Vec<Flat>, String>;

    /// Whether the module exports `name`.
    fn has_export(&self, name: &str) -> bool;

    /// Set the fuel global ([`module::FUEL_EXPORT`]).
    fn set_fuel(&mut self, fuel: i64);

    /// The fuel left.
    fn fuel(&self) -> i64;

    /// The bytes of its memory.
    fn memory_bytes(&self) -> usize;
}

/// The host lent to a module's imports for one entry, with the time the host has spent answering
/// them: the entry's time as much as the module's own work is.
struct Lent<'a> {
    host: &'a mut (dyn HostCalls + 'a),
    /// What the host's time is read from.
    clock: &'a dyn VmClock,
    /// The host's time on this entry's calls so far, and the most it may be.
    spent: &'a Cell<Duration>,
    budget: Duration,
    /// Set when the host's time ran past the budget.
    over: &'a Cell<bool>,
}

impl<'a, 'b> scoped_tls_hkt::ReborrowMut<'a> for Lent<'b> {
    type Result = Lent<'a>;
    fn reborrow_mut(&'a mut self) -> Self::Result {
        Lent {
            host: self.host,
            clock: self.clock,
            spent: self.spent,
            budget: self.budget,
            over: self.over,
        }
    }
}

scoped_tls_hkt::scoped_thread_local!(static mut HOST: for<'a> Lent<'a>);

/// One call of an interface import: operation `op` (an index in [`OPS`]) with `args`, the
/// module's memory and allocator in `guest`. The answer is written where the module asked; the
/// import itself answers nothing.
///
/// # Errors
/// The call does not fit the interface (a trap).
pub fn host_import(op: usize, guest: &mut dyn Guest, args: &[Flat]) -> Result<Vec<Flat>, String> {
    let op = OPS.get(op).ok_or("no such operation")?;
    if !HOST.is_set() {
        return Err("a host operation outside an entry".into());
    }
    let start = HOST.with(|h| h.clock.now());
    let (values, retptr) = abi::lift_params(op, args, guest)?;
    let answer = if op.name == "console.print" {
        let text = values
            .first()
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        HOST.with(|h| {
            h.host
                .console(&dereth_plugin_api::engine::console_line(&text));
        });
        Ok(Value::Nil)
    } else {
        HOST.with(|h| h.host.call(op.name, &values))
            .map_err(|e| call_error_text(&e))
    };
    abi::store_result(guest, op, retptr, answer)?;
    // The host's time is the entry's: charged to the module's fuel where the runtime lets it be,
    // and the entry stopped once the host's time alone is past the budget.
    let took = HOST.with(|h| h.clock.now().saturating_sub(start));
    guest.spend(fuel_for(took.as_secs_f64() * 1000.0))?;
    let over = HOST.with(|h| {
        h.spent.set(h.spent.get() + took);
        let over = h.spent.get() > h.budget;
        if over {
            h.over.set(true);
        }
        over
    });
    if over {
        return Err("the plugin ran past its time budget in the host's answers".into());
    }
    Ok(Vec::new())
}

fn call_error_text(e: &CallError) -> String {
    e.to_string()
}

/// The machine's monotonic clock: the engine's clock unless it is given another.
#[derive(Debug)]
struct MachineClock {
    start: Instant,
}

impl VmClock for MachineClock {
    fn now(&self) -> Duration {
        self.start.elapsed()
    }
}

/// The engine for compiled plugins, over a runtime.
#[derive(Debug, Clone)]
pub struct WasmEngine {
    backend: Rc<dyn Backend>,
    /// What the host's time answering a module's calls is read from.
    clock: Rc<dyn VmClock>,
}

impl WasmEngine {
    #[must_use]
    pub fn new(backend: Rc<dyn Backend>) -> Self {
        Self {
            backend,
            clock: Rc::new(MachineClock {
                start: Instant::now(),
            }),
        }
    }

    /// The same engine, reading the host's time answering its modules' calls from `clock` rather
    /// than the machine's monotonic clock.
    #[must_use]
    pub fn with_clock(mut self, clock: Rc<dyn VmClock>) -> Self {
        self.clock = clock;
        self
    }

    /// The engine over the interpreter ([`interp::Interpreter`]).
    #[cfg(feature = "interpreter")]
    #[must_use]
    pub fn interpreted() -> Self {
        Self::new(Rc::new(interp::Interpreter::default()))
    }
}

impl ScriptEngine for WasmEngine {
    fn name(&self) -> &'static str {
        self.backend.name()
    }

    fn handles(&self, entry: &str) -> bool {
        entry.to_ascii_lowercase().ends_with(".wasm")
    }

    fn instantiate(&self, limits: Limits) -> Result<Box<dyn ScriptInstance>, ScriptError> {
        Ok(Box::new(WasmInstance {
            backend: self.backend.clone(),
            clock: self.clock.clone(),
            limits,
            running: None,
        }))
    }
}

/// One compiled plugin.
pub struct WasmInstance {
    backend: Rc<dyn Backend>,
    clock: Rc<dyn VmClock>,
    limits: Limits,
    running: Option<Box<dyn Instance>>,
}

impl fmt::Debug for WasmInstance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WasmInstance")
            .field("backend", &self.backend.name())
            .field("running", &self.running.is_some())
            .finish()
    }
}

fn runtime(message: impl Into<String>) -> ScriptError {
    ScriptError::new(ScriptErrorKind::Runtime, message)
}

impl WasmInstance {
    fn max_pages(&self) -> u64 {
        (self.limits.memory_bytes as u64 / 65_536).max(1)
    }

    /// Run `f` against the module with the host lent and the fuel filled; a trap becomes the
    /// failure it was (over the budget, out of memory, or a runtime trap).
    fn enter<R>(
        &mut self,
        host: &mut dyn HostCalls,
        entries: f64,
        f: impl FnOnce(&mut dyn Instance) -> Result<R, String>,
    ) -> Result<R, ScriptError> {
        let max_bytes = usize::try_from(self.max_pages() * 65_536).unwrap_or(usize::MAX);
        let budget_ms = self.limits.call_budget_ms * entries;
        let fuel = fuel_for(budget_ms);
        let m = self
            .running
            .as_deref_mut()
            .ok_or_else(|| runtime("the plugin is not loaded"))?;
        m.set_fuel(fuel);
        let spent = Cell::new(Duration::ZERO);
        let over = Cell::new(false);
        let lent = Lent {
            host,
            clock: &*self.clock,
            spent: &spent,
            budget: Duration::from_secs_f64(budget_ms.max(0.0) / 1000.0),
            over: &over,
        };
        let r = HOST.set(lent, || f(m));
        r.map_err(|trap| {
            if m.fuel() < 0 || over.get() {
                ScriptError::new(ScriptErrorKind::Budget, trap)
            } else if m.memory_bytes() >= max_bytes {
                ScriptError::new(ScriptErrorKind::Memory, trap)
            } else {
                runtime(trap)
            }
        })
    }
}

/// The fuel for a budget of `ms` milliseconds.
#[must_use]
pub fn fuel_for(ms: f64) -> i64 {
    // The budget is a few milliseconds, well inside an i64.
    #[allow(clippy::cast_possible_truncation)]
    let f = (ms.max(0.0) * FUEL_PER_MS as f64) as i64;
    f
}

/// Call an export answering `result<_, string>` or `result<string, string>`, and free what it
/// answered.
fn call_result(
    m: &mut dyn Instance,
    name: &str,
    args: &[Flat],
    ok_string: bool,
) -> Result<Result<String, String>, String> {
    let out = m.call(name, args)?;
    let at = match out.as_slice() {
        [Flat::I32(p)] => p.cast_unsigned(),
        _ => return Err(format!("`{name}` answered {out:?}, not a pointer")),
    };
    let r = abi::read_result(m, at, ok_string)?;
    let post = format!("cabi_post_{name}");
    if m.has_export(&post) {
        m.call(&post, &[Flat::I32(at.cast_signed())])?;
    }
    Ok(r)
}

impl ScriptInstance for WasmInstance {
    fn load(
        &mut self,
        _host: &mut dyn HostCalls,
        chunk_name: &str,
        _source: &str,
    ) -> Result<(), ScriptError> {
        Err(ScriptError::new(
            ScriptErrorKind::Syntax,
            format!("{chunk_name}: a compiled plugin's entry is a .wasm file"),
        ))
    }

    fn load_bytes(
        &mut self,
        host: &mut dyn HostCalls,
        chunk_name: &str,
        bytes: &[u8],
    ) -> Result<(), ScriptError> {
        let syntax =
            |e: String| ScriptError::new(ScriptErrorKind::Syntax, format!("{chunk_name}: {e}"));
        let core = module::core_module(bytes).map_err(syntax)?;
        let prepared = module::prepare(&core, self.max_pages()).map_err(syntax)?;
        let instance = self
            .backend
            .instantiate(&prepared.bytes, &prepared.imports)
            .map_err(syntax)?;
        self.running = Some(instance);
        let entries = dereth_plugin_api::engine::LOAD_BUDGETS;
        match self.enter(host, entries, |m| call_result(m, "init", &[], false))? {
            Ok(_) => Ok(()),
            Err(e) => Err(runtime(e)),
        }
    }

    fn deliver(&mut self, host: &mut dyn HostCalls, event: &Event) -> Result<Value, ScriptError> {
        let data = event.data.to_json();
        let name = event.name.clone();
        let entries = dereth_plugin_api::engine::entry_budgets(&name);
        let answer = self.enter(host, entries, |m| {
            let (np, nl) = abi::put_string(m, &name)?;
            let (dp, dl) = abi::put_string(m, &data)?;
            let args = [np, nl, dp, dl].map(|v| Flat::I32(v.cast_signed()));
            call_result(m, "deliver", &args, true)
        })?;
        match answer {
            Ok(text) if text.is_empty() => Ok(Value::Nil),
            Ok(text) => Value::from_json(&text)
                .map_err(|e| runtime(format!("deliver answered no JSON: {e}"))),
            Err(e) => Err(runtime(e)),
        }
    }

    fn memory_used(&self) -> usize {
        self.running.as_ref().map_or(0, |m| m.memory_bytes())
    }
}

#[cfg(all(test, feature = "interpreter"))]
mod tests;
