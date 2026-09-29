//! The soak's host-relative profile. No absolute ceilings: each run writes a profile
//! of what it measured on the host it ran on, and compares it with that host's recorded baseline.
//!
//! - [`Host`] and [`Build`]: what ran where. The host is named only by a hash of its machine id;
//!   no host or user names are ever read.
//! - [`OverrunWindow`]: the one absolute guard, the share of world ticks over the tick interval in
//!   any window of consecutive ticks (default: more than 5% of 3,600 ticks, 60 s of game time).
//! - [`Margins`] and [`compare`]: the regression check against a baseline profile.
//! - [`BaselineFile`]: `target/soak-baselines/<host-hash>.json`, one entry per configuration key.
//!
//! This module needs no real content, so it is built without the `soak` feature (as
//! `empyrean_testkit::soak_profile`) and the unit tier tests it. Nothing here is an ACE port.

use std::collections::VecDeque;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

use crate::soak_metrics::Histogram;

/// The profile's format tag.
pub const PROFILE_FORMAT: &str = "dereth-soak-profile/1";
/// The baseline file's format tag.
pub const BASELINE_FORMAT: &str = "dereth-soak-baselines/1";

/// The machine the run is on. No host or user names: the machine is named by [`Host::machine_hash`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Host {
    /// FNV-1a 64 of the OS's machine id (`/etc/machine-id`, the Windows `MachineGuid`, macOS's
    /// `IOPlatformUUID`), as 16 hex digits; when that cannot be read, of the OS, arch, CPU and core
    /// count instead (prefixed `x`).
    pub machine_hash: String,
    pub os: String,
    pub arch: String,
    pub logical_cores: usize,
    pub ram_mb: Option<u64>,
    pub cpu: Option<String>,
}

/// FNV-1a, 64 bits: stable across Rust versions and machines (std's hasher is not promised to be).
#[must_use]
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

fn command_stdout(program: &str, args: &[&str]) -> Option<String> {
    let out = std::process::Command::new(program)
        .args(args)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// `reg query <key> /v <value>`: the value's data (the text after its type).
#[cfg(windows)]
fn reg_value(key: &str, value: &str) -> Option<String> {
    let out = command_stdout("reg", &["query", key, "/v", value])?;
    let line = out.lines().find(|l| l.trim_start().starts_with(value))?;
    let rest = line.trim_start()[value.len()..].trim_start();
    let data = rest
        .split_once(char::is_whitespace)
        .map(|(_, d)| d.trim())?;
    (!data.is_empty()).then(|| data.to_owned())
}

#[cfg(target_os = "macos")]
fn sysctl(name: &str) -> Option<String> {
    command_stdout("sysctl", &["-n", name])
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
}

fn machine_id() -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        ["/etc/machine-id", "/var/lib/dbus/machine-id"]
            .iter()
            .find_map(|p| std::fs::read_to_string(p).ok())
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
    }
    #[cfg(windows)]
    {
        reg_value(r"HKLM\SOFTWARE\Microsoft\Cryptography", "MachineGuid")
    }
    #[cfg(target_os = "macos")]
    {
        let out = command_stdout("ioreg", &["-rd1", "-c", "IOPlatformExpertDevice"])?;
        let line = out.lines().find(|l| l.contains("IOPlatformUUID"))?;
        line.rsplit('"').nth(1).map(str::to_owned)
    }
    #[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
    {
        None
    }
}

fn ram_mb() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let s = std::fs::read_to_string("/proc/meminfo").ok()?;
        let kb: u64 = s
            .lines()
            .find(|l| l.starts_with("MemTotal:"))?
            .split_whitespace()
            .nth(1)?
            .parse()
            .ok()?;
        Some(kb / 1024)
    }
    #[cfg(windows)]
    {
        let out = command_stdout(
            "powershell",
            &[
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "(Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory",
            ],
        )?;
        out.trim().parse::<u64>().ok().map(|b| b / 1_048_576)
    }
    #[cfg(target_os = "macos")]
    {
        sysctl("hw.memsize")?
            .parse::<u64>()
            .ok()
            .map(|b| b / 1_048_576)
    }
    #[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
    {
        None
    }
}

