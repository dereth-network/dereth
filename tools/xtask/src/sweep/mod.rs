//! `cargo xtask sweep`: run a set of cargo test targets and report a verdict that cannot read as
//! silence, sharding the serialised tier binaries across processes.
//!
//! ```text
//! cargo xtask sweep                          every discovered target and every lib
//! cargo xtask sweep --touched world.rs       targets whose source mentions a string
//! cargo xtask sweep rendering::sky o604_vendor_money
//! cargo xtask sweep --list                   the registry, and exit
//! cargo xtask sweep --shard-binary dereth-client:dat          one binary, 4 processes
//! cargo xtask sweep --shard-binary dereth-client:dat --serial the same, in one
//! cargo xtask sweep --shard-binary dereth-client:gpu          one process per 2 cores
//! cargo xtask sweep --debug rendering::sky   the dev profile, to debug a failure
//! ```
//!
//! Two failures this workspace kept repeating, and what the sweep does about them:
//!
//! 1. **A suite named in the wrong package emits no `test result:` line.** `cargo test -p <pkg>
//!    --test <name>` for a target that is not in that package prints `error: no test target named
//!    ...` and nothing else. A sweep that greps for result lines sees nothing where it expected a
//!    pass, and in a long run a blank row reads as fine. That cost at least seven false "main is
//!    green" claims, one of which hid a red suite for days. The fix is structural: package names
//!    are never typed here. Every target is discovered by walking the crates' `tests/*.rs` and
//!    `tests/<tier>/...`, so a suite's package and tier come from where its file lives, and naming
//!    a suite that does not exist is a hard error, not a blank line.
//! 2. **A sweep without a denominator cannot tell green from never-ran.** Every run counts one
//!    result line per target it launched, and says so in the summary even when everything passed.
//!    A step that could not run is a failure, never a skip.
//!
//! The standing red list is [`known_red::KNOWN_RED`]; see that module for the rule that a fixed
//! known red fails the sweep too.
//!
//! Every target builds and runs in the `test-release` profile, as the rest of `cargo xtask` does;
//! `--debug` selects the dev profile, for investigating one failure.
//!
//! **Sharding.** The serialised tier binaries -- `dereth-client`'s and `dereth-testkit`'s `dat` (a
//! heap defect) and `dereth-client`'s `gpu` (a device lock inside one process) -- run their tests
//! one at a time. They are run as N single-threaded processes over disjoint module slices, which is
//! the same one thread per process and a fraction of the wall clock (see [`shard`]). `--shards N`
//! or `DERETH_TEST_SHARDS=N` sets N, `--serial` forces the one-process form, and every shard's exit
//! code is checked. Every sharded run times each test from libtest's output and writes
//! `sweep/timings/<pkg>-<tier>.tsv` and the shard logs under `sweep/` in cargo's target
//! directory; the deal balances on recorded time where a committed file exists
//! (`tools/sweep-timings/`), on test count otherwise. `DERETH_TEST_GPU` and every other variable
//! pass through to the test processes unchanged.
//!
//! `DERETH_TEST_DAT_DIR` is left alone: the tests' own lookup already falls back to the main
//! checkout's install when a git worktree has none. `DERETH_TEST_FFMPEG` is resolved when unset,
//! because a package manager can install `ffmpeg` where a noninteractive shell has no PATH to it,
//! and the audio decode oracle then read as an unexpected red on a tree where nothing was wrong
//! (measured: 5 passed / 1 failed bare, 6 / 0 with the variable pointed at the installed binary). A
//! missing ffmpeg still fails loudly with the test's own message.

pub mod known_red;
pub mod shard;

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant, SystemTime};

use regex::Regex;
use std::sync::LazyLock;

use crate::util::{target_dir, workspace_root, Profile};
use known_red::{known_red, score, Score, KNOWN_NO_TESTS, KNOWN_RED};
use shard::{attribute_all, module_of, run_sharded, shard_count, By, Sharded, Summary};

/// Where the sweep's lines go: straight to stdout, or into a buffer a caller prints only on
/// failure (the gates read a sharded run the way they read a captured `cargo test`).
#[derive(Debug, Default)]
pub struct Out {
    buf: Option<String>,
}

impl Out {
    pub fn print() -> Self {
        Out { buf: None }
    }

    pub fn capture() -> Self {
        Out {
            buf: Some(String::new()),
        }
    }

    pub fn say(&mut self, line: impl AsRef<str>) {
        match &mut self.buf {
            Some(b) => {
                b.push_str(line.as_ref());
                b.push('\n');
            }
            None => {
                println!("{}", line.as_ref());
                let _ = std::io::stdout().flush();
            }
        }
    }

    pub fn into_text(self) -> String {
        self.buf.unwrap_or_default()
    }
}

/// The tiers a crate's consolidated tests live in: one binary per tier, `tests/<tier>/main.rs`,
/// one module per former `tests/<stem>.rs`. `local` is the `dat` tier by another name for crates
/// with private-data tests and no retail dat; it must be listed, because a tier this list does not
/// name is a tier the sweep silently does not run.
pub const TIERS: &[&str] = &["cpu", "dat", "gpu", "local"];

/// The client and shared crates the sweep covers, package `dereth-<dir>`; the server's crates have
/// their own gate.
const CRATE_GROUPS: &[&str] = &["core", "dereth", "dereth/client/crates"];

/// Every package the sweep covers, by name, sorted.
pub fn packages(ws: &Path) -> Vec<String> {
    let mut out = Vec::new();
    for group in CRATE_GROUPS {
        for e in std::fs::read_dir(ws.join(group))
            .into_iter()
            .flatten()
            .flatten()
        {
            if e.path().join("Cargo.toml").is_file() {
                if let Some(n) = e.file_name().to_str() {
                    out.push(crate::util::crate_name(group, n));
                }
            }
        }
    }
    out.sort();
    out
}

/// The directory of package `pkg` (`dereth-<short>` lives in `<group>/<short>`, but for the
/// crates `crate::util::crate_dir` names otherwise).
pub fn crate_dir(ws: &Path, pkg: &str) -> PathBuf {
    if crate::util::NAMED_OTHERWISE
        .iter()
        .any(|(_, name)| *name == pkg)
    {
        return crate::util::crate_dir(ws, pkg);
    }
    let short = pkg.strip_prefix("dereth-").unwrap_or(pkg);
    CRATE_GROUPS
        .iter()
        .map(|g| ws.join(g).join(short))
        .find(|d| d.join("Cargo.toml").is_file())
        .unwrap_or_else(|| ws.join(CRATE_GROUPS[0]).join(short))
}

/// One target the sweep can run: a lib (`suite` of `None`), a legacy `tests/<stem>.rs` target
/// (`tier` of `None`) or one module of a tier binary.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Pick {
    pub pkg: String,
    pub suite: Option<String>,
    pub tier: Option<String>,
}

impl Pick {
    fn lib(pkg: &str) -> Self {
        Pick {
            pkg: pkg.to_owned(),
            suite: None,
            tier: None,
        }
    }

    /// `pkg::suite`, the name the red lists and the output use.
    pub fn name(&self) -> String {
        format!("{}::{}", self.pkg, self.suite.as_deref().unwrap_or("lib"))
    }

    /// [`Pick::name`], with the tier when the same module name lives in two tiers of one package.
    fn shown(&self, reg: &Registry) -> String {
        match (&self.suite, &self.tier) {
            (Some(s), Some(t)) if reg.tiers_of(&self.pkg, s) > 1 => {
                format!("{} [{t}]", self.name())
            }
            _ => self.name(),
        }
    }
}

/// Where a suite lives: its package, and its tier (`None` for a legacy target).
pub type Loc = (String, Option<String>);

/// Every integration target, keyed by suite name, valued by every `(package, tier)` that holds a
/// file of that name.
///
/// The value is a list because some names really do exist in two crates, or in two tiers of one
/// crate (a `dat` and a `gpu` module of the same stem). Keeping the last one seen would be the
/// instrument picking a member of its space and not saying -- one tier's module would silently
/// never run. A duplicate is run in every place it lives when sweeping, and is a hard error when
/// named explicitly.
#[derive(Debug, Default)]
pub struct Registry {
    pub targets: BTreeMap<String, Vec<Loc>>,
}

fn rs_files(dir: &Path) -> Vec<(String, bool)> {
    let mut out: Vec<(String, bool)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| Some((e.file_name().to_str()?.to_owned(), e.path().is_dir())))
        .collect();
    out.sort();
    out
}

