//! Where a plugin's time goes, for its author: with [`Limits::profile`](dereth_plugin_api::Limits)
//! set, each VM keeps the time of its entries by event, the time the host spent answering its
//! calls by operation, and the time its own Lua ran by function and by line, and gives the host a
//! table of them for the plugin's console every [`EVERY`].
//!
//! The time is the VM's clock's, read by the VM and handed in (`now`). The Lua time is sampled at
//! the interrupt Luau makes at every loop and call: the time since the last sample goes to the line
//! running then. Time inside a host call is the host's, not the line's. Garbage collection lands
//! on the line that allocated.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;

use mlua::Lua;
use web_time::Duration;

/// How often a VM writes its table.
pub const EVERY: Duration = Duration::from_secs(30);

/// Samples closer together than this are folded into the next, so the stack is read rarely.
const GRAIN: Duration = Duration::from_micros(20);

/// Count, total and longest.
#[derive(Debug, Default, Clone, Copy)]
struct Tally {
    n: u64,
    total: Duration,
    max: Duration,
}

impl Tally {
    fn add(&mut self, d: Duration) {
        self.n += 1;
        self.total += d;
        self.max = self.max.max(d);
    }
}

/// One VM's profile.
#[derive(Debug, Default)]
pub struct Profile {
    since: Cell<Option<Duration>>,
    /// The last sample (or the end of the last host call).
    last: Cell<Option<Duration>>,
    entries: RefCell<BTreeMap<String, Tally>>,
    host: RefCell<BTreeMap<String, Tally>>,
    functions: RefCell<BTreeMap<String, Duration>>,
    lines: RefCell<BTreeMap<String, Duration>>,
}

impl Profile {
    /// An entry begins, at `now`.
    pub fn begin(&self, now: Duration) {
        self.last.set(Some(now));
        if self.since.get().is_none() {
            self.since.set(Some(now));
        }
    }

    /// The interrupt, at `now`: the time since the last sample goes to the line running now.
    pub fn sample(&self, lua: &Lua, now: Duration) {
        let Some(last) = self.last.get() else {
            self.last.set(Some(now));
            return;
        };
        let spent = now.saturating_sub(last);
        if spent < GRAIN {
            return;
        }
        self.last.set(Some(now));
        let here = lua.inspect_stack(0, |d| {
            let src = d
                .source()
                .short_src
                .map(|s| s.into_owned())
                .unwrap_or_default();
            let line = d.current_line().unwrap_or(0);
            let defined = d.source().line_defined.unwrap_or(0);
            let name = d.names().name.map(|n| n.into_owned()).unwrap_or_default();
            (format!("{src}:{defined} {name}"), format!("{src}:{line}"))
        });
        if let Some((function, line)) = here {
            *self.functions.borrow_mut().entry(function).or_default() += spent;
            *self.lines.borrow_mut().entry(line).or_default() += spent;
        }
    }

    /// A host call `op` took `d`, ending at `now`.
    pub fn host_call(&self, op: &str, d: Duration, now: Duration) {
        self.host
            .borrow_mut()
            .entry(op.to_owned())
            .or_default()
            .add(d);
        self.last.set(Some(now));
    }

    /// An entry (`label`) took `d`, ending at `now`; answers the table when one is due (and starts
    /// the next).
    pub fn end(&self, label: &str, d: Duration, memory: usize, now: Duration) -> Option<String> {
        self.entries
            .borrow_mut()
            .entry(label.to_owned())
            .or_default()
            .add(d);
        let since = self.since.get()?;
        let span = now.saturating_sub(since);
        if span < EVERY {
            return None;
        }
        let table = self.table(span, memory);
        self.since.set(None);
        self.entries.borrow_mut().clear();
        self.host.borrow_mut().clear();
        self.functions.borrow_mut().clear();
        self.lines.borrow_mut().clear();
        Some(table)
    }

    pub(crate) fn table(&self, span: Duration, memory: usize) -> String {
        let ms = |d: Duration| d.as_secs_f64() * 1000.0;
        let secs = span.as_secs_f64();
        let entries = self.entries.borrow();
        let total: Duration = entries.values().map(|t| t.total).sum();
        let host: Duration = self.host.borrow().values().map(|t| t.total).sum();
        let mut out = format!(
            "over {secs:.0} s: {:.1} ms in the plugin ({:.2} ms a second), {:.1} ms of it the host's; {} KiB held\n",
            ms(total),
            ms(total) / secs,
            ms(host),
            memory / 1024
        );
        let mut by: Vec<_> = entries.iter().collect();
        by.sort_by_key(|e| std::cmp::Reverse(e.1.total));
        out.push_str("  entries (count, total ms, longest ms):\n");
        for (k, t) in by.iter().take(10) {
            out.push_str(&format!(
                "    {k:<28} {:>7} {:>9.2} {:>7.2}\n",
                t.n,
                ms(t.total),
                ms(t.max)
            ));
        }
        let host = self.host.borrow();
        let mut by: Vec<_> = host.iter().collect();
        by.sort_by_key(|e| std::cmp::Reverse(e.1.total));
        out.push_str("  host calls (count, total ms, longest ms):\n");
        for (k, t) in by.iter().take(12) {
            out.push_str(&format!(
                "    {k:<28} {:>7} {:>9.2} {:>7.2}\n",
                t.n,
                ms(t.total),
                ms(t.max)
            ));
        }
        for (title, map) in [("functions", &self.functions), ("lines", &self.lines)] {
            let map = map.borrow();
            let mut by: Vec<_> = map.iter().collect();
            by.sort_by_key(|e| std::cmp::Reverse(*e.1));
            out.push_str(&format!("  Lua {title} (ms):\n"));
            for (k, d) in by.iter().take(12) {
                out.push_str(&format!("    {k:<40} {:>9.2}\n", ms(**d)));
            }
        }
        out
    }
}