fn cpu_model() -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        let s = std::fs::read_to_string("/proc/cpuinfo").ok()?;
        let line = s.lines().find(|l| l.starts_with("model name"))?;
        line.split_once(':').map(|(_, v)| v.trim().to_owned())
    }
    #[cfg(windows)]
    {
        reg_value(
            r"HKLM\HARDWARE\DESCRIPTION\System\CentralProcessor\0",
            "ProcessorNameString",
        )
    }
    #[cfg(target_os = "macos")]
    {
        sysctl("machdep.cpu.brand_string")
    }
    #[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
    {
        None
    }
}

/// The processors this process may run on, as the OS reports them (Linux: `Cpus_allowed_list`;
/// Windows: the process affinity mask in hex), so that a `--cores N` run shows it was pinned.
/// `None` on macOS, which has no affinity.
#[must_use]
pub fn process_affinity() -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        let s = std::fs::read_to_string("/proc/self/status").ok()?;
        s.lines()
            .find(|l| l.starts_with("Cpus_allowed_list:"))
            .map(|l| l["Cpus_allowed_list:".len()..].trim().to_owned())
    }
    #[cfg(windows)]
    {
        let cmd = format!(
            "(Get-Process -Id {}).ProcessorAffinity.ToInt64().ToString('X')",
            std::process::id()
        );
        command_stdout(
            "powershell",
            &["-NoProfile", "-NonInteractive", "-Command", &cmd],
        )
        .map(|s| format!("0x{}", s.trim()))
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        None
    }
}

impl Host {
    /// Reads this machine (a few child processes on Windows and macOS; call once per run).
    #[must_use]
    pub fn detect() -> Self {
        let os = std::env::consts::OS.to_owned();
        let arch = std::env::consts::ARCH.to_owned();
        let logical_cores =
            std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
        let cpu = cpu_model();
        let machine_hash = match machine_id() {
            Some(id) => format!("{:016x}", fnv1a64(format!("dereth-soak:{id}").as_bytes())),
            None => format!(
                "x{:015x}",
                fnv1a64(format!("dereth-soak:{os}:{arch}:{cpu:?}:{logical_cores}").as_bytes()) >> 4
            ),
        };
        Self {
            machine_hash,
            os,
            arch,
            logical_cores,
            ram_mb: ram_mb(),
            cpu,
        }
    }

    #[must_use]
    pub fn to_json(&self) -> Value {
        json!({
            "machine_hash": self.machine_hash,
            "os": self.os,
            "arch": self.arch,
            "logical_cores": self.logical_cores,
            "ram_mb": self.ram_mb,
            "cpu": self.cpu,
        })
    }
}

/// The build that ran.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Build {
    /// `git rev-parse --short=12 HEAD` in the crate's directory, if git answers.
    pub commit: Option<String>,
    /// Tracked files differ from the commit.
    pub dirty: bool,
    /// `release` or `debug` (`debug_assertions`).
    pub profile: &'static str,
}

impl Build {
    #[must_use]
    pub fn detect() -> Self {
        let dir = env!("CARGO_MANIFEST_DIR");
        let commit = command_stdout("git", &["-C", dir, "rev-parse", "--short=12", "HEAD"])
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty());
        let dirty = command_stdout(
            "git",
            &["-C", dir, "status", "--porcelain", "--untracked-files=no"],
        )
        .is_some_and(|s| !s.trim().is_empty());
        Self {
            commit,
            dirty,
            profile: if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            },
        }
    }

    #[must_use]
    pub fn to_json(&self) -> Value {
        json!({ "commit": self.commit, "dirty": self.dirty, "profile": self.profile })
    }
}

/// The overrun guard: over any `window` consecutive ticks, the share whose time exceeds
/// `interval_ms` must stay at or below `max_fraction`. Before the first window fills, the count
/// is still divided by the whole window, so a burst at start-up counts as it would later.
#[derive(Debug, Clone)]
pub struct OverrunWindow {
    pub interval_ms: f64,
    pub window: usize,
    pub max_fraction: f64,
    recent: VecDeque<bool>,
    in_window: usize,
    /// Ticks seen, and ticks over the interval.
    pub ticks: u64,
    pub over: u64,
    /// Milliseconds past the interval, summed over the ticks that overran.
    pub over_ms: f64,
    /// The worst window's count, and the tick (1-based) that ended it.
    pub worst: usize,
    pub worst_end: u64,
}