pub fn discover(ws: &Path) -> Registry {
    let mut reg = Registry::default();
    let mut add = |name: String, pkg: &str, tier: Option<&str>| {
        reg.targets
            .entry(name)
            .or_default()
            .push((pkg.to_owned(), tier.map(str::to_owned)));
    };
    for pkg in packages(ws) {
        let tdir = crate_dir(ws, &pkg).join("tests");
        if !tdir.is_dir() {
            continue;
        }
        for (f, dir) in rs_files(&tdir) {
            if let Some(stem) = f.strip_suffix(".rs").filter(|_| !dir) {
                add(stem.to_owned(), &pkg, None);
            }
        }
        // `tests/<tier>/<stem>.rs`, one module per former target; `main.rs` is the binary and
        // `common/` its shared helpers, neither a suite. An area directory,
        // `tests/<tier>/<area>/<stem>.rs`, gives the suite `<area>::<stem>`, which is also its
        // filter.
        for tier in TIERS {
            let ddir = tdir.join(tier);
            if !ddir.is_dir() {
                continue;
            }
            for (f, dir) in rs_files(&ddir) {
                if !dir {
                    if let Some(stem) = f.strip_suffix(".rs").filter(|s| *s != "main") {
                        add(stem.to_owned(), &pkg, Some(tier));
                    }
                } else if f != "common" {
                    for (g, gdir) in rs_files(&ddir.join(&f)) {
                        if let Some(stem) = g.strip_suffix(".rs").filter(|s| !gdir && *s != "mod") {
                            add(format!("{f}::{stem}"), &pkg, Some(tier));
                        }
                    }
                }
            }
        }
    }
    reg
}

impl Registry {
    /// Every place a suite name lives, as picks.
    fn picks(&self, suite: &str) -> Vec<Pick> {
        self.targets
            .get(suite)
            .into_iter()
            .flatten()
            .map(|(pkg, tier)| Pick {
                pkg: pkg.clone(),
                suite: Some(suite.to_owned()),
                tier: tier.clone(),
            })
            .collect()
    }

    /// How many tiers of `pkg` hold a module named `suite`.
    fn tiers_of(&self, pkg: &str, suite: &str) -> usize {
        self.targets
            .get(suite)
            .map_or(0, |locs| locs.iter().filter(|(p, _)| p == pkg).count())
    }

    /// The source file of a target, wherever the layout put it.
    pub fn suite_path(ws: &Path, pick: &Pick) -> PathBuf {
        let tests = crate_dir(ws, &pick.pkg).join("tests");
        let suite = pick.suite.as_deref().unwrap_or("");
        match &pick.tier {
            Some(tier) => {
                let mut p = tests.join(tier);
                for part in suite.split("::") {
                    p = p.join(part);
                }
                p.with_extension("rs")
            }
            None => tests.join(format!("{suite}.rs")),
        }
    }

    /// A target "touches the GPU" if it is in a `gpu` binary or its source reaches for a device by
    /// any of [`GPU_MARKERS`]. A lib does not.
    fn touches_gpu(ws: &Path, pick: &Pick) -> bool {
        if pick.suite.is_none() {
            return false;
        }
        if pick.tier.as_deref() == Some("gpu") {
            return true;
        }
        let src = std::fs::read(Self::suite_path(ws, pick))
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or_default();
        GPU_MARKERS.iter().any(|m| src.contains(m))
    }
}

/// The routes by which a target reaches a graphics device.
///
/// The in-process device lock serialises tests inside one binary and does nothing across
/// processes, so two worktrees sweeping at once contend for one adapter; under three concurrent
/// sweeps, device targets were killed mid-run (exit -1, partial output, no result line). That is
/// not a defect in the suite and must not be swept into a red list, hence `--no-gpu`, which states
/// its own smaller denominator. Every marker is case-sensitive, and the authoritative one is the
/// crate feature gate, spelled in lowercase: 67 device-holding targets once carried only that gate,
/// and `--no-gpu` handed all of them to the lane it advertised as adapter-free.
pub const GPU_MARKERS: &[&str] = &[
    "gpu_lock",
    "D3D12",
    "headless_app",
    "Device",
    "dereth_render::",
    "feature = \"d3d12\"",
    "feature=\"d3d12\"",
    "feature = \"vulkan\"",
];

/// The packages with a lib target.
pub fn libs(ws: &Path) -> Vec<String> {
    packages(ws)
        .into_iter()
        .filter(|p| crate_dir(ws, p).join("src").join("lib.rs").is_file())
        .collect()
}

/// A per-target run's numbers, in the shape a sharded run is attributed into too.
#[derive(Debug, Clone)]
pub struct Row {
    /// `None`: no result line at all, the target did not run.
    pub passed: Option<usize>,
    pub failed: usize,
    pub lines: usize,
    pub blob: String,
    pub secs: f64,
    pub names: Vec<String>,
}

static RESULT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)test result: (\w+)\. (\d+) passed; (\d+) failed").expect("RESULT")
});

/// `(passed, failed)` summed over every result line of a run, or `None` when there is none.
pub fn parse(blob: &str) -> Option<(usize, usize)> {
    let hits: Vec<regex::Captures> = RESULT.captures_iter(blob).collect();
    if hits.is_empty() {
        return None;
    }
    let sum = |i: usize| {
        hits.iter()
            .map(|h| h[i].parse::<usize>().unwrap_or(0))
            .sum()
    };
    Some((sum(2), sum(3)))
}

/// One `[env]` entry of a Cargo configuration file: its value (made absolute when the entry says
/// `relative`) and whether it is `force`d over a value already in the environment.
pub type ConfigEnv = BTreeMap<String, (String, bool)>;