impl OverrunWindow {
    /// The default guard: 16.67 ms (the world's 60 Hz), more than 5% of 3,600 ticks (60 s).
    pub const DEFAULT_INTERVAL_MS: f64 = 1000.0 / 60.0;
    pub const DEFAULT_WINDOW: usize = 3600;
    pub const DEFAULT_MAX_FRACTION: f64 = 0.05;

    #[must_use]
    pub fn new(interval_ms: f64, window: usize, max_fraction: f64) -> Self {
        let window = window.max(1);
        Self {
            interval_ms,
            window,
            max_fraction,
            recent: VecDeque::with_capacity(window),
            in_window: 0,
            ticks: 0,
            over: 0,
            over_ms: 0.0,
            worst: 0,
            worst_end: 0,
        }
    }

    /// One tick of `ms`.
    pub fn push(&mut self, ms: f64) {
        let over = ms > self.interval_ms;
        self.ticks += 1;
        if over {
            self.over += 1;
            self.over_ms += ms - self.interval_ms;
            self.in_window += 1;
        }
        self.recent.push_back(over);
        if self.recent.len() > self.window && self.recent.pop_front() == Some(true) {
            self.in_window -= 1;
        }
        if self.in_window > self.worst {
            self.worst = self.in_window;
            self.worst_end = self.ticks;
        }
    }

    /// The worst window's share of ticks over the interval.
    #[must_use]
    pub fn worst_fraction(&self) -> f64 {
        #[allow(clippy::cast_precision_loss)]
        let f = self.worst as f64 / self.window as f64;
        f
    }

    /// The share of all ticks over the interval.
    #[must_use]
    pub fn over_fraction(&self) -> f64 {
        #[allow(clippy::cast_precision_loss)]
        let f = if self.ticks == 0 {
            0.0
        } else {
            self.over as f64 / self.ticks as f64
        };
        f
    }

    /// The guard tripped: some window had more than `max_fraction` of its ticks over.
    #[must_use]
    pub fn breached(&self) -> bool {
        self.worst_fraction() > self.max_fraction
    }
}

impl Default for OverrunWindow {
    fn default() -> Self {
        Self::new(
            Self::DEFAULT_INTERVAL_MS,
            Self::DEFAULT_WINDOW,
            Self::DEFAULT_MAX_FRACTION,
        )
    }
}

/// One timed system (a performance-monitor stage or section) or one landblock over the run.
#[derive(Debug, Clone, PartialEq)]
pub struct Timed {
    pub name: String,
    pub count: u64,
    pub total_ms: f64,
    pub p50_ms: f64,
    pub p99_ms: f64,
    pub max_ms: f64,
}

impl Timed {
    #[must_use]
    pub fn from_histogram(name: &str, h: &Histogram) -> Self {
        Self {
            name: name.to_owned(),
            count: h.count,
            total_ms: h.sum_ms,
            p50_ms: h.quantile_ms(0.5),
            p99_ms: h.quantile_ms(0.99),
            max_ms: h.max_ms,
        }
    }

    #[must_use]
    pub fn to_json(&self) -> Value {
        json!({ "name": self.name, "count": self.count, "total_ms": round3(self.total_ms), "p50_ms": self.p50_ms, "p99_ms": self.p99_ms, "max_ms": round3(self.max_ms) })
    }
}

/// Where the soak loop's wall time went. The bots, their transports and the probe run in
/// the same process and on the same thread as the world, one after another, so on a small host their
/// time lengthens the run but is never inside the server tick. What is not the server tick is the
/// harness's: the bots' step ([`HarnessTime::bots_s`]), the clients' side of `TestServer::step`
/// (the step less the server tick), and the soak's own instrumentation (probe, samples, scans).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct HarnessTime {
    /// The whole soak loop, wall seconds.
    pub loop_s: f64,
    /// The server tick summed over the run (the six `WorldManager.UpdateWorld` stages).
    pub server_tick_s: f64,
    /// `TestServer::step` summed: the clients' transports, the world tick and the delivery.
    pub step_s: f64,
    /// The bots' own step: read, decide, act, send, and the relog checks.
    pub bots_s: f64,
    /// The soak's probe, samples and scans.
    pub instrumentation_s: f64,
    /// The process's CPU time (user and system, every thread) over the loop, where the OS gives it.
    pub process_cpu_s: Option<f64>,
}

impl HarnessTime {
    /// Everything that is not the server tick.
    #[must_use]
    pub fn harness_s(&self) -> f64 {
        (self.loop_s - self.server_tick_s).max(0.0)
    }

    /// The clients' side of `TestServer::step` (the step less the server tick).
    #[must_use]
    pub fn clients_s(&self) -> f64 {
        (self.step_s - self.server_tick_s).max(0.0)
    }

    /// The loop's rest (the bookkeeping between the parts timed).
    #[must_use]
    pub fn other_s(&self) -> f64 {
        (self.loop_s - self.step_s - self.bots_s - self.instrumentation_s).max(0.0)
    }

    /// The harness's share of the loop, 0 to 1.
    #[must_use]
    pub fn share(&self) -> f64 {
        if self.loop_s > 0.0 {
            self.harness_s() / self.loop_s
        } else {
            0.0
        }
    }

    #[must_use]
    pub fn to_json(&self) -> Value {
        json!({
            "loop_s": round3(self.loop_s),
            "server_tick_s": round3(self.server_tick_s),
            "harness_s": round3(self.harness_s()),
            "harness_share": round3(self.share()),
            "bots_s": round3(self.bots_s),
            "clients_s": round3(self.clients_s()),
            "instrumentation_s": round3(self.instrumentation_s),
            "other_s": round3(self.other_s()),
            "process_cpu_s": self.process_cpu_s.map(round3),
        })
    }

    /// One Markdown line.
    #[must_use]
    pub fn line(&self) -> String {
        let pct = |x: f64| {
            if self.loop_s > 0.0 {
                x / self.loop_s * 100.0
            } else {
                0.0
            }
        };
        let mut s = format!(
            "harness share {:.1}% of the loop's {:.0} s wall (server tick {:.1}%; bots {:.1}%, clients' transports {:.1}%, instrumentation {:.1}%, other {:.1}%)",
            self.share() * 100.0,
            self.loop_s,
            pct(self.server_tick_s),
            pct(self.bots_s),
            pct(self.clients_s()),
            pct(self.instrumentation_s),
            pct(self.other_s())
        );
        if let Some(cpu) = self.process_cpu_s {
            let _ = write!(s, "; process CPU {cpu:.0} s ({:.2} cores busy on average: one thread runs world and bots in turn)", if self.loop_s > 0.0 { cpu / self.loop_s } else { 0.0 });
        }
        s
    }
}

/// The process's CPU time so far (user and system, all threads), in seconds: `/proc/self/stat` on
/// Linux (clock ticks at `USER_HZ` 100), `Get-Process` on Windows; `None` elsewhere.
#[must_use]
pub fn process_cpu_seconds() -> Option<f64> {
    #[cfg(target_os = "linux")]
    {
        let s = std::fs::read_to_string("/proc/self/stat").ok()?;
        // the fields after the command name (which may hold spaces) start at state (field 3)
        let rest = &s[s.rfind(')')? + 2..];
        let f: Vec<&str> = rest.split_whitespace().collect();
        let (utime, stime): (f64, f64) = (f.get(11)?.parse().ok()?, f.get(12)?.parse().ok()?);
        Some((utime + stime) / 100.0)
    }
    #[cfg(windows)]
    {
        let cmd = format!(
            "(Get-Process -Id {}).TotalProcessorTime.TotalSeconds",
            std::process::id()
        );
        command_stdout(
            "powershell",
            &["-NoProfile", "-NonInteractive", "-Command", &cmd],
        )
        .and_then(|s| s.trim().replace(',', ".").parse().ok())
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        None
    }
}