/// The `[env]` tables of the Cargo configuration files cargo reads from `start` and every
/// directory above it, as cargo applies them to a test it runs: the nearest file's value wins,
/// `relative = true` makes a value relative to the directory holding that file's `.cargo/`, and a
/// value already in the environment wins unless the entry says `force` (see [`sweep_env`]).
///
/// A shard runs its test binary directly rather than through cargo, so without this it would not
/// see a variable set in `.cargo/config.toml` -- the retail dats' location among them.
pub fn cargo_config_env(start: &Path) -> ConfigEnv {
    let mut found = ConfigEnv::new();
    let mut dir = Some(start);
    while let Some(d) = dir {
        for name in ["config.toml", "config"] {
            let path = d.join(".cargo").join(name);
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let table = text
                .parse::<toml::Table>()
                .ok()
                .and_then(|t| t.get("env").and_then(toml::Value::as_table).cloned())
                .unwrap_or_default();
            for (key, entry) in table {
                if found.contains_key(&key) {
                    continue;
                }
                let plain = |v: &toml::Value| match v {
                    toml::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                let value = match &entry {
                    toml::Value::Table(t) => {
                        let value = t.get("value").map(plain).unwrap_or_default();
                        let relative =
                            t.get("relative").and_then(toml::Value::as_bool) == Some(true);
                        let force = t.get("force").and_then(toml::Value::as_bool) == Some(true);
                        let value = if relative {
                            d.join(value).to_string_lossy().into_owned()
                        } else {
                            value
                        };
                        (value, force)
                    }
                    other => (plain(other), false),
                };
                found.insert(key, value);
            }
            break;
        }
        dir = d.parent();
    }
    found
}

/// What the sweep adds to the environment of every run: the Cargo configuration's `[env]` entries
/// the environment does not already set (or that are forced), and `DERETH_TEST_FFMPEG` when it is
/// unset and an installed `ffmpeg` can be found.
pub fn sweep_env() -> Vec<(String, String)> {
    env_over(&cargo_config_env(&workspace_root()), |k| {
        std::env::var_os(k).is_some()
    })
}

/// [`sweep_env`] over a given configuration and environment.
fn env_over(config: &ConfigEnv, is_set: impl Fn(&str) -> bool) -> Vec<(String, String)> {
    let mut env: Vec<(String, String)> = config
        .iter()
        .filter(|(k, (_, force))| *force || !is_set(k))
        .map(|(k, (v, _))| (k.clone(), v.clone()))
        .collect();
    if !is_set("DERETH_TEST_FFMPEG") && !env.iter().any(|(k, _)| k == "DERETH_TEST_FFMPEG") {
        if let Some(found) = find_ffmpeg() {
            env.push((
                "DERETH_TEST_FFMPEG".to_owned(),
                found.to_string_lossy().into_owned(),
            ));
        }
    }
    env
}

/// The `ffmpeg` the audio decode oracle needs, or `None`.
///
/// PATH first, because that is the answer the test would have found by itself; then the roots an
/// install actually lands in. `None` rather than a guess, so a machine with no ffmpeg still gets
/// the test's own loud failure instead of a bad path.
pub fn find_ffmpeg() -> Option<PathBuf> {
    let exe = if cfg!(windows) {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    };
    if let Some(path) = std::env::var_os("PATH") {
        for d in std::env::split_paths(&path) {
            let p = d.join(exe);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    if let Some(la) = std::env::var_os("LOCALAPPDATA") {
        let pkgs = PathBuf::from(la)
            .join("Microsoft")
            .join("WinGet")
            .join("Packages");
        let mut hits = Vec::new();
        for e in std::fs::read_dir(&pkgs).into_iter().flatten().flatten() {
            if !e.file_name().to_string_lossy().starts_with("Gyan.FFmpeg") {
                continue;
            }
            for v in std::fs::read_dir(e.path()).into_iter().flatten().flatten() {
                let p = v.path().join("bin").join("ffmpeg.exe");
                if p.is_file() {
                    hits.push(p);
                }
            }
        }
        hits.sort();
        if let Some(p) = hits.pop() {
            return Some(p);
        }
    }
    [
        r"C:\ProgramData\chocolatey\bin\ffmpeg.exe",
        r"C:\ffmpeg\bin\ffmpeg.exe",
    ]
    .iter()
    .map(PathBuf::from)
    .find(|p| p.is_file())
}

/// A number printed by a command, or `None`.
fn command_number(program: &str, args: &[&str]) -> Option<f64> {
    let out = Command::new(program).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout).trim().parse().ok()
}

/// Total physical RAM in GiB, or `None` if this host will not say (and the memory cap is then
/// skipped, out loud).
pub fn physical_ram_gb() -> Option<f64> {
    let bytes = if cfg!(windows) {
        command_number(
            "powershell",
            &[
                "-NoProfile",
                "-Command",
                "(Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory",
            ],
        )
    } else if cfg!(target_os = "macos") {
        command_number("sysctl", &["-n", "hw.memsize"])
    } else {
        std::fs::read_to_string("/proc/meminfo").ok().and_then(|t| {
            let line = t.lines().find(|l| l.starts_with("MemTotal:"))?;
            let kb: f64 = line.split_whitespace().nth(1)?.parse().ok()?;
            Some(kb * 1024.0)
        })
    }?;
    Some(bytes / (1024.0 * 1024.0 * 1024.0))
}

/// Free space in GiB on the drive holding `path`, or `None` if the host will not say.
pub fn free_gb(path: &Path) -> Option<f64> {
    let bytes = if cfg!(windows) {
        let p = path.to_string_lossy().replace('\'', "''");
        command_number(
            "powershell",
            &[
                "-NoProfile",
                "-Command",
                &format!("([System.IO.DriveInfo]::new((Resolve-Path -LiteralPath '{p}').Path)).AvailableFreeSpace"),
            ],
        )
    } else {
        let out = Command::new("df").arg("-Pk").arg(path).output().ok()?;
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        let kb: f64 = text
            .lines()
            .nth(1)?
            .split_whitespace()
            .nth(3)?
            .parse()
            .ok()?;
        Some(kb * 1024.0)
    }?;
    Some(bytes / (1024.0 * 1024.0 * 1024.0))
}

fn cores() -> Option<usize> {
    std::thread::available_parallelism().ok().map(usize::from)
}

fn env_shards() -> Option<String> {
    std::env::var("DERETH_TEST_SHARDS").ok()
}

/// A UTC timestamp, `YYYY-MM-DDTHH:MM:SSZ`.
fn utc_now() -> String {
    let secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let days = i64::try_from(secs / 86_400).unwrap_or(0);
    let rem = secs % 86_400;
    // Days to a civil date (proleptic Gregorian).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// The dats' location every run will see: the sweep's own addition, else the environment's.
fn dat_dir_line(env: &[(String, String)]) -> String {
    let value = env
        .iter()
        .find(|(k, _)| k == "DERETH_TEST_DAT_DIR")
        .map(|(_, v)| v.clone())
        .or_else(|| std::env::var("DERETH_TEST_DAT_DIR").ok());
    format!(
        "DERETH_TEST_DAT_DIR = {}",
        value.unwrap_or_else(|| "(unset: the dat tests have no dats)".to_owned())
    )
}

/// How the sharded runner is asked to run one binary.
#[derive(Debug, Clone)]
pub struct ShardOpts {
    pub shards: Option<usize>,
    pub serial: bool,
    pub by: Option<By>,
    pub timings: Option<PathBuf>,
    pub profile: Profile,
}

impl ShardOpts {
    /// The options a gate runs a sharded binary with: the defaults, in `profile`.
    pub fn gate(profile: Profile) -> Self {
        ShardOpts {
            shards: None,
            serial: false,
            by: None,
            timings: None,
            profile,
        }
    }
}

/// `--shard-binary PKG:TIER`: run one tier binary sharded and report one merged result line, in
/// libtest's own words, because a gate counts the tests a run executed by reading exactly that
/// shape and a gate that cannot count what it ran is a silent gate. The per-shard lines
/// deliberately do not spell `test result:`. Returns the exit code: 2 when the binary does not
/// exist or would not run, 1 when a test failed or a shard did not finish.
#[allow(clippy::too_many_lines)]
pub fn shard_binary(pkg: &str, tier: &str, opts: &ShardOpts, out: &mut Out) -> i32 {
    let ws = workspace_root();
    out.say(format!(
        "cargo profile: {}",
        if opts.profile == Profile::Dev {
            "dev (--debug)"
        } else {
            "test-release"
        }
    ));
    if !TIERS.contains(&tier) {
        out.say(format!(
            "no such tier {tier:?}; the tiers are {}",
            TIERS.join(", ")
        ));
        return 2;
    }
    let dir = crate_dir(&ws, pkg);
    if !dir.join("tests").join(tier).join("main.rs").is_file() {
        out.say(format!(
            "NO SUCH BINARY: {pkg} has no tests/{tier}/main.rs. A binary named wrong runs nothing \
             and would read as a clean sweep, so this is a hard error."
        ));
        return 2;
    }
    let env = sweep_env();
    out.say(dat_dir_line(&env));
    let n = shard_count(
        pkg,
        tier,
        opts.shards,
        opts.serial,
        env_shards().as_deref(),
        cores(),
    );
    let t0 = Instant::now();
    let log_dir = target_dir().join("sweep");
    let spec = Sharded {
        ws: &ws,
        crate_dir: &dir,
        pkg,
        tier,
        want: n,
        env: &env,
        profile_args: opts.profile.cargo_args(),
        only: None,
        rerun_crashed: true,
        by: opts.by,
        timings: opts.timings.as_deref(),
        log_dir: &log_dir,
        ram_gb: if n > 1 { physical_ram_gb() } else { None },
    };
    let Some(outcome) = run_sharded(&spec, out) else {
        return 2;
    };
    let summary = outcome.summary;
    let results = outcome.results;
    let wall = t0.elapsed().as_secs_f64();
    out.say("");
    // Three figures, because one would be the wrong one for somebody: the slowest shard is what
    // sharding moves; the process total is what the machine spent; the total includes the build in
    // front, which on a busy host can be minutes of waiting on cargo's lock.
    out.say(format!(
        "merged over {} shard(s): tests {:.1} s wall, {:.1} s of process time; total {wall:.1} s \
         including the build",
        results.len(),
        summary.wall,
        summary.cpu
    ));
    let red = summary.failed > 0 || !summary.bad.is_empty();
    out.say(format!(
        "test result: {}. {} passed; {} failed; {} ignored; 0 measured; 0 filtered out",
        if red { "FAILED" } else { "ok" },
        summary.passed,
        summary.failed,
        summary.ignored
    ));
    if !summary.names.is_empty() {
        out.say(format!(
            "failing ({}): {}",
            summary.names.len(),
            summary.names.join(", ")
        ));
    }
    for r in &summary.bad {
        out.say(format!(
            "SHARD {} DID NOT FINISH: exit {} {} -- {}: {}",
            r.k,
            r.code,
            if r.label.is_empty() {
                "(no result line)"
            } else {
                &r.label
            },
            r.by.items_word(),
            r.items_text()
        ));
        out.say(format!(
            "  Its tests have NOT passed. Log: {}",
            r.log.display()
        ));
    }
    if !summary.bad.is_empty() {
        for r in &summary.bad {
            if let Some(again) = &r.rerun {
                out.say(format!(
                    "  shard {} re-run ALONE: {} ({} passed, {} failed, exit {})",
                    r.k,
                    if again.normal {
                        "the crash did NOT reproduce"
                    } else {
                        "THE CRASH REPRODUCED"
                    },
                    again.passed,
                    again.failed,
                    again.code
                ));
            }
        }
        out.say(format!(
            "VERDICT: {} of {} shard(s) did not finish. This run does not cover the binary and is \
             neither a red list nor a green one. A shard that died and then passed on its own is \
             still a shard that died: the rerun is diagnostic evidence, not a clean run.",
            summary.bad.len(),
            results.len()
        ));
    } else if summary.failed > 0 {
        out.say(format!(
            "VERDICT: {} failed across {} shard(s), every shard exited normally.",
            summary.failed,
            results.len()
        ));
    } else {
        out.say(format!(
            "VERDICT: {} passed, 0 failed, every shard exited normally.",
            summary.passed
        ));
    }
    i32::from(red)
}

/// Run one target the ordinary way: `cargo test` for a lib, a legacy target or one module of a
/// tier binary (selected with `--test <tier> <suite>::`, with the tier's features, or cargo builds
/// no binary at all and prints no result).
fn run_one(ws: &Path, pick: &Pick, env: &[(String, String)], profile: Profile) -> Row {
    let pkg = pick.pkg.as_str();
    let suite = pick.suite.as_deref();
    let tier = pick.tier.as_deref();
    let mut args: Vec<String> = vec!["test".into()];
    args.extend(profile.cargo_args().iter().map(|s| (*s).to_owned()));
    args.extend(["-p".into(), pkg.to_owned()]);
    match (suite, tier) {
        (None, _) => args.push("--lib".into()),
        (Some(s), Some(t)) => {
            let feats = shard::tier_features(&crate_dir(ws, pkg), t);
            if !feats.is_empty() {
                args.extend(["--features".into(), feats.join(",")]);
            }
            args.extend(["--test".into(), t.to_owned(), format!("{s}::")]);
        }
        (Some(s), None) => args.extend(["--test".into(), s.to_owned()]),
    }
    // Device-holding suites run serially: one heap-corrupted at the default thread count and
    // emitted no result line. The client dat binary heap-corrupts at the default thread count and
    // at eight under load, so its modules run serially too until the defect is found.
    if Registry::touches_gpu(ws, pick) || (pkg == "dereth-client" && tier == Some("dat")) {
        args.extend(["--".into(), "--test-threads=1".into()]);
    }
    let t0 = Instant::now();
    let mut cmd = Command::new("cargo");
    cmd.args(&args).current_dir(ws);
    for (k, v) in env {
        cmd.env(k, v);
    }
    let (code, text) = match cmd.output() {
        Ok(o) => (
            o.status
                .code()
                .map_or_else(|| "?".to_owned(), |c| c.to_string()),
            format!(
                "{}{}",
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr)
            )
            .replace("\r\n", "\n"),
        ),
        Err(e) => ("launch failed".to_owned(), e.to_string()),
    };
    let secs = t0.elapsed().as_secs_f64();
    let blob = format!(
        "$ cargo {}\n(cwd {}, exit {code}, {secs:.1} s)\n\n{text}",
        args.join(" "),
        ws.display()
    );
    let lines = RESULT.captures_iter(&blob).count();
    match parse(&blob) {
        None => Row {
            passed: None,
            failed: 0,
            lines: 0,
            blob,
            secs,
            names: Vec::new(),
        },
        Some((p, f)) => Row {
            passed: Some(p),
            failed: f,
            lines,
            names: shard::failing_names(&blob),
            blob,
            secs,
        },
    }
}

/// Run `suites` of one tier binary sharded and hand back per-suite rows in the shape [`run_one`]
/// returns, so the sweep's loop, its red-list scoring and its TSV are unchanged. A shard that did
/// not finish gives every one of its modules no result, which is the sweep's existing NO-RESULT
/// state: a target whose process died has not passed.
#[allow(clippy::too_many_arguments)]
fn sharded_rows(
    ws: &Path,
    pkg: &str,
    tier: &str,
    want: usize,
    env: &[(String, String)],
    suites: &BTreeSet<String>,
    opts: &ShardOpts,
    out: &mut Out,
) -> (BTreeMap<Pick, Row>, Option<Summary>) {
    let dir = crate_dir(ws, pkg);
    let log_dir = target_dir().join("sweep");
    let spec = Sharded {
        ws,
        crate_dir: &dir,
        pkg,
        tier,
        want,
        env,
        profile_args: opts.profile.cargo_args(),
        only: Some(suites),
        rerun_crashed: true,
        by: opts.by,
        timings: opts.timings.as_deref(),
        log_dir: &log_dir,
        ram_gb: physical_ram_gb(),
    };
    let mut rows = BTreeMap::new();
    let Some(outcome) = run_sharded(&spec, out) else {
        return (rows, None);
    };
    let summary = outcome.summary;
    out.say(format!(
        "   merged: {} passed  {} failed  {} ignored  over {} shard(s), {:.1} s wall",
        summary.passed,
        summary.failed,
        summary.ignored,
        outcome.results.len(),
        summary.wall
    ));
    if !summary.names.is_empty() {
        out.say(format!("   failing: {}", summary.names.join(", ")));
    }
    let (per, incomplete) = attribute_all(&outcome.results, &outcome.listed);
    if per.is_none() {
        // The arithmetic did not close, so no per-module split here is trustworthy: every module is
        // NO RESULT rather than a plausible row, because a derived number that has stopped deriving
        // must not read as a measurement.
        out.say(
            "   per-module attribution DID NOT CLOSE against the shards' own result lines. Every \
             module of this binary is reported as NO RESULT; the shard totals above are what \
             actually ran.",
        );
    }
    // Which shard's log a module gets: a crashed one if it touched the module, so the NO-RESULT row
    // carries the crash rather than a neighbour's clean output.
    let mut order: Vec<&shard::ShardResult> = outcome.results.iter().collect();
    order.sort_by_key(|r| r.finished());
    let mut log_of: BTreeMap<String, &shard::ShardResult> = BTreeMap::new();
    for r in order {
        for t in shard::shard_tests(r, &outcome.listed) {
            log_of.entry(module_of(&t, &outcome.areas)).or_insert(r);
        }
    }
    for module in outcome.listed.keys() {
        let r = log_of.get(module).copied().unwrap_or(&outcome.results[0]);
        let key = Pick {
            pkg: pkg.to_owned(),
            suite: Some(module.clone()),
            tier: Some(tier.to_owned()),
        };
        match per.as_ref().filter(|_| !incomplete.contains(module)) {
            None => {
                rows.insert(
                    key,
                    Row {
                        passed: None,
                        failed: 0,
                        lines: 0,
                        blob: r.blob.clone(),
                        secs: r.secs,
                        names: Vec::new(),
                    },
                );
            }
            Some(per) => {
                let (p, f, _) = per[module];
                let pre = format!("{module}::");
                rows.insert(
                    key,
                    Row {
                        passed: Some(p),
                        failed: f,
                        lines: 1,
                        blob: r.blob.clone(),
                        secs: r.secs,
                        names: summary
                            .names
                            .iter()
                            .filter(|n| n.starts_with(&pre))
                            .cloned()
                            .collect(),
                    },
                );
            }
        }
    }
    (rows, Some(summary))
}

/// What one finished target's numbers mean for the sweep.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowState {
    /// No result line at all: the target did not run. It has not passed.
    NoResult,
    /// Zero tests, and the binary is one whose every test is ignored on purpose.
    NoTestsByDesign(&'static str),
    /// Zero tests anywhere else. It has not passed either.
    NoTests,
    /// It ran tests; the red list decides the rest.
    Scored(Score),
}

/// The state of one target's row, as a pure function of its numbers.
///
/// A binary that ran no tests has not passed: a feature-gated suite printed `test result: ok. 0
/// passed` and was scored ok until a cross-check caught it. It is neither red (nothing failed) nor
/// ok (nothing ran): it gets its own state, because a third state must not be rounded to one of the
/// other two.
pub fn row_state(
    table: &[(&'static str, &'static str, &'static str)],
    name: &str,
    suite: &str,
    passed: Option<usize>,
    failed: usize,
) -> RowState {
    let Some(passed) = passed else {
        return RowState::NoResult;
    };
    if passed == 0 && failed == 0 {
        return match KNOWN_NO_TESTS.iter().find(|(n, _)| *n == name) {
            Some((_, why)) => RowState::NoTestsByDesign(why),
            None => RowState::NoTests,
        };
    }
    RowState::Scored(score(table, suite, failed))
}

/// The sweep's exit code: 1 when the red list failed, when any target did not run or ran nothing,
/// or when the disk ended the run -- a verdict over part of the tree is never a green one.
pub fn sweep_exit(red_list_bad: usize, did_not_run: usize, aborted: bool) -> i32 {
    i32::from(red_list_bad > 0 || did_not_run > 0 || aborted)
}

/// The sweep's command line.
#[derive(Debug, Default)]
struct Args {
    suites: Vec<String>,
    touched: Vec<String>,
    list: bool,
    no_libs: bool,
    log_dir: Option<PathBuf>,
    no_gpu: bool,
    only_gpu: bool,
    skip_ok: Option<PathBuf>,
    min_free_gb: Option<f64>,
    debug: bool,
    shards: Option<usize>,
    serial: bool,
    shard_by: Option<By>,
    timings: Option<PathBuf>,
    shard_binary: Option<String>,
}

const USAGE: &str = "\
usage: cargo xtask sweep [SUITE...] [options]
  SUITE...              suite names (as --list prints them); default is every target and lib
  --touched TEXT        only targets whose source mentions TEXT (repeatable)
  --list                show the registry and exit
  --no-libs             leave out the lib targets
  --log-dir DIR         save a TSV of every target plus the full output of everything not green
  --no-gpu              only targets that never create a graphics device (a subset, said so)
  --only-gpu            only the device targets; run this when the machine is yours
  --skip-ok TSV         skip targets an earlier run's TSV recorded as ok, and say how many; for
                        resuming an interrupted sweep, not for narrowing one
  --min-free-gb GB      stop if the drive falls below this (default 6): a full disk makes the
                        linker fail and cargo emit no result line, indistinguishable from a red
  --debug               the unoptimised dev profile instead of test-release
  --shards N            run each serialised tier binary as N single-threaded processes (default:
                        DERETH_TEST_SHARDS, else 4 for the two dat binaries and half the logical
                        cores, at most 8, for dereth-client's gpu binary; capped by the module count
                        and by 60% of physical RAM)
  --serial              one process at --test-threads=1, no sharding: the equivalence baseline
  --shard-by test|module  what a shard is a slice of (default: module)
  --timings TSV         deal on this test<TAB>seconds file instead of the committed one
  --shard-binary PKG:TIER  run ONE tier binary sharded and print its merged summary";

fn parse_args(args: &[String]) -> Result<Args, String> {
    let mut a = Args::default();
    let mut it = args.iter();
    let value = |it: &mut std::slice::Iter<String>, flag: &str| -> Result<String, String> {
        it.next()
            .cloned()
            .ok_or_else(|| format!("{flag} needs a value"))
    };
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--touched" => a.touched.push(value(&mut it, arg)?),
            "--list" => a.list = true,
            "--no-libs" => a.no_libs = true,
            "--log-dir" => a.log_dir = Some(value(&mut it, arg)?.into()),
            "--no-gpu" => a.no_gpu = true,
            "--only-gpu" => a.only_gpu = true,
            "--skip-ok" => a.skip_ok = Some(value(&mut it, arg)?.into()),
            "--min-free-gb" => {
                a.min_free_gb = Some(
                    value(&mut it, arg)?
                        .parse()
                        .map_err(|_| "--min-free-gb takes a number".to_owned())?,
                );
            }
            "--debug" => a.debug = true,
            "--shards" => {
                let n: usize = value(&mut it, arg)?
                    .parse()
                    .map_err(|_| "--shards takes a positive number".to_owned())?;
                if n == 0 {
                    return Err("--shards takes a positive number".to_owned());
                }
                a.shards = Some(n);
            }
            "--serial" => a.serial = true,
            "--shard-by" => {
                a.shard_by = Some(match value(&mut it, arg)?.as_str() {
                    "test" => By::Test,
                    "module" => By::Module,
                    other => return Err(format!("--shard-by takes test or module, not {other:?}")),
                });
            }
            "--timings" => a.timings = Some(value(&mut it, arg)?.into()),
            "--shard-binary" => a.shard_binary = Some(value(&mut it, arg)?),
            flag if flag.starts_with("--") => return Err(format!("unknown option {flag}")),
            suite => a.suites.push(suite.to_owned()),
        }
    }
    if a.no_gpu && a.only_gpu {
        return Err("--no-gpu and --only-gpu select disjoint halves; give one".to_owned());
    }
    Ok(a)
}

/// `cargo xtask sweep`.
pub fn sweep(args: &[String]) -> i32 {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("{USAGE}");
        return 0;
    }
    let a = match parse_args(args) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}\n\n{USAGE}");
            return 2;
        }
    };
    let profile = if a.debug {
        Profile::Dev
    } else {
        Profile::TestRelease
    };
    let opts = ShardOpts {
        shards: a.shards,
        serial: a.serial,
        by: a.shard_by,
        timings: a.timings.clone(),
        profile,
    };
    let mut out = Out::print();
    if let Some(spec) = &a.shard_binary {
        let Some((pkg, tier)) = spec.split_once(':') else {
            out.say("--shard-binary takes PKG:TIER, e.g. dereth-client:dat");
            return 2;
        };
        return shard_binary(pkg, tier, &opts, &mut out);
    }
    out.say(format!(
        "cargo profile: {}",
        if a.debug {
            "dev (--debug)"
        } else {
            "test-release"
        }
    ));
    run_sweep(&a, &opts, &mut out)
}