/// The ten largest by total time, and the ten largest by p99.
#[must_use]
pub fn top10(items: &[Timed]) -> (Vec<Timed>, Vec<Timed>) {
    let mut by_total: Vec<Timed> = items.iter().filter(|t| t.count > 0).cloned().collect();
    by_total.sort_by(|a, b| {
        b.total_ms
            .total_cmp(&a.total_ms)
            .then_with(|| a.name.cmp(&b.name))
    });
    let mut by_p99 = by_total.clone();
    by_p99.sort_by(|a, b| {
        b.p99_ms
            .total_cmp(&a.p99_ms)
            .then_with(|| b.total_ms.total_cmp(&a.total_ms))
    });
    by_total.truncate(10);
    by_p99.truncate(10);
    (by_total, by_p99)
}

fn round3(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

/// The figures a baseline is compared on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Headline {
    /// The server tick's p99 (the six `WorldManager.UpdateWorld` stages), ms.
    pub p99_tick_ms: f64,
    /// Resident memory growth per bot-hour of play, MB.
    pub growth_mb_per_bot_hour: f64,
    /// Messages the bots received per wall second.
    pub msgs_per_s: f64,
}

impl Headline {
    /// Reads the headline figures of a profile (or a baseline entry).
    #[must_use]
    pub fn from_json(v: &Value) -> Option<Self> {
        Some(Self {
            p99_tick_ms: v.pointer("/tick/p99_ms")?.as_f64()?,
            growth_mb_per_bot_hour: v.pointer("/memory/growth_mb_per_bot_hour")?.as_f64()?,
            msgs_per_s: v.pointer("/messages/per_s")?.as_f64()?,
        })
    }
}

/// How far a run may move from its baseline before it is flagged. The fractions are relative to
/// the baseline; the floors are absolute and keep noise on tiny values from being flagged.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Margins {
    /// p99 tick may grow by this fraction (default 0.25).
    pub p99_tick: f64,
    /// Memory growth per bot-hour may grow by this fraction (default 0.20).
    pub mem_growth: f64,
    /// Messages per second may fall by this fraction (default 0.20).
    pub msgs: f64,
    /// ...and the p99 must also have grown by more than this many ms (default 0.25: sub-millisecond
    /// ticks jitter by more than 25% from run to run).
    pub p99_floor_ms: f64,
    /// ...and memory growth by more than this many MB per bot-hour (default 1).
    pub mem_floor_mb: f64,
}

impl Default for Margins {
    fn default() -> Self {
        Self {
            p99_tick: 0.25,
            mem_growth: 0.20,
            msgs: 0.20,
            p99_floor_ms: 0.25,
            mem_floor_mb: 1.0,
        }
    }
}

impl Margins {
    /// The defaults, overridden by `SOAK_MARGIN_P99`, `SOAK_MARGIN_MEM` and `SOAK_MARGIN_MSGS`
    /// (percentages).
    #[must_use]
    pub fn from_env() -> Self {
        let pct = |k: &str| {
            std::env::var(k)
                .ok()
                .and_then(|v| v.parse::<f64>().ok())
                .map(|p| p / 100.0)
        };
        let d = Self::default();
        Self {
            p99_tick: pct("SOAK_MARGIN_P99").unwrap_or(d.p99_tick),
            mem_growth: pct("SOAK_MARGIN_MEM").unwrap_or(d.mem_growth),
            msgs: pct("SOAK_MARGIN_MSGS").unwrap_or(d.msgs),
            ..d
        }
    }
}

/// One compared figure.
#[derive(Debug, Clone, PartialEq)]
pub struct Finding {
    pub metric: &'static str,
    pub baseline: f64,
    pub current: f64,
    /// `(current - baseline) / |baseline|`; 0 when the baseline is 0.
    pub change: f64,
    /// The allowed change (signed: negative for a figure that may only fall so far).
    pub allowed: f64,
    pub regressed: bool,
}

fn change(base: f64, cur: f64) -> f64 {
    if base == 0.0 {
        0.0
    } else {
        (cur - base) / base.abs()
    }
}

/// Compares a run with its baseline.
///
/// - p99 tick: flagged above `baseline * (1 + p99_tick)` and more than `p99_floor_ms` above it;
/// - memory growth: flagged above `baseline + |baseline| * mem_growth` and more than `mem_floor_mb` above it;
/// - messages per second: flagged below `baseline * (1 - msgs)`.
#[must_use]
pub fn compare(base: &Headline, cur: &Headline, m: &Margins) -> Vec<Finding> {
    let p99 = cur.p99_tick_ms > base.p99_tick_ms * (1.0 + m.p99_tick)
        && cur.p99_tick_ms - base.p99_tick_ms > m.p99_floor_ms;
    let mem = cur.growth_mb_per_bot_hour
        > base.growth_mb_per_bot_hour + base.growth_mb_per_bot_hour.abs() * m.mem_growth
        && cur.growth_mb_per_bot_hour - base.growth_mb_per_bot_hour > m.mem_floor_mb;
    let msgs = base.msgs_per_s > 0.0 && cur.msgs_per_s < base.msgs_per_s * (1.0 - m.msgs);
    vec![
        Finding {
            metric: "p99 tick ms",
            baseline: base.p99_tick_ms,
            current: cur.p99_tick_ms,
            change: change(base.p99_tick_ms, cur.p99_tick_ms),
            allowed: m.p99_tick,
            regressed: p99,
        },
        Finding {
            metric: "memory growth MB per bot-hour",
            baseline: base.growth_mb_per_bot_hour,
            current: cur.growth_mb_per_bot_hour,
            change: change(base.growth_mb_per_bot_hour, cur.growth_mb_per_bot_hour),
            allowed: m.mem_growth,
            regressed: mem,
        },
        Finding {
            metric: "messages per s",
            baseline: base.msgs_per_s,
            current: cur.msgs_per_s,
            change: change(base.msgs_per_s, cur.msgs_per_s),
            allowed: -m.msgs,
            regressed: msgs,
        },
    ]
}

/// The comparison as a Markdown table.
#[must_use]
pub fn findings_table(findings: &[Finding]) -> String {
    let mut s = String::from(
        "| figure | baseline | this run | change | allowed | |\n|---|---|---|---|---|---|\n",
    );
    for f in findings {
        let _ = writeln!(
            s,
            "| {} | {:.3} | {:.3} | {:+.1}% | {:+.0}% | {} |",
            f.metric,
            f.baseline,
            f.current,
            f.change * 100.0,
            f.allowed * 100.0,
            if f.regressed { "**REGRESSION**" } else { "ok" }
        );
    }
    s
}

/// The key a configuration is stored under: `<run name>/<build profile>`, plus `/cores<N>` for a
/// pinned (modest-host) run.
#[must_use]
pub fn config_key(name: &str, profile: &str, cores: Option<usize>) -> String {
    match cores {
        Some(n) => format!("{name}/{profile}/cores{n}"),
        None => format!("{name}/{profile}"),
    }
}

/// `target/soak-baselines/<host-hash>.json`: a host's baselines, one per configuration key.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BaselineFile {
    pub host: Value,
    pub entries: Map<String, Value>,
}

impl BaselineFile {
    /// The file for `machine_hash` under `dir`.
    #[must_use]
    pub fn path(dir: &Path, machine_hash: &str) -> PathBuf {
        dir.join(format!("{machine_hash}.json"))
    }

    /// Reads a baseline file; `None` when it is absent or not a baseline file.
    #[must_use]
    pub fn read(path: &Path) -> Option<Self> {
        let v: Value = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
        (v.get("format")?.as_str()? == BASELINE_FORMAT).then_some(())?;
        Some(Self {
            host: v.get("host").cloned().unwrap_or(Value::Null),
            entries: v.get("entries")?.as_object()?.clone(),
        })
    }

    #[must_use]
    pub fn to_json(&self) -> Value {
        json!({ "format": BASELINE_FORMAT, "host": self.host, "entries": self.entries })
    }

    /// Writes the file (creating its directory).
    ///
    /// # Errors
    /// When the directory or the file cannot be written.
    pub fn write(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = serde_json::to_string_pretty(&self.to_json()).map_err(std::io::Error::other)?;
        std::fs::write(path, text + "\n")
    }
}