#[allow(clippy::too_many_lines)]
fn run_sweep(a: &Args, opts: &ShardOpts, out: &mut Out) -> i32 {
    let ws = workspace_root();
    let reg = discover(&ws);
    let places = |locs: &[Loc]| -> String {
        locs.iter()
            .map(|(p, t)| match t {
                Some(t) => format!("{p} ({t})"),
                None => p.clone(),
            })
            .collect::<Vec<_>>()
            .join(", ")
    };
    let dupes: Vec<(&String, &Vec<Loc>)> =
        reg.targets.iter().filter(|(_, p)| p.len() > 1).collect();
    if !dupes.is_empty() {
        // Two crates, or two tiers of one crate, with a same-named test file: a bare suite name no
        // longer identifies one target. Say so rather than pick.
        for (n, locs) in &dupes {
            out.say(format!(
                "AMBIGUOUS: {n} exists in {} -- each is swept, none is guessed",
                places(locs)
            ));
        }
        out.say("");
    }
    if a.list {
        for (n, locs) in &reg.targets {
            let flag = known_red(KNOWN_RED, n)
                .map_or_else(String::new, |(owner, _)| format!("  [KNOWN RED {owner}]"));
            let dup = if locs.len() > 1 { "  [AMBIGUOUS]" } else { "" };
            out.say(format!("{n:<46} {}{flag}{dup}", places(locs)));
        }
        out.say(format!(
            "\n{} integration targets, {} libs, {} known red",
            reg.targets.len(),
            libs(&ws).len(),
            KNOWN_RED.len()
        ));
        return 0;
    }
    for (n, _, _) in KNOWN_RED {
        if !reg.targets.contains_key(*n) {
            out.say(format!(
                "STALE RED LIST: {n} is listed as known-red and no such target exists"
            ));
        }
    }

    let mut chosen: Vec<Pick> = Vec::new();
    if !a.suites.is_empty() {
        for n in &a.suites {
            let Some(locs) = reg.targets.get(n) else {
                out.say(format!("NO SUCH TARGET: {n}"));
                out.say(
                    "A suite named wrong emits no result line and reads as nothing, so this is a \
                     hard error. Run --list to see the registry.",
                );
                return 2;
            };
            if locs.len() > 1 {
                out.say(format!("AMBIGUOUS TARGET: {n} exists in {}", places(locs)));
                out.say(
                    "A bare name does not identify one target. Rename one file, or run cargo \
                     directly with the -p and --test you mean.",
                );
                return 2;
            }
            chosen.extend(reg.picks(n));
        }
    } else if !a.touched.is_empty() {
        for n in reg.targets.keys() {
            for pick in reg.picks(n) {
                let src = std::fs::read(Registry::suite_path(&ws, &pick))
                    .map(|b| String::from_utf8_lossy(&b).into_owned())
                    .unwrap_or_default();
                if a.touched.iter().any(|t| src.contains(t.as_str())) {
                    chosen.push(pick);
                }
            }
        }
        if chosen.is_empty() {
            out.say(format!(
                "no target mentions {} -- that is a finding, not an empty run",
                a.touched.join(" or ")
            ));
            return 2;
        }
    } else {
        for n in reg.targets.keys() {
            chosen.extend(reg.picks(n));
        }
        chosen.sort();
    }
    let lib_rows = || -> Vec<Pick> { libs(&ws).iter().map(|p| Pick::lib(p)).collect() };
    if !a.no_libs && a.suites.is_empty() {
        let mut with = lib_rows();
        with.append(&mut chosen);
        chosen = with;
    }

    let env = sweep_env();
    out.say(dat_dir_line(&env));
    out.say(format!("launching {} targets\n", chosen.len()));

    if a.no_gpu || a.only_gpu {
        let want = a.only_gpu;
        let before = chosen.len();
        chosen.retain(|pick| pick.suite.is_some() && Registry::touches_gpu(&ws, pick) == want);
        if a.no_gpu && !a.no_libs && a.suites.is_empty() {
            let mut with = lib_rows();
            with.append(&mut chosen);
            chosen = with;
        }
        out.say(format!(
            "{}: {} of {before} targets selected. THIS IS A SUBSET -- its verdict is a\n  \
             statement about {} targets only, never about the tree.",
            if want { "--only-gpu" } else { "--no-gpu" },
            chosen.len(),
            if want { "device" } else { "device-free" }
        ));
    }

    let mut skipped_ok = 0usize;
    if let Some(tsv) = &a.skip_ok {
        let text = match std::fs::read_to_string(tsv) {
            Ok(t) => t,
            Err(e) => {
                out.say(format!("--skip-ok: cannot read {}: {e}", tsv.display()));
                return 2;
            }
        };
        let done: BTreeSet<Pick> = text
            .lines()
            .skip(1)
            .filter_map(|l| {
                let col: Vec<&str> = l.split('\t').collect();
                (col.len() > 3 && col[3] == "ok").then(|| Pick {
                    pkg: col[1].to_owned(),
                    suite: (col[2] != "lib").then(|| col[2].to_owned()),
                    tier: col
                        .get(8)
                        .filter(|t| !t.is_empty())
                        .map(|t| (*t).to_owned()),
                })
            })
            .collect();
        let before = chosen.len();
        chosen.retain(|row| !done.contains(row));
        skipped_ok = before - chosen.len();
        out.say(format!(
            "--skip-ok: {skipped_ok} of {before} targets were ok in {} and are NOT being re-run.\n  \
             They are still part of this run's denominator; the two TSVs must be\n  unioned \
             before anyone calls the tree green.",
            tsv.display()
        ));
    }

    // The sharded binaries: a serialised tier binary's modules are pulled out of the per-target
    // loop and run as N single-threaded processes over disjoint slices. The rows come back in the
    // per-target shape, so everything below -- the red list's decision, the TSV, the NO-RESULT
    // arithmetic, the verdict -- is the same code on the same numbers, and `--serial` puts every
    // one of them back through the per-target run. Only the selected modules are dealt, so
    // `--touched`, a named suite list and `--no-gpu` narrow a sharded run exactly as they narrow a
    // serial one.
    let envn = env_shards();
    let mut groups: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();
    for pick in &chosen {
        let (Some(suite), Some(tier)) = (&pick.suite, &pick.tier) else {
            continue;
        };
        if shard_count(
            &pick.pkg,
            tier,
            a.shards,
            a.serial,
            envn.as_deref(),
            cores(),
        ) > 1
        {
            groups
                .entry((pick.pkg.clone(), tier.clone()))
                .or_default()
                .insert(suite.clone());
        }
    }
    let mut presharded: BTreeMap<Pick, Row> = BTreeMap::new();
    let mut shard_summaries: Vec<((String, String), Summary)> = Vec::new();
    for ((pkg, tier), suites) in &groups {
        let n = shard_count(pkg, tier, a.shards, a.serial, envn.as_deref(), cores());
        let (rows, summary) = sharded_rows(&ws, pkg, tier, n, &env, suites, opts, out);
        presharded.extend(rows);
        if let Some(s) = summary {
            shard_summaries.push(((pkg.clone(), tier.clone()), s));
        }
        out.say("");
    }

    let mut tsv = None;
    if let Some(dir) = &a.log_dir {
        let _ = std::fs::create_dir_all(dir);
        match std::fs::File::create(dir.join("sweep.tsv")) {
            Ok(mut f) => {
                let _ = writeln!(
                    f,
                    "finished_utc\tpkg\tsuite\tverdict\tpassed\tfailed\tseconds\tfailing_tests\ttier"
                );
                tsv = Some(f);
                out.say(format!("log dir = {}", dir.display()));
            }
            Err(e) => {
                out.say(format!("--log-dir: cannot create {}: {e}", dir.display()));
                return 2;
            }
        }
    }
    let mut record = |out: &mut Out,
                      pick: &Pick,
                      verdict: &str,
                      passed: String,
                      failed: String,
                      secs: f64,
                      names: &[String],
                      blob: &str| {
        let Some(f) = tsv.as_mut() else { return };
        let (pkg, suite) = (pick.pkg.as_str(), pick.suite.as_deref());
        let _ = writeln!(
            f,
            "{}\t{pkg}\t{}\t{verdict}\t{passed}\t{failed}\t{secs:.1}\t{}\t{}",
            utc_now(),
            suite.unwrap_or("lib"),
            names.join(","),
            pick.tier.as_deref().unwrap_or("")
        );
        let _ = f.flush();
        // Keep the whole output of anything that is not cleanly green: a red quoted as a list of
        // names cannot be triaged later, and a missing log once cost an hour to re-take. The logger
        // must never be able to end the run it is recording -- a full disk once raised here and
        // killed a sweep at 87 of 438 -- but it must not swallow the reason either.
        if verdict != "ok" {
            let dir = a.log_dir.as_deref().expect("a log dir");
            let file = dir.join(format!(
                "{pkg}__{}{}.log",
                pick.tier
                    .as_deref()
                    .map_or_else(String::new, |t| format!("{t}__")),
                suite.unwrap_or("lib").replace("::", "__")
            ));
            if let Err(e) = std::fs::write(&file, blob) {
                out.say(format!("    (log not written: {e})"));
            }
        }
    };

    let min_free = a.min_free_gb.unwrap_or(6.0);
    let mut last_disk_check: Option<(Instant, Option<f64>)> = None;
    let mut disk_unknown_said = false;
    let (mut seen, mut unexpected, mut red_list_bad) = (0usize, 0usize, 0usize);
    let (mut expected_red, mut fixed_red, mut no_result, mut no_tests) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut aborted_at: Option<(usize, String, f64)> = None;
    let total = chosen.len();
    for (i, pick) in chosen.iter().enumerate() {
        let i = i + 1;
        let name = pick.shown(&reg);
        // Stop rather than manufacture reds: a full drive once turned three suites into NO RESULT
        // within 3 seconds each (the linker failed, not one line of Rust at fault), and every
        // target after that point would have read red for the same reason.
        let stale = last_disk_check.is_none_or(|(t, _)| t.elapsed() > Duration::from_secs(30));
        if stale {
            last_disk_check = Some((Instant::now(), free_gb(&ws)));
        }
        match last_disk_check.and_then(|(_, gb)| gb) {
            Some(gb) if gb < min_free => {
                aborted_at = Some((i, name.clone(), gb));
                out.say(format!(
                    "\nABORTED before {name}: only {gb:.1} GB free (--min-free-gb {min_free:.1}).\n  \
                     A full disk fails the LINK, so cargo emits no result line and\n  every \
                     remaining target would read red for a reason that is not\n  in this \
                     repository. {} targets were NOT run.",
                    total - i + 1
                ));
                break;
            }
            None if !disk_unknown_said => {
                disk_unknown_said = true;
                out.say("  (free disk space unknown on this host: --min-free-gb NOT applied)");
            }
            _ => {}
        }
        let row = match presharded.get(pick) {
            Some(r) => r.clone(),
            None => run_one(&ws, pick, &env, opts.profile),
        };
        let key = pick.suite.as_deref().unwrap_or("");
        let state = match row_state(KNOWN_RED, &pick.name(), key, row.passed, row.failed) {
            RowState::NoResult => {
                no_result.push(name.clone());
                out.say(format!(
                    "[{i}/{total}] {name:<52} NO RESULT LINE  ({:.1} s)",
                    row.secs
                ));
                record(
                    out,
                    pick,
                    "NO-RESULT",
                    String::new(),
                    String::new(),
                    row.secs,
                    &[],
                    &row.blob,
                );
                continue;
            }
            RowState::NoTestsByDesign(why) => {
                seen += 1;
                record(
                    out,
                    pick,
                    "NO-TESTS-BY-DESIGN",
                    "0".into(),
                    "0".into(),
                    row.secs,
                    &[],
                    &row.blob,
                );
                out.say(format!(
                    "[{i}/{total}] {name:<52} {:<40} ({why})",
                    "no tests by design"
                ));
                continue;
            }
            RowState::NoTests => {
                seen += 1;
                no_tests.push(name.clone());
                record(
                    out,
                    pick,
                    "NO-TESTS",
                    "0".into(),
                    "0".into(),
                    row.secs,
                    &[],
                    &row.blob,
                );
                out.say(format!(
                    "[{i}/{total}] {name:<52} {:<40} 0 passed  0 failed  ({:.1} s)",
                    "RAN NO TESTS", row.secs
                ));
                continue;
            }
            RowState::Scored(state) => state,
        };
        seen += 1;
        let (passed, failed) = (row.passed.unwrap_or(0), row.failed);
        if state.fails_the_sweep() {
            red_list_bad += 1;
        }
        let tag = match state {
            Score::RedKnown => {
                expected_red.push(name.clone());
                format!(
                    "RED (known, {})",
                    known_red(KNOWN_RED, key).map_or("", |(o, _)| o)
                )
            }
            Score::RedUnowned => {
                unexpected += 1;
                "RED  *** UNEXPECTED ***".to_owned()
            }
            Score::StaleEntry => {
                fixed_red.push(name.clone());
                "ok  *** known-red now PASSES: retire the entry ***".to_owned()
            }
            Score::Ok => "ok".to_owned(),
        };
        let mut names = row.names.clone();
        if failed > 0 && names.is_empty() {
            // A count with no roll-call. Never let this read as "no failures".
            names = vec![format!("<{failed} failed, names unrecoverable>")];
        }
        record(
            out,
            pick,
            if failed > 0 { "RED" } else { "ok" },
            passed.to_string(),
            failed.to_string(),
            row.secs,
            &names,
            &row.blob,
        );
        out.say(format!(
            "[{i}/{total}] {name:<52} {tag:<40} {passed} passed  {failed} failed  ({:.1} s){}",
            row.secs,
            if failed > 0 {
                format!("  {}", names.join(", "))
            } else {
                String::new()
            }
        ));
        let _ = row.lines;
    }

    out.say("");
    out.say("-".repeat(92));
    for ((pkg, tier), summary) in &shard_summaries {
        for r in &summary.bad {
            out.say(format!(
                "SHARD {} OF {pkg}::{tier} DID NOT FINISH: exit {} {}",
                r.k,
                r.code,
                if r.label.is_empty() {
                    "(no result line)"
                } else {
                    &r.label
                }
            ));
            out.say(format!("  {}: {}", r.by.items_word(), r.items_text()));
            out.say(format!("  log: {}", r.log.display()));
            out.say(
                "  Every one of those modules is NO RESULT below: a target whose process died has \
                 not passed.",
            );
        }
    }
    let launched = aborted_at.as_ref().map_or(total, |(i, _, _)| i - 1);
    out.say(format!(
        "targets launched: {launched}   result lines seen: {seen}   NO RESULT: {}",
        no_result.len()
    ));
    if skipped_ok > 0 {
        out.say(format!(
            "SKIPPED (ok in {}): {skipped_ok} -- union the TSVs; this run alone is not the tree",
            a.skip_ok
                .as_deref()
                .map_or_else(String::new, |p| p.display().to_string())
        ));
    }
    if let Some((i, name, gb)) = &aborted_at {
        out.say(format!(
            "DID NOT RUN: {} targets, because the drive fell to {gb:.1} GB at {name}.",
            total - i + 1
        ));
        out.say("  This run does not cover the tree. It is not a red list and not a green one.");
    }
    if !no_result.is_empty() {
        out.say(format!("NO RESULT LINE from: {}", no_result.join(", ")));
        out.say("  A target that emits no result line has NOT passed. It did not run.");
    }
    if !no_tests.is_empty() {
        out.say(format!("RAN NO TESTS: {}", no_tests.join(", ")));
        out.say("  A binary that ran zero tests has NOT passed either. Usually a crate gated");
        out.say("  behind a cargo feature, or a filter matching no test name.");
    }
    if !fixed_red.is_empty() {
        out.say(format!(
            "KNOWN-RED NOW PASSING (stale list): {}",
            fixed_red.join(", ")
        ));
        out.say("  This FAILS the sweep. An entry that outlives its red does");
        out.say("  not sit there harmlessly -- it EXCUSES that suite, so the next genuine");
        out.say("  regression in it prints `RED (known, ...)` and the run still exits 0.");
        out.say(
            "  THE FIX: delete the entry from KNOWN_RED in tools/xtask/src/sweep/known_red.rs,",
        );
        out.say("  and keep the reason in its roll-call of retired entries.");
    }
    if unexpected > 0 || !fixed_red.is_empty() {
        if unexpected > 0 {
            out.say(format!("VERDICT: {unexpected} UNEXPECTED RED"));
        }
        if !fixed_red.is_empty() {
            out.say(format!(
                "VERDICT: {} STALE KNOWN_RED ENTR{}",
                fixed_red.len(),
                if fixed_red.len() == 1 { "Y" } else { "IES" }
            ));
        }
    } else if !no_result.is_empty() || !no_tests.is_empty() {
        // Not "green". Nothing failed, but something did not run, and rounding that to green is the
        // failure this sweep exists to prevent -- including on its own summary line, which once
        // said "green" while printing RAN NO TESTS three lines above.
        let mut both = no_result.clone();
        both.extend(no_tests.iter().cloned());
        out.say(format!(
            "VERDICT: nothing failed, but {} target(s) did not run: {}",
            both.len(),
            both.join(", ")
        ));
    } else if !expected_red.is_empty() {
        out.say(format!(
            "VERDICT: green except {}, which are known red: {}",
            expected_red.len(),
            expected_red.join(", ")
        ));
        for n in &expected_red {
            let s = n.split_once("::").map_or(n.as_str(), |(_, s)| s);
            let s = s.split_once(" [").map_or(s, |(s, _)| s);
            if let Some((owner, why)) = known_red(KNOWN_RED, s) {
                out.say(format!("   {s}  ({owner})  {why}"));
            }
        }
    } else {
        out.say("VERDICT: green, with no known-red entries outstanding");
    }
    if let Some((i, _, _)) = &aborted_at {
        out.say(format!(
            "VERDICT ABOVE COVERS ONLY {} OF {total} TARGETS -- the disk ended the run.",
            i - 1
        ));
    }
    sweep_exit(
        red_list_bad,
        no_result.len() + no_tests.len(),
        aborted_at.is_some(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const GREEN: &str = "running 3 tests\ntest a ... ok\ntest b ... ok\ntest c ... ok\n\n\
        test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n";

    /// The wrong-package shape. It contains the word `test` five times and names a real crate:
    /// anything looser than "a result line" reads it as fine.
    const WRONG_PKG: &str =
        "error: no test target named `examine_window` in default-run packages\n\
        help: available test targets:\n    o124_selection_queries\n    o164_click_routing\n";

    /// The crash shape: the binary is built, launched and dies; libtest never reaches its summary,
    /// and cargo reports it only in a tail a result-line grep never sees.
    const CRASH: &str = "   Compiling dereth-client v0.1.0\n    Finished `test` profile [unoptimized + debuginfo] target(s) in 41.02s\n     Running tests\\o525_trade_window.rs (target\\debug\\deps\\o525_trade_window-9c1.exe)\n\n\
        running 6 tests\nerror: test failed, to rerun pass `-p dereth-client --test o525_trade_window`\n\n\
        Caused by:\n  process didn't exit successfully: `target\\debug\\deps\\o525_trade_window-9c1.exe`\n  (exit code: 0xc0000005, STATUS_ACCESS_VIOLATION)\n";

    /// A red whose per-test status lines are split by a printing test -- the interleaving that once
    /// scored real failures as passes for weeks. The roll-call at the end is intact.
    const INTERLEAVED: &str = "running 5 tests\ntest t_one ... ok\ntest t_two ... [world] loading landblock 0x1234\nFAILED\n\
        test t_three ... ok\ntest t_four ... probe: 0 hits\nFAILED\ntest t_five ... ok\n\nfailures:\n\n\
        ---- t_two stdout ----\nassertion failed: ghosted.contains(&split_id)\n\nfailures:\n    t_four\n    t_two\n\n\
        test result: FAILED. 3 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out\n";

    /// A real run of nothing at all: legitimate for a lib with no unit tests, and
    /// indistinguishable from a passing suite unless counted.
    const EMPTY: &str = "running 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n";

    /// Each output shape is told apart: a green suite ran and passed; a wrong package name and a
    /// crash did not run; an interleaved red counts 3 passed and 2 failed with both names whole; a
    /// run of zero tests reads as zero, never as a pass.
    #[test]
    fn each_output_shape_is_told_apart_from_the_others() {
        assert_eq!(parse(GREEN), Some((3, 0)));
        assert_eq!(parse(WRONG_PKG), None);
        assert_eq!(parse(CRASH), None);
        assert_eq!(parse(INTERLEAVED), Some((3, 2)));
        assert_eq!(shard::failing_names(INTERLEAVED), vec!["t_four", "t_two"]);
        assert_eq!(parse(EMPTY), Some((0, 0)));
    }

    /// The naive per-test-line parser misses what this one catches -- otherwise the calibration
    /// above would be satisfied by the very parser whose blind spot cost weeks -- and the
    /// roll-call parser invents no names where there are none.
    #[test]
    fn the_naive_status_line_parser_misses_what_the_roll_call_catches() {
        let naive = Regex::new(r"(?m)^test (\S+) \.\.\. FAILED$").expect("naive");
        assert_eq!(naive.find_iter(INTERLEAVED).count(), 0);
        assert_eq!(naive.find_iter(CRASH).count(), 0);
        assert!(shard::failing_names(GREEN).is_empty());
        assert!(
            shard::failing_names(CRASH).is_empty(),
            "a crash's count is unrecoverable, not zero"
        );
    }

    fn argv(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| (*w).to_owned()).collect()
    }

    /// The command line keeps the flags people use: `--shard-binary`, `--serial`, `--shards`,
    /// `--shard-by`, `--timings`, `--debug`, suites and `--touched`; a zero shard count, an unknown
    /// flag and both device halves at once are refused.
    #[test]
    fn the_command_line_keeps_the_flags_people_use_and_refuses_nonsense() {
        let a = parse_args(&argv(&[
            "--shard-binary",
            "dereth-client:gpu",
            "--serial",
            "--shards",
            "3",
            "--shard-by",
            "test",
            "--timings",
            "t.tsv",
            "--debug",
        ]))
        .expect("parses");
        assert_eq!(a.shard_binary.as_deref(), Some("dereth-client:gpu"));
        assert!(a.serial && a.debug);
        assert_eq!((a.shards, a.shard_by), (Some(3), Some(By::Test)));
        assert_eq!(a.timings, Some(PathBuf::from("t.tsv")));
        let b = parse_args(&argv(&[
            "rendering::sky",
            "--touched",
            "x",
            "--touched",
            "y",
        ]))
        .expect("parses");
        assert_eq!(
            (b.suites, b.touched),
            (argv(&["rendering::sky"]), argv(&["x", "y"]))
        );
        assert!(parse_args(&argv(&["--shards", "0"])).is_err());
        assert!(parse_args(&argv(&["--selftest"])).is_err());
        assert!(parse_args(&argv(&["--no-gpu", "--only-gpu"])).is_err());
    }

    /// The registry is derived from where files live: a consolidated module and an area module get
    /// their tier, a legacy target none, `main.rs`, `mod.rs` and `common/` are not suites, and a
    /// name in two crates is kept under both.
    #[test]
    fn the_registry_is_derived_from_the_file_layout() {
        let ws = std::env::temp_dir().join(format!("xtask-sweep-registry-{}", std::process::id()));
        std::fs::remove_dir_all(&ws).ok();
        let make = |rel: &str| {
            let p = ws.join(rel);
            std::fs::create_dir_all(p.parent().expect("a parent")).expect("dirs");
            std::fs::write(p, "").expect("write");
        };
        make("core/one/Cargo.toml");
        make("core/one/tests/legacy.rs");
        make("core/one/tests/dat/main.rs");
        make("core/one/tests/dat/flat.rs");
        make("core/one/tests/dat/common/mod.rs");
        make("core/one/tests/gpu/main.rs");
        make("core/one/tests/gpu/zone/mod.rs");
        make("core/one/tests/gpu/zone/spot.rs");
        make("dereth/client/crates/two/Cargo.toml");
        make("dereth/client/crates/two/tests/cpu/main.rs");
        make("dereth/client/crates/two/tests/cpu/flat.rs");
        make("core/one/tests/gpu/flat.rs");
        let reg = discover(&ws);
        let names: Vec<&str> = reg.targets.keys().map(String::as_str).collect();
        assert_eq!(names, ["flat", "legacy", "zone::spot"]);
        let loc = |p: &str, t: Option<&str>| (p.to_owned(), t.map(str::to_owned));
        assert_eq!(
            reg.targets["flat"],
            vec![
                loc("dereth-one", Some("dat")),
                loc("dereth-one", Some("gpu")),
                loc("dereth-two", Some("cpu"))
            ],
            "the same stem in two tiers of one crate is two targets, neither lost"
        );
        assert_eq!(reg.targets["legacy"], vec![loc("dereth-one", None)]);
        let flats = reg.picks("flat");
        assert_eq!(reg.tiers_of("dereth-one", "flat"), 2);
        assert_eq!(flats[0].shown(&reg), "dereth-one::flat [dat]");
        assert_eq!(flats[2].shown(&reg), "dereth-two::flat");
        let sky = &reg.picks("zone::spot")[0];
        assert_eq!(
            Registry::suite_path(&ws, sky),
            ws.join("core/one/tests/gpu/zone/spot.rs")
        );
        assert!(Registry::touches_gpu(&ws, sky));
        assert!(!Registry::touches_gpu(&ws, &flats[0]));
        assert!(Registry::touches_gpu(&ws, &flats[1]));
        std::fs::remove_dir_all(&ws).ok();
    }

    /// A Cargo configuration's `[env]` reaches the runs as cargo would pass it: the nearest file
    /// wins, a relative entry is read against the directory holding that `.cargo/`, a variable the
    /// environment already sets wins unless the entry is forced, and a directory with no
    /// configuration adds nothing.
    #[test]
    fn a_cargo_config_env_reaches_the_runs_as_cargo_would_pass_it() {
        let root = std::env::temp_dir().join(format!("xtask-sweep-config-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        let inner = root.join("ws");
        std::fs::create_dir_all(root.join(".cargo")).expect("dirs");
        std::fs::create_dir_all(inner.join(".cargo")).expect("dirs");
        std::fs::write(
            root.join(".cargo/config.toml"),
            "[env]\nDATS = { value = \"clients/x\", relative = true }\nNEAR = \"outer\"\n\
             FORCED = { value = \"yes\", force = true }\n",
        )
        .expect("write");
        std::fs::write(
            inner.join(".cargo/config.toml"),
            "[env]\nNEAR = \"inner\"\n[alias]\nx = \"run\"\n",
        )
        .expect("write");
        let config = cargo_config_env(&inner);
        assert_eq!(
            config["NEAR"],
            ("inner".to_owned(), false),
            "the nearest file wins"
        );
        assert_eq!(
            config["DATS"],
            (root.join("clients/x").to_string_lossy().into_owned(), false)
        );
        assert_eq!(config["FORCED"], ("yes".to_owned(), true));
        let env = env_over(&config, |k| {
            k == "DATS" || k == "FORCED" || k == "DERETH_TEST_FFMPEG"
        });
        let keys: Vec<&str> = env.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(
            keys,
            ["FORCED", "NEAR"],
            "a set variable wins unless forced"
        );
        let empty =
            std::env::temp_dir().join(format!("xtask-sweep-noconfig-{}", std::process::id()));
        std::fs::create_dir_all(&empty).expect("dirs");
        assert!(env_over(&ConfigEnv::new(), |_| true).is_empty());
        std::fs::remove_dir_all(&root).ok();
        std::fs::remove_dir_all(&empty).ok();
    }

    /// A row with no result line did not run; zero tests is its own state (by design only for a
    /// listed binary); a run of tests goes to the red list; and anything that did not run, ran
    /// nothing or was cut short by the disk fails the sweep even when nothing is red.
    #[test]
    fn a_target_that_did_not_run_or_ran_nothing_is_never_a_pass() {
        let t: &[(&str, &str, &str)] = &[("owned", "owner", "a cause long enough to act on")];
        assert_eq!(row_state(t, "p::x", "x", None, 0), RowState::NoResult);
        assert_eq!(row_state(t, "p::x", "x", Some(0), 0), RowState::NoTests);
        let (designed, _) = KNOWN_NO_TESTS[0];
        assert!(matches!(
            row_state(t, designed, "roundtrip", Some(0), 0),
            RowState::NoTestsByDesign(_)
        ));
        assert_eq!(
            row_state(t, "p::x", "x", Some(3), 0),
            RowState::Scored(Score::Ok)
        );
        assert_eq!(
            row_state(t, "p::x", "x", Some(3), 1),
            RowState::Scored(Score::RedUnowned)
        );
        assert_eq!(
            row_state(t, "p::owned", "owned", Some(3), 1),
            RowState::Scored(Score::RedKnown)
        );
        assert_eq!(
            row_state(t, "p::owned", "owned", Some(3), 0),
            RowState::Scored(Score::StaleEntry)
        );
        assert_eq!(sweep_exit(0, 0, false), 0);
        assert_eq!(sweep_exit(1, 0, false), 1);
        assert_eq!(
            sweep_exit(0, 1, false),
            1,
            "nothing failed, but something did not run"
        );
        assert_eq!(sweep_exit(0, 0, true), 1, "the disk ended the run");
    }

    /// The TSV timestamp is a real UTC date.
    #[test]
    fn the_timestamp_is_an_iso_utc_date() {
        let t = utc_now();
        assert_eq!(t.len(), 20, "{t}");
        assert!(
            t.starts_with("20") && t.ends_with('Z') && t.as_bytes()[10] == b'T',
            "{t}"
        );
    }

    /// Captured output keeps every line for a caller to print on failure; printed output keeps
    /// none.
    #[test]
    fn captured_output_keeps_its_lines() {
        let mut c = Out::capture();
        c.say("one");
        c.say(String::from("two"));
        assert_eq!(c.into_text(), "one\ntwo\n");
    }
}
