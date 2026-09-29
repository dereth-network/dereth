//! Running one tier binary as N single-threaded processes over disjoint slices of its modules.
//!
//! **Why N processes and not N threads, and why here rather than in the test binary.** The client
//! `dat` binary fail-fasts with heap corruption (`STATUS_HEAP_CORRUPTION`) at libtest's default
//! thread count and has never finished a default-threaded run. The defect is a within-process
//! race: calibration measured twelve clean runs at 1, 2, 4 and 8 threads and five crashes in seven
//! at 32. The interim gate is therefore `--test-threads=1`, which costs 20 to 40 minutes for one
//! `dat` run.
//!
//! **A second process shares no heap.** So the concurrency comes back at the process boundary: N
//! copies of the same binary, each `--test-threads=1`, each handed a disjoint slice of the module
//! list as positional filters. Nothing inside any one process is more parallel than the serial
//! gate it replaces, so the race is not reintroduced; the only thing that grows is how many
//! single-threaded processes the machine runs at once.
//!
//! **The honesty requirement is sharper here.** A shard that dies emits no `test result:` line,
//! so a merge that only summed result lines would report the surviving shards' totals and read as
//! a smaller, greener run. Every shard's exit code is checked against libtest's own two (0 all
//! passed, 101 a test failed); anything else is named, its slice is printed so the crash stays
//! diagnostic evidence, the slice is re-run alone before being reported, and its modules are
//! handed back as NO RESULT rather than dropped.
//!
//! Two crashes occurred with three `dat` binaries running on one host at once, so N is also capped
//! by memory: a `dat` process peaks around 2.3 GB, and an N whose peak would pass 60% of physical
//! RAM comes down and says so.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use regex::Regex;

use super::Out;

/// What a shard is a slice of.
///
/// Every sharded binary is dealt by `Module` today, and for `dereth-testkit`'s `dat` binary that
/// is a measurement rather than a default: 356 tests in thirteen modules with one holding 120 looks
/// like a ceiling for a module deal until it is timed. What floors that binary is one test (the
/// census that re-runs every scenario in-process, 600 to 700 s of a 1,502 s serial run). Measured at
/// N=4: by module 787.5 s, by test 850.9 s. Slicing by test buys nothing against one indivisible
/// test and costs a longer command line, so the finer grain is available (`--shard-by test`) and
/// not the default. If that census becomes a static set equality, the choice must be re-measured.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum By {
    /// Each item is a module; its filter is `<module>::`.
    Module,
    /// Each item is a whole test name, passed under `--exact`.
    Test,
}

impl By {
    pub fn items_word(self) -> &'static str {
        match self {
            By::Module => "modules",
            By::Test => "tests",
        }
    }
}

/// The default process count of a sharded binary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Default {
    Fixed(usize),
    /// Follows the host's cores: see [`auto_shards`].
    Auto,
}

/// The `(package, tier)` binaries that run `--test-threads=1`, and how many processes each is
/// split across by default, measured on the reference host against the serial run of the same
/// tree.
///
/// `dereth-client`'s `gpu` binary is automatic: its `--test-threads=1` is a device lock inside one
/// process, and separate processes each open their own device, which every driver supports.
/// `dereth-render`'s `gpu` binary is small and stays one process.
pub const SHARD_DEFAULT: &[((&str, &str), Default)] = &[
    (("dereth-client", "dat"), Default::Fixed(4)),
    (("dereth-testkit", "dat"), Default::Fixed(4)),
    (("dereth-client", "gpu"), Default::Auto),
];

/// What each sharded binary is dealt by. See [`By`] for why every one is `Module`.
///
/// `dereth-client`'s `dat` stations share state through `tests/dat/common/` within a module, so a
/// module is the smallest slice that is safe to move whole; 1,288 tests over 216 modules deal
/// 322/322/322/322 anyway.
pub const SHARD_BY: &[((&str, &str), By)] = &[
    (("dereth-client", "dat"), By::Module),
    (("dereth-testkit", "dat"), By::Module),
    (("dereth-client", "gpu"), By::Module),
];

/// The deal's grain for a binary: the table's entry, else modules.
pub fn shard_by(pkg: &str, tier: &str) -> By {
    SHARD_BY
        .iter()
        .find(|((p, t), _)| *p == pkg && *t == tier)
        .map_or(By::Module, |(_, b)| *b)
}

/// Measured on a 16-thread, 8-core host with a hardware GPU, 1,280 gpu tests: one process 2,468 s;
/// eight processes 554 s wall for the slowest shard, with the same known reds. The processes sum
/// to 3,713 s against 2,468 s -- the tests are CPU-bound even on a hardware device, so eight
/// processes on eight cores contend and more would buy little. The same eight on the software
/// renderer ran 28 to 48 minutes per shard, about five times the hardware time test for test, and
/// saturated the CPU.
///
/// One automatic binary gets `cores / AUTO_CORES_PER_SHARD` processes, at least one and at most
/// [`AUTO_MAX_SHARDS`], before the memory cap.
pub const AUTO_CORES_PER_SHARD: usize = 2;
pub const AUTO_MAX_SHARDS: usize = 8;

/// The committed recorded-time files, per binary: a file name prefix under `tools/sweep-timings/`.
/// The newest match is dealt on, so the same tree deals the same slices on every machine.
pub const TIMINGS_COMMITTED: &[((&str, &str), &str)] =
    &[(("dereth-client", "gpu"), "dereth-client-gpu")];

/// Windows caps a command line at 32,767 characters, and a test-level shard spends its whole budget
/// on test names. Over this the deal falls back to modules, out loud, rather than launching a shard
/// the OS will truncate or reject: a truncated command line runs the wrong tests, and a shard that
/// ran the wrong tests still prints a confident result line.
pub const MAX_CMDLINE: usize = 30_000;

/// One `dat` process's measured peak memory in GB, and the share of physical RAM the runner will
/// commit to shards.
pub const SHARD_PEAK_GB: f64 = 2.3;
pub const SHARD_RAM_BUDGET: f64 = 0.60;
/// Per-binary peaks where they differ from the `dat` figure. The gpu figure is not measured
/// separately; it is the `dat` one.
pub const SHARD_PEAK_GB_BY: &[((&str, &str), f64)] = &[(("dereth-client", "gpu"), 2.3)];

/// libtest's own two exit codes: 0, every test passed; 101, at least one failed. Anything else is
/// the process dying, which a result-line-only sweep would read as silence.
pub const LIBTEST_EXITS: [i64; 2] = [0, 101];

/// The Windows status codes worth naming when one arrives. An unlisted code is still abnormal and
/// still fails the run; this table only decides whether the message can say what it was.
pub const NT_STATUS: &[(u32, &str)] = &[
    (0xC000_0005, "STATUS_ACCESS_VIOLATION"),
    (0xC000_001D, "STATUS_ILLEGAL_INSTRUCTION"),
    (0xC000_0094, "STATUS_INTEGER_DIVIDE_BY_ZERO"),
    (0xC000_00FD, "STATUS_STACK_OVERFLOW"),
    (0xC000_0374, "STATUS_HEAP_CORRUPTION"),
    (0xC000_0409, "STATUS_STACK_BUFFER_OVERRUN"),
    (0xC000_0602, "STATUS_FAIL_FAST_EXCEPTION"),
];

/// `--list` prints one `<module>::<path>::<name>: test` line per test, ignored tests included, then
/// an `N tests, M benchmarks` tail.
pub static LIST_LINE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^(\S+): test\s*$").expect("LIST_LINE"));
/// `test result: ok. 3 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out`
pub static RESULT_FULL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?m)test result: (\w+)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out",
    )
    .expect("RESULT_FULL")
});
/// An ignored test prints its verdict the moment libtest skips it, so unlike a `FAILED` line this
/// one cannot be split by a printing test: nothing runs between the name and the word. That is
/// what makes the per-module attribution checkable.
pub static IGNORED_LINE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^test (\S+) \.\.\. ignored").expect("IGNORED_LINE"));
/// libtest prints the `failures:` roll-call after every test has finished and its output is
/// flushed, so unlike the per-test `test NAME ... FAILED` lines it cannot be split by a printing
/// test. A parser that read the status lines once scored real failures as passes. The verdict comes
/// from the count, the names from this trailing block, and a non-zero count with an empty block is
/// reported as names unrecoverable rather than as no failures.
pub static FAILBLOCK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^failures:\n((?:    \S.*\n)+)").expect("FAILBLOCK"));

/// Names from every trailing `failures:` roll-call in `blob`, deduplicated (libtest repeats them
/// inside each failing test's captured stdout too).
pub fn failing_names(blob: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for m in FAILBLOCK.captures_iter(blob) {
        for line in m[1].lines() {
            let n = line.trim();
            if !n.is_empty() && !names.iter().any(|x| x == n) {
                names.push(n.to_owned());
            }
        }
    }
    names
}

/// `(normal, label)` for one finished shard.
///
/// `normal` is false for everything that is not one of libtest's own two codes, and the label
/// names the Windows status when it is one this runner knows. A Windows exit code arrives signed or
/// unsigned depending on how the process ended, so it is masked to 32 bits before it is looked up:
/// `-1073740940` and `3221226356` are the same `0xC0000374`, and failures have used both.
pub fn exit_status(code: i64) -> (bool, String) {
    if LIBTEST_EXITS.contains(&code) {
        return (true, String::new());
    }
    let u = u32::try_from(code & 0xFFFF_FFFF).unwrap_or(u32::MAX);
    match NT_STATUS.iter().find(|(c, _)| *c == u) {
        Some((_, name)) => (false, format!("{name} (0x{u:08X})")),
        None => (false, format!("abnormal exit 0x{u:08X}")),
    }
}

/// The shard key of one test: its former target. That is the first `::` component in a flat tier
/// binary, and `<area>::<stem>` when the first component is an area directory
/// (`tests/<tier>/<area>/<stem>.rs`) -- the suite name the sweep gives the same file, so a sharded
/// row and a serial row name the same thing. A test declared in the area's own `mod.rs` has no stem
/// and keys on the area.
pub fn module_of(name: &str, areas: &BTreeSet<String>) -> String {
    let parts: Vec<&str> = name.split("::").collect();
    if areas.contains(parts[0]) && parts.len() > 2 {
        return format!("{}::{}", parts[0], parts[1]);
    }
    parts[0].to_owned()
}

/// `{module: [test, ...]}` from a libtest `--list` dump, tests in list order.
///
/// A module is a former `tests/<stem>.rs`, so a slice of modules is a slice of the old targets and
/// `<module>::` is a filter that selects exactly it. The trailing `::` matters: a bare `astra_chat`
/// also matches `astra_chat_focus_model`, because libtest's positional filters are substring
/// matches, not prefixes.
pub fn list_modules(list_output: &str, areas: &BTreeSet<String>) -> BTreeMap<String, Vec<String>> {
    let mut mods: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for m in LIST_LINE.captures_iter(list_output) {
        mods.entry(module_of(&m[1], areas))
            .or_default()
            .push(m[1].to_owned());
    }
    mods
}

/// Every test `<module>::` actually selects, which is not the same as every test in the module.
///
/// libtest's positional filters are substring matches on the whole test name. The `::` stops
/// `alpha` reaching `alpha_extra`, but nothing stops a filter reaching a module whose name *ends*
/// with it: `routing::` also selects every `o164_click_routing::<test>`. Measured: the first N=4
/// run of the client `dat` binary ran 1,311 tests where `--list` said 1,288, because those two
/// modules landed in different shards and 23 tests ran twice, and nothing said which.
pub fn filter_selection(module: &str, names: &[String]) -> Vec<String> {
    let key = format!("{module}::");
    names.iter().filter(|n| n.contains(&key)).cloned().collect()
}

/// `[(modules, tests)]`: modules welded together where their filters overlap, sorted.
///
/// Two modules whose filters can select the same test must go in the same shard, or the slices are
/// not disjoint and the test runs twice. Welding keeps the cheap substring filter (one short
/// argument per module) and makes the deal exact; naming every test under `--exact` would spend the
/// whole command line on names. A group's weight is the tests its filters select between them,
/// counted once, so the deal balances on what will actually run.
pub fn filter_groups(census: &BTreeMap<String, Vec<String>>) -> Vec<(Vec<String>, Vec<String>)> {
    let names: Vec<String> = census.values().flatten().cloned().collect();
    let mods: Vec<&String> = census.keys().collect();
    let index: BTreeMap<&str, usize> = mods
        .iter()
        .enumerate()
        .map(|(i, m)| (m.as_str(), i))
        .collect();
    let mut owner: Vec<usize> = (0..mods.len()).collect();
    fn find(owner: &mut [usize], mut m: usize) -> usize {
        while owner[m] != m {
            owner[m] = owner[owner[m]];
            m = owner[m];
        }
        m
    }
    let mut by_test: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for m in &mods {
        for n in filter_selection(m, &names) {
            by_test.entry(n).or_default().push(index[m.as_str()]);
        }
    }
    for ms in by_test.values() {
        for &other in &ms[1..] {
            let (a, b) = (find(&mut owner, ms[0]), find(&mut owner, other));
            if a != b {
                owner[b] = a;
            }
        }
    }
    let mut members: BTreeMap<usize, Vec<String>> = BTreeMap::new();
    for (i, m) in mods.iter().enumerate() {
        let root = find(&mut owner, i);
        members.entry(root).or_default().push((*m).clone());
    }
    let mut out: Vec<(Vec<String>, Vec<String>)> = members
        .into_values()
        .map(|mut group| {
            group.sort();
            let tests: BTreeSet<String> = group
                .iter()
                .flat_map(|m| filter_selection(m, &names))
                .collect();
            (group, tests.into_iter().collect())
        })
        .collect();
    out.sort();
    out
}

/// Deal weighted units into `n` shards; returns `n` unit lists (fewer when there are fewer units).
///
/// Heaviest first, each to the lightest shard so far. A round-robin over the list balances the
/// number of units, which is not what costs wall clock: modules here run from one test to thirty
/// odd, so dealing by position leaves one shard running long after the others and the run is as
/// slow as its worst slice. Ties break on the unit, so the same tree deals the same slices every
/// time and a crashed shard's slice is reproducible.
pub fn deal_shards<K: Ord + Clone>(weights: &BTreeMap<K, f64>, n: usize) -> Vec<Vec<K>> {
    let n = if weights.is_empty() {
        1
    } else {
        n.clamp(1, weights.len())
    };
    let mut shards: Vec<Vec<K>> = vec![Vec::new(); n];
    let mut load = vec![0.0f64; n];
    let mut units: Vec<(&K, f64)> = weights.iter().map(|(k, w)| (k, *w)).collect();
    units.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    for (k, w) in units {
        let lightest = (0..n)
            .min_by(|&a, &b| load[a].total_cmp(&load[b]).then(a.cmp(&b)))
            .expect("at least one shard");
        shards[lightest].push(k.clone());
        load[lightest] += w;
    }
    shards
}

/// `(n, why)`: the process count this host will actually run, and why it moved if it did.
///
/// Two ceilings. A shard with nothing in it is a process that runs zero tests, so N never exceeds
/// the number of units to deal. And N processes at `peak_gb` each must fit inside `budget` of
/// physical RAM: a host that starts paging turns a slow gate into an unreadable one. RAM the host
/// will not report skips the cap, and says so, rather than applying it to a number nobody measured.
pub fn cap_shards(
    want: usize,
    items: usize,
    ram_gb: Option<f64>,
    peak_gb: f64,
    budget: f64,
) -> (usize, String) {
    let mut why = Vec::new();
    let mut n = want.max(1);
    if items > 0 && n > items {
        n = items;
        why.push(format!("only {items} item(s) to deal"));
    }
    match ram_gb {
        Some(ram) if ram > 0.0 => {
            let mut room = 1usize;
            while count_f64(room + 1) * peak_gb <= ram * budget {
                room += 1;
            }
            if n > room {
                why.push(format!(
                    "{ram:.0} GB RAM x {:.0}% / {peak_gb:.1} GB per process = {room}",
                    budget * 100.0
                ));
                n = room;
            }
        }
        _ if want > 1 => why.push("physical RAM unknown, memory cap NOT applied".to_owned()),
        _ => {}
    }
    (n, why.join("; "))
}

/// The process count an automatic binary gets on a host with `cores` logical processors.
pub fn auto_shards(cores: Option<usize>) -> usize {
    (cores.unwrap_or(1) / AUTO_CORES_PER_SHARD).clamp(1, AUTO_MAX_SHARDS)
}

/// How many processes one tier binary gets: 1 under `--serial`, then an explicit `--shards N`,
/// then `DERETH_TEST_SHARDS` (`env_shards`) for a binary that shards at all, then the binary's
/// measured default. The environment variable does not reach a binary that is not in
/// [`SHARD_DEFAULT`], and a value that is not a positive number is ignored rather than guessed at.
pub fn shard_count(
    pkg: &str,
    tier: &str,
    shards: Option<usize>,
    serial: bool,
    env_shards: Option<&str>,
    cores: Option<usize>,
) -> usize {
    if serial {
        return 1;
    }
    if let Some(n) = shards {
        return n;
    }
    let Some((_, default)) = SHARD_DEFAULT
        .iter()
        .find(|((p, t), _)| *p == pkg && *t == tier)
    else {
        return 1;
    };
    if *default == Default::Fixed(1) {
        return 1;
    }
    if let Some(n) = env_shards
        .map(str::trim)
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|n| *n >= 1)
    {
        return n;
    }
    match default {
        Default::Auto => auto_shards(cores),
        Default::Fixed(n) => *n,
    }
}

// ------------------------------------------------------------------------------ per-test timing
//
// libtest on stable has no per-test time (`--report-time` and `--format json` are both nightly
// only). It does not need one here: at `--test-threads=1` the pretty formatter writes
// `test <name> ... ` and flushes before the test runs, then writes the verdict when it ends. A
// shard's output is read from a pipe as it arrives and every chunk is stamped, so a test's wall time
// is the arrival of its verdict minus the arrival of its name: the wall clock of the test inside its
// shard, including any first-use cost (opening the dats) that test paid.

static TEST_START: LazyLock<regex::bytes::Regex> =
    LazyLock::new(|| regex::bytes::Regex::new(r"(?m-u)^test (\S+) \.\.\. ").expect("TEST_START"));
static TEST_VERDICT: LazyLock<regex::bytes::Regex> = LazyLock::new(|| {
    regex::bytes::Regex::new(r"(?-u)\b(ok|FAILED|ignored)\b").expect("TEST_VERDICT")
});

/// One shard's output as it came: `(arrival in seconds, bytes)`.
pub type Chunks = Vec<(f64, Vec<u8>)>;

/// `{test: seconds}` from stamped output. A test whose verdict never arrived (the shard died in it)
/// is timed to the last byte the shard wrote, the lower bound the log can prove.
pub fn test_times(chunks: &[(f64, Vec<u8>)]) -> BTreeMap<String, f64> {
    let mut out = BTreeMap::new();
    if chunks.is_empty() {
        return out;
    }
    let data: Vec<u8> = chunks.iter().flat_map(|(_, b)| b.iter().copied()).collect();
    let mut ends = Vec::with_capacity(chunks.len());
    let mut off = 0;
    for (_, b) in chunks {
        off += b.len();
        ends.push(off);
    }
    let at = |pos: usize| {
        let k = ends.partition_point(|&e| e <= pos);
        chunks[k.min(chunks.len() - 1)].0
    };
    let last = chunks[chunks.len() - 1].0;
    for m in TEST_START.captures_iter(&data) {
        let whole = m.get(0).expect("a match");
        let name = String::from_utf8_lossy(&m[1]).into_owned();
        let start = at(whole.end() - 1);
        let end = TEST_VERDICT
            .find_at(&data, whole.end())
            .map_or(last, |v| at(v.start()));
        out.insert(name, (end - start).max(0.0));
    }
    out
}

/// `{test: seconds}` from a `test<TAB>seconds` file with a header line, or empty.
pub fn load_timings(path: &Path) -> BTreeMap<String, f64> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return BTreeMap::new();
    };
    text.lines()
        .skip(1)
        .filter_map(|line| {
            let mut col = line.split('\t');
            let name = col.next()?;
            let secs = col.next()?.trim().parse::<f64>().ok()?;
            Some((name.to_owned(), secs))
        })
        .collect()
}

/// Write `{test: seconds}` slowest first, the shape [`load_timings`] reads.
pub fn write_timings(path: &Path, times: &BTreeMap<String, f64>) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut rows: Vec<(&String, &f64)> = times.iter().collect();
    rows.sort_by(|a, b| b.1.total_cmp(a.1).then_with(|| a.0.cmp(b.0)));
    let mut text = String::from("test\tseconds\n");
    for (name, secs) in rows {
        text.push_str(&format!("{name}\t{secs:.3}\n"));
    }
    std::fs::write(path, text)
}

/// `(path, timings)` for the deal: an explicit file, else the newest committed file matching this
/// binary under `tools/sweep-timings/`, else none (the deal balances on test count).
pub fn find_timings(
    ws: &Path,
    pkg: &str,
    tier: &str,
    explicit: Option<&Path>,
) -> (Option<PathBuf>, BTreeMap<String, f64>) {
    if let Some(p) = explicit {
        return (Some(p.to_path_buf()), load_timings(p));
    }
    let Some((_, prefix)) = TIMINGS_COMMITTED
        .iter()
        .find(|((p, t), _)| *p == pkg && *t == tier)
    else {
        return (None, BTreeMap::new());
    };
    let dir = ws.join("tools").join("sweep-timings");
    let mut found: Vec<PathBuf> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(prefix) && n.ends_with(".tsv"))
        })
        .collect();
    found.sort();
    match found.pop() {
        Some(p) => {
            let t = load_timings(&p);
            (Some(p), t)
        }
        None => (None, BTreeMap::new()),
    }
}

/// Each unit's weight: the recorded seconds of the tests it selects, or its test count when there
/// are no timings. A test the file does not name weighs the file's mean, so a stale file degrades
/// toward the count deal rather than failing.
pub fn unit_weights<K: Ord + Clone>(
    units: &BTreeMap<K, Vec<String>>,
    timings: &BTreeMap<String, f64>,
) -> BTreeMap<K, f64> {
    if timings.is_empty() {
        return units
            .iter()
            .map(|(k, t)| (k.clone(), count_f64(t.len())))
            .collect();
    }
    let mean = timings.values().sum::<f64>() / count_f64(timings.len());
    units
        .iter()
        .map(|(k, tests)| {
            (
                k.clone(),
                tests
                    .iter()
                    .map(|t| timings.get(t).copied().unwrap_or(mean))
                    .sum(),
            )
        })
        .collect()
}

/// A count as a weight. Counts here are test counts, far below where `f64` loses integers.
fn count_f64(n: usize) -> f64 {
    f64::from(u32::try_from(n).unwrap_or(u32::MAX))
}

/// The exact command one shard runs. One function, so the command that is printed, the command
/// that is launched and the re-run of a crashed shard cannot drift apart.
///
/// Module items become `<module>::` filters (the trailing `::` is load-bearing: see
/// [`list_modules`]); test items are whole names under `--exact`, which turns the same positional
/// filters into exact matches. No positional filter at all when `items` is `None`: the serial form,
/// the whole binary in one single-threaded process.
pub fn shard_argv(exe: &str, items: Option<&[String]>, by: By) -> Vec<String> {
    let mut argv = vec![exe.to_owned(), "--test-threads=1".to_owned()];
    let Some(items) = items else {
        return argv;
    };
    match by {
        By::Test => {
            argv.push("--exact".to_owned());
            argv.extend(items.iter().cloned());
        }
        By::Module => argv.extend(items.iter().map(|m| format!("{m}::"))),
    }
    argv
}

/// The longest command line any of `slices` would launch, in characters.
pub fn longest_command(exe: &str, slices: &[Vec<String>], by: By) -> usize {
    slices
        .iter()
        .map(|sl| shard_argv(exe, Some(sl), by).join(" ").len())
        .max()
        .unwrap_or(0)
}

/// One finished shard, read from its exit code and output.
#[derive(Debug, Clone)]
pub struct ShardResult {
    pub k: usize,
    pub items: Option<Vec<String>>,
    pub by: By,
    pub code: i64,
    pub normal: bool,
    pub label: String,
    pub secs: f64,
    pub log: PathBuf,
    pub blob: String,
    pub names: Vec<String>,
    pub ignored_names: Vec<String>,
    pub result_line: bool,
    pub passed: usize,
    pub failed: usize,
    pub ignored: usize,
    pub times: BTreeMap<String, f64>,
    /// The slice re-run alone after a crash. Evidence, never a replacement: see [`run_sharded`].
    pub rerun: Option<Box<ShardResult>>,
}

impl ShardResult {
    /// It exited with one of libtest's codes and printed a result line.
    pub fn finished(&self) -> bool {
        self.normal && self.result_line
    }

    pub fn items_text(&self) -> String {
        self.items
            .as_ref()
            .map_or_else(|| "(the whole binary)".to_owned(), |i| i.join(" "))
    }
}

/// One shard's numbers, as a pure function of its exit code and its output, so every arm that
/// matters -- a crash read as silence, a result line read as a pass, the per-module attribution --
/// can be driven on synthetic output rather than a forty-minute test binary.
pub fn shard_result(
    k: usize,
    items: Option<Vec<String>>,
    code: i64,
    blob: &str,
    secs: f64,
    log: &Path,
    by: By,
) -> ShardResult {
    let blob = blob.replace("\r\n", "\n");
    let (normal, label) = exit_status(code);
    let hits: Vec<regex::Captures> = RESULT_FULL.captures_iter(&blob).collect();
    let sum = |i: usize| -> usize {
        hits.iter()
            .map(|h| h[i].parse::<usize>().unwrap_or(0))
            .sum()
    };
    let (passed, failed, ignored) = (sum(2), sum(3), sum(4));
    let result_line = !hits.is_empty();
    ShardResult {
        k,
        items,
        by,
        code,
        normal,
        label,
        secs,
        log: log.to_path_buf(),
        names: failing_names(&blob),
        ignored_names: IGNORED_LINE
            .captures_iter(&blob)
            .map(|m| m[1].to_owned())
            .collect(),
        blob,
        result_line,
        passed,
        failed,
        ignored,
        times: BTreeMap::new(),
        rerun: None,
    }
}

/// How many tests one shard will actually run: a union over its filters, never a sum, because two
/// welded modules select some of the same tests and adding their selections counts those twice.
/// That is what made the partition guard fire on a correct deal the first time it ran.
pub fn shard_test_count(items: &[String], by: By, all_names: &[String]) -> usize {
    match by {
        By::Test => items.len(),
        By::Module => items
            .iter()
            .flat_map(|m| filter_selection(m, all_names))
            .collect::<BTreeSet<_>>()
            .len(),
    }
}

/// The exact test names one shard was asked to run: what decides whether a module's census was
/// covered by a shard that finished, which is the question a crash turns into a per-module verdict.
pub fn shard_tests(res: &ShardResult, listed: &BTreeMap<String, Vec<String>>) -> Vec<String> {
    let names: Vec<String> = listed.values().flatten().cloned().collect();
    match (&res.items, res.by) {
        (None, _) => names,
        (Some(items), By::Test) => items.clone(),
        (Some(items), By::Module) => items
            .iter()
            .flat_map(|m| filter_selection(m, &names))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
    }
}

/// Per-module `(passed, failed, ignored)`.
pub type PerModule = BTreeMap<String, (usize, usize, usize)>;

/// `(per, incomplete)`: per-module counts across every shard, and the modules no finished shard
/// covered.
///
/// Sharding trades one result line per module for one per shard, while the red list still scores
/// per module. So the numbers are put back from text libtest writes whole: the failing names from
/// the trailing roll-call, the ignored names from their own lines, and `passed` as what is left of
/// the module's `--list` census. `incomplete` is every module some finished shard did not cover --
/// because a shard died, or because this run asked for only part of the binary; those have not
/// passed, and one dead shard costs its own slice and not the whole run.
///
/// **And then it is checked rather than asserted.** The per-module totals must add back up to the
/// finished shards' own result lines. If they do not, `per` is `None` and the caller says the split
/// failed instead of printing a plausible row per module: a derived number that has quietly stopped
/// deriving is the failure this runner is about, and the arithmetic is free.
pub fn attribute_all(
    results: &[ShardResult],
    listed: &BTreeMap<String, Vec<String>>,
) -> (Option<PerModule>, BTreeSet<String>) {
    let good: Vec<&ShardResult> = results.iter().filter(|r| r.finished()).collect();
    let ran: BTreeSet<String> = good.iter().flat_map(|r| shard_tests(r, listed)).collect();
    let mut per = PerModule::new();
    let mut incomplete = BTreeSet::new();
    for (module, tests) in listed {
        let live = tests.iter().filter(|t| ran.contains(*t)).count();
        if live != tests.len() {
            incomplete.insert(module.clone());
        }
        let pre = format!("{module}::");
        let f = good
            .iter()
            .flat_map(|r| &r.names)
            .filter(|n| n.starts_with(&pre))
            .count();
        let g = good
            .iter()
            .flat_map(|r| &r.ignored_names)
            .filter(|n| n.starts_with(&pre))
            .count();
        per.insert(module.clone(), (live.saturating_sub(f + g), f, g));
    }
    let total = |pick: fn(&(usize, usize, usize)) -> usize| per.values().map(pick).sum::<usize>();
    let closes = total(|p| p.0) == good.iter().map(|r| r.passed).sum::<usize>()
        && total(|p| p.1) == good.iter().map(|r| r.failed).sum::<usize>()
        && total(|p| p.2) == good.iter().map(|r| r.ignored).sum::<usize>();
    (closes.then_some(per), incomplete)
}

/// One summary from N shard results.
#[derive(Debug, Clone)]
pub struct Summary {
    pub passed: usize,
    pub failed: usize,
    pub ignored: usize,
    pub names: Vec<String>,
    /// Every shard that did not finish. The half a naive merge loses: a shard that died adds 0
    /// passed and 0 failed, so totals alone make a crashed run look like a smaller green one, and
    /// every caller fails on a non-empty `bad`.
    pub bad: Vec<ShardResult>,
    /// The slowest shard: what sharding moves, and the only figure comparable with a serial run.
    pub wall: f64,
    /// The shards added up: what the machine spent.
    pub cpu: f64,
}

pub fn merge_shards(results: &[ShardResult]) -> Summary {
    let mut names: Vec<String> = Vec::new();
    for n in results.iter().flat_map(|r| &r.names) {
        if !names.contains(n) {
            names.push(n.clone());
        }
    }
    Summary {
        passed: results.iter().map(|r| r.passed).sum(),
        failed: results.iter().map(|r| r.failed).sum(),
        ignored: results.iter().map(|r| r.ignored).sum(),
        names,
        bad: results.iter().filter(|r| !r.finished()).cloned().collect(),
        wall: results.iter().map(|r| r.secs).fold(0.0, f64::max),
        cpu: results.iter().map(|r| r.secs).sum(),
    }
}

// ------------------------------------------------------------------------------ launching

/// The area directories of one tier binary: `tests/<tier>/<area>/`, `common` excluded.
pub fn tier_areas(crate_dir: &Path, tier: &str) -> BTreeSet<String> {
    std::fs::read_dir(crate_dir.join("tests").join(tier))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().to_str().map(str::to_owned))
        .filter(|n| n != "common")
        .collect()
}

/// A shard in flight.
pub struct Running {
    k: usize,
    items: Option<Vec<String>>,
    by: By,
    child: Child,
    log: PathBuf,
    chunks: Arc<Mutex<Chunks>>,
    pumped: mpsc::Receiver<()>,
}

/// Launch one shard.
///
/// The output goes through one pipe (stdout and stderr together), drained by a thread that writes
/// every chunk to the log as it arrives and stamps it for [`test_times`]. N pipes filled by N
/// chatty test binaries would deadlock the moment one went undrained, and the log is wanted on disk
/// anyway: a crash whose output lived only in a dead process's pipe is not data for anybody.
#[allow(clippy::too_many_arguments)]
pub fn run_shard(
    k: usize,
    exe: &str,
    items: Option<Vec<String>>,
    by: By,
    cwd: &Path,
    env: &[(String, String)],
    log: &Path,
    epoch: Instant,
) -> std::io::Result<Running> {
    let argv = shard_argv(exe, items.as_deref(), by);
    let mut fh = std::fs::File::create(log)?;
    writeln!(fh, "$ {}\n", argv.join(" "))?;
    let (mut reader, writer) = std::io::pipe()?;
    let mut cmd = Command::new(&argv[0]);
    cmd.args(&argv[1..])
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(writer.try_clone()?)
        .stderr(writer);
    for (k, v) in env {
        cmd.env(k, v);
    }
    let child = cmd.spawn()?;
    // The command held the pipe's write ends; dropping it leaves the child as the only writer, so
    // the reader sees end-of-file when the child (and anything it spawned) exits.
    drop(cmd);
    let chunks: Arc<Mutex<Chunks>> = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&chunks);
    let (done, pumped) = mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = vec![0u8; 65536];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let at = epoch.elapsed().as_secs_f64();
                    sink.lock()
                        .expect("the chunk list")
                        .push((at, buf[..n].to_vec()));
                    let _ = fh.write_all(&buf[..n]);
                    let _ = fh.flush();
                }
            }
        }
        let _ = done.send(());
    });
    Ok(Running {
        k,
        items,
        by,
        child,
        log: log.to_path_buf(),
        chunks,
        pumped,
    })
}

/// One finished shard's result, given how long it ran.
fn finish(r: Running, code: i64, secs: f64) -> ShardResult {
    // A child the shard spawned can hold the pipe open after the shard itself exits; the log then
    // ends where the shard did.
    let _ = r.pumped.recv_timeout(Duration::from_secs(60));
    let blob = std::fs::read(&r.log)
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();
    let mut res = shard_result(r.k, r.items, code, &blob, secs, &r.log, r.by);
    let chunks = r.chunks.lock().expect("the chunk list").clone();
    res.times = test_times(&chunks);
    res
}

fn exit_code(status: std::process::ExitStatus) -> i64 {
    status.code().map_or(-1, i64::from)
}

/// Wait for every shard and give each one **its own** wall clock.
///
/// Polling rather than waiting on each in turn, and the difference is not cosmetic: waiting on
/// shard 0 first means shard 1's time is not measured until shard 0 is done, so every shard after
/// the slowest reports the slowest one's duration. A first draft did exactly that and printed four
/// identical figures, which reads as a perfectly balanced deal -- the opposite of what the numbers
/// were collected to show. A deal is only as good as its longest shard, so the spread is the
/// measurement.
pub fn collect_all(mut running: Vec<Running>, t0: Instant) -> Vec<ShardResult> {
    let mut out: Vec<Option<ShardResult>> = (0..running.len()).map(|_| None).collect();
    let mut left: Vec<(usize, Running)> = running.drain(..).enumerate().collect();
    while !left.is_empty() {
        let mut still = Vec::new();
        for (i, mut r) in left {
            match r.child.try_wait() {
                Ok(Some(status)) => {
                    let secs = t0.elapsed().as_secs_f64();
                    out[i] = Some(finish(r, exit_code(status), secs));
                }
                Ok(None) => still.push((i, r)),
                Err(_) => {
                    let secs = t0.elapsed().as_secs_f64();
                    out[i] = Some(finish(r, -1, secs));
                }
            }
        }
        left = still;
        if !left.is_empty() {
            std::thread::sleep(Duration::from_millis(200));
        }
    }
    out.into_iter().flatten().collect()
}

/// The features a tier binary needs. `dat`, `gpu` and `local` sit behind `retail-dats`; `gpu` also
/// needs the crate's device feature, `vulkan` or `d3d12`, read from its own manifest rather than
/// assumed.
pub fn tier_features(crate_dir: &Path, tier: &str) -> Vec<String> {
    if tier == "cpu" {
        return Vec::new();
    }
    let mut feats = vec!["retail-dats".to_owned()];
    if tier == "gpu" {
        let manifest = std::fs::read_to_string(crate_dir.join("Cargo.toml")).unwrap_or_default();
        for dev in ["vulkan", "d3d12"] {
            let declared = manifest.lines().any(|l| {
                l.strip_prefix(dev)
                    .is_some_and(|rest| rest.trim_start().starts_with('='))
            });
            if declared {
                feats.push(dev.to_owned());
                break;
            }
        }
    }
    feats
}

/// Build one tier binary; its path, or `None` with the build output.
///
/// `--no-run` plus `--message-format=json`, because the shards are the binary run N times and not
/// `cargo test` run N times: N concurrent cargo processes queue on the build-directory lock, which
/// would serialise the very thing this is here to parallelise. Launching the executable directly
/// also leaves each shard's command line in a form a reader can paste. `--locked` always: a sweep
/// must never be the thing that moves `Cargo.lock`.
pub fn test_binary(
    ws: &Path,
    crate_dir: &Path,
    pkg: &str,
    tier: &str,
    profile_args: &[&str],
    env: &[(String, String)],
) -> Result<PathBuf, String> {
    let feats = tier_features(crate_dir, tier);
    let mut args: Vec<String> = vec!["test".into(), "--locked".into()];
    args.extend(profile_args.iter().map(|s| (*s).to_owned()));
    args.extend(["-p".into(), pkg.to_owned()]);
    if !feats.is_empty() {
        args.extend(["--features".into(), feats.join(",")]);
    }
    args.extend(["--test", tier, "--no-run", "--message-format=json"].map(str::to_owned));
    let mut cmd = Command::new("cargo");
    cmd.args(&args).current_dir(ws);
    for (k, v) in env {
        cmd.env(k, v);
    }
    let out = cmd.output().map_err(|e| format!("cargo: {e}"))?;
    let rel = crate_dir
        .strip_prefix(ws)
        .unwrap_or(crate_dir)
        .to_string_lossy()
        .replace('\\', "/");
    let want = format!("{rel}/tests/{tier}/main.rs");
    let mut exe = None;
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let Ok(msg) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let src = msg["target"]["src_path"]
            .as_str()
            .unwrap_or("")
            .replace('\\', "/");
        if msg["target"]["name"].as_str() == Some(tier) && src.ends_with(&want) {
            if let Some(e) = msg["executable"].as_str() {
                exe = Some(PathBuf::from(e));
            }
        }
    }
    let blob = format!(
        "$ cargo {}\n(cwd {}, exit {})\n\n{}",
        args.join(" "),
        ws.display(),
        out.status
            .code()
            .map_or_else(|| "?".to_owned(), |c| c.to_string()),
        String::from_utf8_lossy(&out.stderr)
    );
    match exe {
        Some(e) if e.is_file() => Ok(e),
        _ => Err(blob),
    }
}

/// Where a run's shard logs and timings go.
pub struct Sharded<'a> {
    pub ws: &'a Path,
    pub crate_dir: &'a Path,
    pub pkg: &'a str,
    pub tier: &'a str,
    pub want: usize,
    pub env: &'a [(String, String)],
    pub profile_args: &'a [&'a str],
    /// A subset of modules to deal; `None` is the whole binary.
    pub only: Option<&'a BTreeSet<String>>,
    pub rerun_crashed: bool,
    pub by: Option<By>,
    pub timings: Option<&'a Path>,
    pub log_dir: &'a Path,
    pub ram_gb: Option<f64>,
}

/// What a sharded run hands back: the merged summary, every shard, and the modules the run covered
/// with their tests.
pub struct Outcome {
    pub summary: Summary,
    pub results: Vec<ShardResult>,
    pub listed: BTreeMap<String, Vec<String>>,
    pub areas: BTreeSet<String>,
}

/// Run one tier binary as N single-threaded processes over disjoint module slices.
///
/// `want` of 1 with no subset runs the serial form -- one process, no positional filter -- so the
/// equivalence baseline and the sharded run come out of the same code path and differ in exactly
/// the thing under test. `None` when the binary would not build or listed no test. Nothing here
/// decides an exit code: callers do, from `summary.bad` and `summary.failed`.
#[allow(clippy::too_many_lines)]
pub fn run_sharded(s: &Sharded, out: &mut Out) -> Option<Outcome> {
    let exe = match test_binary(s.ws, s.crate_dir, s.pkg, s.tier, s.profile_args, s.env) {
        Ok(e) => e,
        Err(blob) => {
            out.say(format!(
                "{}::{} -- the tier binary DID NOT BUILD, so nothing ran:",
                s.pkg, s.tier
            ));
            let tail: String = {
                let chars: Vec<char> = blob.chars().collect();
                chars[chars.len().saturating_sub(2000)..].iter().collect()
            };
            out.say(tail);
            return None;
        }
    };
    let exe_s = exe.to_string_lossy().into_owned();
    let mut list = Command::new(&exe);
    list.arg("--list").current_dir(s.ws);
    for (k, v) in s.env {
        list.env(k, v);
    }
    let listing = list
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).replace("\r\n", "\n"))
        .unwrap_or_default();
    let areas = tier_areas(s.crate_dir, s.tier);
    let census = list_modules(&listing, &areas);
    let all_names: Vec<String> = census.values().flatten().cloned().collect();
    let asked: BTreeMap<String, Vec<String>> = match s.only {
        None => census.clone(),
        Some(only) => census
            .iter()
            .filter(|(m, _)| only.contains(*m))
            .map(|(m, t)| (m.clone(), t.clone()))
            .collect(),
    };
    if asked.is_empty() {
        out.say(format!(
            "{}::{} -- `--list` named no test at all. That is a finding, not an empty run.",
            s.pkg, s.tier
        ));
        return None;
    }
    let mut by = s.by.unwrap_or_else(|| shard_by(s.pkg, s.tier));
    let (tpath, timings) = find_timings(s.ws, s.pkg, s.tier, s.timings);
    let peak = SHARD_PEAK_GB_BY
        .iter()
        .find(|((p, t), _)| *p == s.pkg && *t == s.tier)
        .map_or(SHARD_PEAK_GB, |(_, g)| *g);

    // `(slices, covered, n, why)` for one grain. For modules the unit is a group of modules whose
    // filters overlap, never a bare module, so two shards cannot select the same test. Welding can
    // pull in a module the caller did not ask for, and that closure is returned rather than hidden:
    // those tests run, so they are reported.
    let deal = |by: By| -> (Vec<Vec<String>>, BTreeSet<String>, usize, String) {
        match by {
            By::Test => {
                let units: BTreeMap<String, Vec<String>> = asked
                    .values()
                    .flatten()
                    .map(|t| (t.clone(), vec![t.clone()]))
                    .collect();
                let (n, why) = cap_shards(s.want, units.len(), s.ram_gb, peak, SHARD_RAM_BUDGET);
                let raw = deal_shards(&unit_weights(&units, &timings), n);
                (raw, asked.keys().cloned().collect(), n, why)
            }
            By::Module => {
                let units: BTreeMap<Vec<String>, Vec<String>> = filter_groups(&census)
                    .into_iter()
                    .filter(|(mods, _)| mods.iter().any(|m| asked.contains_key(m)))
                    .collect();
                let (n, why) = cap_shards(s.want, units.len(), s.ram_gb, peak, SHARD_RAM_BUDGET);
                let raw = deal_shards(&unit_weights(&units, &timings), n);
                let covered: BTreeSet<String> = raw.iter().flatten().flatten().cloned().collect();
                let slices = raw
                    .into_iter()
                    .map(|sl| sl.into_iter().flatten().collect())
                    .collect();
                (slices, covered, n, why)
            }
        }
    };
    let (mut slices, mut covered, mut n, mut why) = deal(by);
    let whole = n == 1 && s.only.is_none();
    if !whole && by == By::Test {
        let longest = longest_command(&exe_s, &slices, By::Test);
        if longest > MAX_CMDLINE {
            out.say(format!(
                "   test-level shards would need a {longest}-character command line (the cap is \
                 {MAX_CMDLINE}), so this binary is dealt BY MODULE instead."
            ));
            by = By::Module;
            (slices, covered, n, why) = deal(by);
        }
    }
    let slices: Vec<Option<Vec<String>>> = if whole {
        covered = census.keys().cloned().collect();
        vec![None]
    } else {
        slices.into_iter().map(Some).collect()
    };
    let _ = n;
    let joined: Vec<&String> = covered.iter().filter(|m| !asked.contains_key(*m)).collect();
    if !joined.is_empty() {
        // Welded in by a filter overlap. Their tests run, so they are part of this run's
        // denominator; saying nothing would be a wider run than the caller asked for, quietly.
        out.say(format!(
            "   {} module(s) joined this run because a selected module's filter also selects \
             their tests: {}",
            joined.len(),
            joined
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        ));
    }
    let listed: BTreeMap<String, Vec<String>> = census
        .iter()
        .filter(|(m, _)| covered.contains(*m))
        .map(|(m, t)| (m.clone(), t.clone()))
        .collect();
    let ntests: usize = listed.values().map(Vec::len).sum();
    out.say(format!(
        "{}::{} -- {ntests} tests in {} modules across {} {}, --test-threads=1 each, dealt by \
         {}{}",
        s.pkg,
        s.tier,
        listed.len(),
        slices.len(),
        if whole {
            "process (SERIAL, no filter)"
        } else {
            "shard(s)"
        },
        if whole {
            "the whole binary"
        } else if by == By::Test {
            "test"
        } else {
            "module"
        },
        if why.is_empty() {
            String::new()
        } else {
            format!("  [{why}]")
        }
    ));
    if !whole {
        let counts: Vec<usize> = slices
            .iter()
            .map(|sl| shard_test_count(sl.as_deref().unwrap_or(&[]), by, &all_names))
            .collect();
        match &tpath {
            Some(p) if !timings.is_empty() => {
                let known = all_names
                    .iter()
                    .filter(|t| timings.contains_key(*t))
                    .count();
                out.say(format!(
                    "   balanced on recorded time: {} ({known} of {} tests timed; the rest weigh \
                     the mean)",
                    p.strip_prefix(s.ws).unwrap_or(p).display(),
                    all_names.len()
                ));
            }
            _ => out.say("   balanced on test count: no recorded timings for this binary"),
        }
        for (k, sl) in slices.iter().enumerate() {
            let sl = sl.as_deref().unwrap_or(&[]);
            let sel: Vec<String> = match by {
                By::Test => sl.to_vec(),
                By::Module => sl
                    .iter()
                    .flat_map(|m| filter_selection(m, &all_names))
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect(),
            };
            let est = if timings.is_empty() {
                String::new()
            } else {
                let w = unit_weights(&BTreeMap::from([(0, sel)]), &timings);
                format!(" ~{:.0} s recorded", w[&0])
            };
            out.say(format!(
                "   shard {k}: {:4} tests, {:3} {}{est}",
                counts[k],
                sl.len(),
                if by == By::Test {
                    "test(s)"
                } else {
                    "module(s)"
                }
            ));
        }
        // The deal must be a partition, counted the way the shards will select: a union per shard,
        // not a sum per module. A first draft summed per module and so counted a welded group's
        // tests once per member: it cried "1,311 against 1,288" over a deal that ran exactly 1,288.
        // A guard that fires on a correct run is worse than none, because the next reader learns to
        // scroll past it.
        let dealt: usize = counts.iter().sum();
        if dealt != ntests {
            out.say(format!(
                "   THE DEAL IS NOT A PARTITION: {dealt} test(s) dealt against a {ntests}-test \
                 census. Some test would run in two shards, or in none."
            ));
        }
    }
    if let Err(e) = std::fs::create_dir_all(s.log_dir) {
        out.say(format!("   cannot create {}: {e}", s.log_dir.display()));
        return None;
    }
    let t0 = Instant::now();
    let mut running = Vec::new();
    let mut failed_to_launch = Vec::new();
    for (k, sl) in slices.iter().enumerate() {
        let log = s.log_dir.join(format!("{}-{}-shard{k}.log", s.pkg, s.tier));
        match run_shard(k, &exe_s, sl.clone(), by, s.ws, s.env, &log, t0) {
            Ok(r) => running.push(r),
            Err(e) => {
                let mut res = shard_result(
                    k,
                    sl.clone(),
                    -1,
                    &format!("failed to launch: {e}"),
                    0.0,
                    &log,
                    by,
                );
                res.label = format!("failed to launch: {e}");
                failed_to_launch.push(res);
            }
        }
    }
    let mut results = collect_all(running, t0);
    results.extend(failed_to_launch);
    results.sort_by_key(|r| r.k);
    let times: BTreeMap<String, f64> = results.iter().flat_map(|r| r.times.clone()).collect();
    if !times.is_empty() {
        let path = s
            .log_dir
            .join("timings")
            .join(format!("{}-{}.tsv", s.pkg, s.tier));
        match write_timings(&path, &times) {
            Ok(()) => out.say(format!(
                "   per-test wall time for {} test(s): {}",
                times.len(),
                path.display()
            )),
            Err(e) => out.say(format!("   per-test timings not written: {e}")),
        }
    }
    for r in &results {
        out.say(format!(
            "   shard {}: {:4} passed  {:3} failed  {:3} ignored  ({:.1} s)  exit {}{}",
            r.k,
            r.passed,
            r.failed,
            r.ignored,
            r.secs,
            r.code,
            if r.normal {
                String::new()
            } else {
                format!("  *** {} ***", r.label)
            }
        ));
        if !r.finished() {
            // Printed where it happened and not only in the summary: the abnormal exit and its
            // exact slice are diagnostic evidence and must not require opening a log.
            out.say(format!(
                "   shard {} {} -- {}: {}",
                r.k,
                if r.label.is_empty() {
                    "emitted NO result line"
                } else {
                    &r.label
                },
                r.by.items_word().to_uppercase(),
                r.items_text()
            ));
            out.say(format!("   shard {} log: {}", r.k, r.log.display()));
        }
    }
    if s.rerun_crashed {
        for r in results.iter_mut().filter(|r| !r.finished()) {
            out.say(format!(
                "   shard {} did not finish; re-running its slice ALONE before reporting",
                r.k
            ));
            let log = s
                .log_dir
                .join(format!("{}-{}-shard{}-rerun.log", s.pkg, s.tier, r.k));
            let t = Instant::now();
            let again = match run_shard(r.k, &exe_s, r.items.clone(), r.by, s.ws, s.env, &log, t) {
                Ok(run) => collect_all(vec![run], t).remove(0),
                Err(e) => shard_result(
                    r.k,
                    r.items.clone(),
                    -1,
                    &format!("failed to launch: {e}"),
                    0.0,
                    &log,
                    r.by,
                ),
            };
            out.say(format!(
                "   shard {} re-run ALONE: {} passed  {} failed  exit {}{}  ({:.1} s) -- the crash {}",
                r.k,
                again.passed,
                again.failed,
                again.code,
                if again.normal {
                    String::new()
                } else {
                    format!("  *** {} ***", again.label)
                },
                again.secs,
                if again.normal {
                    "did NOT reproduce"
                } else {
                    "REPRODUCED"
                }
            ));
            // **The re-run is evidence, not a replacement.** An earlier draft swapped a passing
            // re-run in for the crashed result, and the run then printed "every shard exited
            // normally" over a process that had died: a crash laundered into a green line by the
            // code written to catch it. The shard that died stays dead in this run's arithmetic; the
            // re-run answers only whether the crash reproduces alone.
            r.rerun = Some(Box::new(again));
        }
    }
    Some(Outcome {
        summary: merge_shards(&results),
        results,
        listed,
        areas,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| (*x).to_owned()).collect()
    }

    fn no_areas() -> BTreeSet<String> {
        BTreeSet::new()
    }

    /// Uneven on purpose (4, 3, 1, 1, 1): an even set cannot tell a balanced deal from a
    /// positional one. `alpha` and `alpha_extra` prove a filter has to carry its `::`.
    const SHARD_LIST: &str = "alpha::one: test\nalpha::two: test\nalpha::nested::three: test\n\
        beta::one: test\ngamma::solo: test\ngamma::two: test\ngamma::three: test\n\
        gamma::four: test\ndelta::only: test\nalpha_extra::one: test\n\n10 tests, 0 benchmarks\n";

    /// Shard 0 of two: `gamma` and `beta`, one of gamma's four ignored.
    const SHARD_LOG_A: &str =
        "running 5 tests\ntest beta::one ... ok\ntest gamma::four ... ignored\n\
        test gamma::solo ... ok\ntest gamma::three ... ok\ntest gamma::two ... ok\n\n\
        test result: ok. 4 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out\n";

    /// Shard 1 of two: `alpha`, `alpha_extra` and `delta`, one red whose status line is split by a
    /// printing test.
    const SHARD_LOG_B: &str = "running 5 tests\ntest alpha::nested::three ... ok\n\
        test alpha::one ... [world] loading landblock 0x1234\nFAILED\ntest alpha::two ... ok\n\
        test alpha_extra::one ... ok\ntest delta::only ... ok\n\nfailures:\n\n\
        ---- alpha::one stdout ----\nassertion failed: the fixture says so\n\nfailures:\n    alpha::one\n\n\
        test result: FAILED. 4 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out\n";

    /// The overlap that cost a real run 23 duplicated tests, in miniature and with the real names:
    /// `routing::` is a substring of `o164_click_routing::`. `solo` is the control.
    const SHARD_LIST_OVERLAP: &str = "routing::one: test\nrouting::two: test\n\
        o164_click_routing::a: test\no164_click_routing::b: test\nsolo::x: test\n\n5 tests, 0 benchmarks\n";

    /// A shard that died: two tests printed, no roll-call, no result line.
    const SHARD_LOG_CRASH: &str =
        "running 5 tests\ntest gamma::solo ... ok\ntest gamma::three ... ok\n";

    fn mods() -> BTreeMap<String, Vec<String>> {
        list_modules(SHARD_LIST, &no_areas())
    }

    fn counts() -> BTreeMap<String, f64> {
        mods()
            .into_iter()
            .map(|(m, t)| (m, count_f64(t.len())))
            .collect()
    }

    fn log(p: &str) -> PathBuf {
        PathBuf::from(p)
    }

    fn sh_a() -> ShardResult {
        shard_result(
            0,
            Some(s(&["gamma", "beta"])),
            0,
            SHARD_LOG_A,
            1.0,
            &log("a.log"),
            By::Module,
        )
    }

    fn sh_b() -> ShardResult {
        shard_result(
            1,
            Some(s(&["alpha", "alpha_extra", "delta"])),
            101,
            SHARD_LOG_B,
            1.0,
            &log("b.log"),
            By::Module,
        )
    }

    fn dead() -> ShardResult {
        shard_result(
            1,
            Some(s(&["alpha", "alpha_extra", "delta"])),
            -1_073_740_940,
            SHARD_LOG_CRASH,
            1.0,
            &log("c.log"),
            By::Module,
        )
    }

    /// `--list` groups into modules on the first `::`, and the `N tests, M benchmarks` tail is not
    /// a test.
    #[test]
    fn a_list_dump_groups_into_modules_and_its_tail_is_not_a_test() {
        let c: BTreeMap<String, usize> = mods().into_iter().map(|(m, t)| (m, t.len())).collect();
        let want: BTreeMap<String, usize> = [
            ("alpha", 3),
            ("alpha_extra", 1),
            ("beta", 1),
            ("delta", 1),
            ("gamma", 4),
        ]
        .into_iter()
        .map(|(m, n)| (m.to_owned(), n))
        .collect();
        assert_eq!(c, want);
        assert_eq!(c.values().sum::<usize>(), 10);
    }

    /// A deal is disjoint and complete, balanced by test count where a positional deal is not, the
    /// same every time, and never deals more shards than units.
    #[test]
    fn a_deal_is_a_balanced_reproducible_partition() {
        let deal = deal_shards(&counts(), 2);
        let mut all: Vec<String> = deal.iter().flatten().cloned().collect();
        all.sort();
        assert_eq!(all, counts().keys().cloned().collect::<Vec<_>>());
        let load = |sh: &[String]| sh.iter().map(|m| counts()[m]).sum::<f64>();
        let mut loads: Vec<f64> = deal.iter().map(|sh| load(sh)).collect();
        loads.sort_by(f64::total_cmp);
        assert_eq!(loads, vec![5.0, 5.0]);
        // The mistake worth guarding against: dealing by position.
        let keys: Vec<String> = counts().keys().cloned().collect();
        let naive: Vec<Vec<String>> = vec![
            keys.iter().step_by(2).cloned().collect(),
            keys.iter().skip(1).step_by(2).cloned().collect(),
        ];
        let mut naive_loads: Vec<f64> = naive.iter().map(|sh| load(sh)).collect();
        naive_loads.sort_by(f64::total_cmp);
        assert_eq!(naive_loads, vec![2.0, 8.0]);
        assert_eq!(deal_shards(&counts(), 2), deal);
        assert_eq!(
            deal_shards(&counts(), 99)
                .iter()
                .map(Vec::len)
                .collect::<Vec<_>>(),
            vec![1, 1, 1, 1, 1]
        );
    }

    /// A module shard's filter is `<mod>::`, a test shard names whole tests under `--exact`, and
    /// the serial form carries no filter at all.
    #[test]
    fn a_shard_command_carries_the_filter_its_grain_needs() {
        assert_eq!(
            shard_argv("t.exe", Some(&s(&["gamma", "beta"])), By::Module),
            s(&["t.exe", "--test-threads=1", "gamma::", "beta::"])
        );
        assert_eq!(
            shard_argv("t.exe", Some(&s(&["gamma::solo", "alpha::one"])), By::Test),
            s(&[
                "t.exe",
                "--test-threads=1",
                "--exact",
                "gamma::solo",
                "alpha::one"
            ])
        );
        assert_eq!(
            shard_argv("t.exe", None, By::Module),
            s(&["t.exe", "--test-threads=1"])
        );
    }

    /// libtest's filters are substring matches: a bare `alpha` sweeps in `alpha_extra`, the
    /// `alpha::` form does not, and `routing::` reaches into `o164_click_routing`.
    #[test]
    fn a_filter_is_a_substring_match_so_it_carries_its_separator() {
        let names: Vec<String> = LIST_LINE
            .captures_iter(SHARD_LIST)
            .map(|m| m[1].to_owned())
            .collect();
        let bare: Vec<&String> = names.iter().filter(|n| n.contains("alpha")).collect();
        assert_eq!(
            bare,
            [
                "alpha::one",
                "alpha::two",
                "alpha::nested::three",
                "alpha_extra::one"
            ]
        );
        assert_eq!(
            filter_selection("alpha", &names),
            s(&["alpha::one", "alpha::two", "alpha::nested::three"])
        );
        let over: Vec<String> = list_modules(SHARD_LIST_OVERLAP, &no_areas())
            .into_values()
            .flatten()
            .collect();
        let mut reached = filter_selection("routing", &over);
        reached.sort();
        assert_eq!(
            reached,
            s(&[
                "o164_click_routing::a",
                "o164_click_routing::b",
                "routing::one",
                "routing::two"
            ])
        );
    }

    /// Overlapping filters weld their modules into one group whose weight counts each test once;
    /// modules nothing reaches stay apart; and at any N the deal runs every test exactly once.
    #[test]
    fn overlapping_filters_weld_modules_so_every_deal_is_a_partition() {
        let over = list_modules(SHARD_LIST_OVERLAP, &no_areas());
        let names: Vec<String> = over.values().flatten().cloned().collect();
        let groups = filter_groups(&over);
        assert_eq!(
            groups.iter().map(|(g, _)| g.clone()).collect::<Vec<_>>(),
            vec![s(&["o164_click_routing", "routing"]), s(&["solo"])]
        );
        assert_eq!(
            groups.iter().map(|(_, t)| t.len()).collect::<Vec<_>>(),
            vec![4, 1]
        );
        assert_eq!(
            filter_groups(&mods())
                .iter()
                .map(|(g, _)| g.clone())
                .collect::<Vec<_>>(),
            vec![
                s(&["alpha"]),
                s(&["alpha_extra"]),
                s(&["beta"]),
                s(&["delta"]),
                s(&["gamma"])
            ]
        );
        // A welded shard counts its tests once; summing the two filters double-counts.
        assert_eq!(
            shard_test_count(&s(&["o164_click_routing", "routing"]), By::Module, &names),
            4
        );
        let summed: usize = ["o164_click_routing", "routing"]
            .iter()
            .map(|m| filter_selection(m, &names).len())
            .sum();
        assert_eq!(summed, 6);
        assert_eq!(shard_test_count(&s(&["a::b", "c::d"]), By::Test, &names), 2);
        let units: BTreeMap<Vec<String>, Vec<String>> = groups.into_iter().collect();
        for n in [2, 3, 5] {
            let mut dealt: Vec<String> = deal_shards(&unit_weights(&units, &BTreeMap::new()), n)
                .into_iter()
                .flatten()
                .flat_map(|g| units[&g].clone())
                .collect();
            dealt.sort();
            let mut want = names.clone();
            want.sort();
            assert_eq!(dealt, want, "at N={n}");
        }
    }

    /// Exit 0 and 101 are libtest's; a heap corruption is named in both of its spellings, as are an
    /// access violation and a stack buffer overrun; an uncatalogued code is still abnormal.
    #[test]
    fn only_libtests_two_exit_codes_are_normal_and_known_crashes_are_named() {
        assert_eq!(exit_status(0), (true, String::new()));
        assert_eq!(exit_status(101), (true, String::new()));
        let heap = "STATUS_HEAP_CORRUPTION (0xC0000374)".to_owned();
        assert_eq!(exit_status(0xC000_0374), (false, heap.clone()));
        assert_eq!(exit_status(-1_073_740_940), (false, heap));
        assert_eq!(
            exit_status(0xC000_0005),
            (false, "STATUS_ACCESS_VIOLATION (0xC0000005)".to_owned())
        );
        assert_eq!(
            exit_status(0xC000_0409),
            (false, "STATUS_STACK_BUFFER_OVERRUN (0xC0000409)".to_owned())
        );
        assert!(!exit_status(7).0);
    }

    /// A finished shard is counted; a red shard's names come from the roll-call; a crashed shard
    /// has no result line and is not a run.
    #[test]
    fn a_shard_is_read_from_its_exit_code_and_its_result_line() {
        let a = sh_a();
        assert_eq!(
            (a.passed, a.failed, a.ignored, a.result_line),
            (4, 0, 1, true)
        );
        assert_eq!(sh_b().names, s(&["alpha::one"]));
        let d = dead();
        assert_eq!((d.result_line, d.normal, d.passed), (false, false, 0));
    }

    /// A merge carries a crashed shard in `bad` while its totals alone read as a clean smaller run;
    /// a crash that passed alone on its re-run is still in `bad`, with the re-run readable; names
    /// union across shards; and finished shards leave `bad` empty.
    #[test]
    fn a_merge_never_lets_a_dead_shard_read_as_a_smaller_green_run() {
        let merged = merge_shards(&[sh_a(), dead()]);
        assert_eq!(merged.bad.iter().map(|r| r.k).collect::<Vec<_>>(), vec![1]);
        assert_eq!(
            (merged.passed, merged.failed, merged.names.len()),
            (4, 0, 0)
        );
        let mut recovered = dead();
        recovered.rerun = Some(Box::new(sh_a()));
        let m = merge_shards(&[sh_a(), recovered]);
        assert_eq!(m.bad.iter().map(|r| r.k).collect::<Vec<_>>(), vec![1]);
        assert!(m.bad[0].rerun.as_ref().expect("the re-run").normal);
        let clean = merge_shards(&[sh_a(), sh_b()]);
        assert_eq!(clean.names, s(&["alpha::one"]));
        assert!(clean.bad.is_empty());
    }

    /// Two finished shards attribute every module; a census that no longer adds up refuses to
    /// attribute; a dead shard's modules are incomplete while the survivor's keep their numbers.
    #[test]
    fn per_module_attribution_is_checked_and_a_crash_costs_only_its_slice() {
        let (per, incomplete) = attribute_all(&[sh_a(), sh_b()], &mods());
        let want: PerModule = [
            ("alpha", (2, 1, 0)),
            ("alpha_extra", (1, 0, 0)),
            ("beta", (1, 0, 0)),
            ("delta", (1, 0, 0)),
            ("gamma", (3, 0, 1)),
        ]
        .into_iter()
        .map(|(m, v)| (m.to_owned(), v))
        .collect();
        assert_eq!(per, Some(want));
        assert!(incomplete.is_empty());
        let mut drifted = mods();
        drifted
            .get_mut("alpha")
            .expect("alpha")
            .push("alpha::a_test_added_since".to_owned());
        assert_eq!(attribute_all(&[sh_a(), sh_b()], &drifted).0, None);
        let (part, incomplete) = attribute_all(&[sh_a(), dead()], &mods());
        let part = part.expect("the survivor attributes");
        assert_eq!(
            incomplete,
            ["alpha", "alpha_extra", "delta"]
                .into_iter()
                .map(str::to_owned)
                .collect()
        );
        assert_eq!((part["beta"], part["gamma"]), ((1, 0, 0), (3, 0, 1)));
        assert_eq!((part["alpha"], part["delta"]), ((0, 0, 0), (0, 0, 0)));
    }

    /// A test-level deal splits the biggest module across shards where a module deal cannot, and
    /// the module is still attributed whole.
    #[test]
    fn a_test_level_deal_splits_a_module_and_still_attributes_it_whole() {
        let tests: BTreeMap<String, f64> =
            mods().into_values().flatten().map(|t| (t, 1.0)).collect();
        let slices = deal_shards(&tests, 4);
        let mut all: Vec<String> = slices.iter().flatten().cloned().collect();
        all.sort();
        assert_eq!(all, tests.keys().cloned().collect::<Vec<_>>());
        let holding = slices
            .iter()
            .filter(|sl| sl.iter().any(|t| t.starts_with("gamma::")))
            .count();
        assert_eq!(holding, 4);
        let module_deal = deal_shards(&counts(), 4);
        assert_eq!(
            module_deal
                .iter()
                .filter(|sl| sl.contains(&"gamma".to_owned()))
                .count(),
            1
        );
        let shards = [
            shard_result(0, Some(s(&["gamma::solo", "gamma::two"])), 0,
                "test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n", 1.0, &log("t0.log"), By::Test),
            shard_result(1, Some(s(&["gamma::three", "gamma::four"])), 0,
                "test gamma::four ... ignored\n\ntest result: ok. 1 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out\n",
                1.0, &log("t1.log"), By::Test),
        ];
        let gamma: BTreeMap<String, Vec<String>> = [("gamma".to_owned(), mods()["gamma"].clone())]
            .into_iter()
            .collect();
        let (per, incomplete) = attribute_all(&shards, &gamma);
        assert_eq!(per.expect("attributed")["gamma"], (3, 0, 1));
        assert!(incomplete.is_empty());
    }

    /// `--serial` forces one process; `--shards N` beats the default; the two dat binaries default
    /// to four; the client gpu binary follows the cores while the render one stays one; the
    /// environment overrides every sharded default and reaches no other binary; `--serial` and
    /// `--shards` beat it; an unreadable value is ignored.
    #[test]
    fn the_process_count_follows_its_order_of_precedence() {
        let c = |pkg, tier, shards, serial, env: Option<&str>| {
            shard_count(pkg, tier, shards, serial, env, Some(16))
        };
        assert_eq!(c("dereth-client", "dat", None, true, None), 1);
        assert_eq!(c("dereth-client", "dat", Some(7), false, None), 7);
        assert_eq!(
            (
                c("dereth-client", "dat", None, false, None),
                c("dereth-testkit", "dat", None, false, None)
            ),
            (4, 4)
        );
        assert_eq!(
            (
                c("dereth-client", "gpu", None, false, None),
                c("dereth-render", "gpu", None, false, None)
            ),
            (auto_shards(Some(16)), 1)
        );
        assert_eq!(c("dereth-client", "gpu", Some(2), false, None), 2);
        assert_eq!(
            (
                c("dereth-client", "gpu", None, false, Some("3")),
                c("dereth-client", "dat", None, false, Some("3"))
            ),
            (3, 3)
        );
        assert_eq!(
            (
                c("dereth-render", "gpu", None, false, Some("3")),
                c("dereth-client-model", "cpu", None, false, Some("3"))
            ),
            (1, 1)
        );
        assert_eq!(
            (
                c("dereth-client", "gpu", None, true, Some("3")),
                c("dereth-client", "gpu", Some(5), false, Some("3"))
            ),
            (1, 5)
        );
        assert_eq!(c("dereth-client", "dat", None, false, Some("zero")), 4);
        assert_eq!(c("dereth-client", "dat", None, false, Some("0")), 4);
    }

    /// The automatic count is cores over two, at least one, at most the cap.
    #[test]
    fn the_automatic_count_is_half_the_cores_within_its_bounds() {
        assert_eq!(
            auto_shards(Some(16)),
            (16 / AUTO_CORES_PER_SHARD).min(AUTO_MAX_SHARDS)
        );
        assert_eq!(auto_shards(Some(1)), 1);
        assert_eq!(auto_shards(None), 1);
        assert_eq!(auto_shards(Some(256)), AUTO_MAX_SHARDS);
    }

    /// An area binary keys on `<area>::<stem>` and a flat one on its first component; a test in
    /// the area's own `mod.rs` welds its area's stems into one unit.
    #[test]
    fn an_area_binary_keys_on_area_and_stem() {
        let list = "rendering::sky::a: test\nrendering::sky::b: test\n\
            rendering::fog::inner::c: test\nrendering::loose: test\ncommon_flat::d: test\n";
        let areas: BTreeSet<String> = ["rendering".to_owned()].into_iter().collect();
        let keyed: BTreeMap<String, usize> = list_modules(list, &areas)
            .into_iter()
            .map(|(m, t)| (m, t.len()))
            .collect();
        let want: BTreeMap<String, usize> = [
            ("rendering::sky", 2),
            ("rendering::fog", 1),
            ("rendering", 1),
            ("common_flat", 1),
        ]
        .into_iter()
        .map(|(m, n)| (m.to_owned(), n))
        .collect();
        assert_eq!(keyed, want);
        assert_eq!(
            list_modules(list, &no_areas())
                .into_keys()
                .collect::<Vec<_>>(),
            s(&["common_flat", "rendering"])
        );
        let groups = filter_groups(&list_modules(list, &areas));
        assert_eq!(
            groups.into_iter().map(|(g, _)| g).collect::<Vec<_>>(),
            vec![
                s(&["common_flat"]),
                s(&["rendering", "rendering::fog", "rendering::sky"])
            ]
        );
    }

    /// Each test is timed from its name to its verdict, a test whose verdict never came is timed to
    /// the last byte, and output with no test lines times nothing.
    #[test]
    fn each_test_is_timed_from_its_name_to_its_verdict() {
        let chunks: Chunks = vec![
            (10.0, b"running 3 tests\ntest m::a ... ".to_vec()),
            (13.0, b"ok\ntest m::b ... ".to_vec()),
            (15.0, b"noise from a child\n".to_vec()),
            (20.0, b"FAILED\ntest m::c ... ".to_vec()),
            (26.0, b"\n".to_vec()),
        ];
        let want: BTreeMap<String, f64> = [("m::a", 3.0), ("m::b", 7.0), ("m::c", 6.0)]
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v))
            .collect();
        assert_eq!(test_times(&chunks), want);
        assert!(test_times(&[(
            1.0,
            b"running 0 tests\n\ntest result: ok. 0 passed; 0 failed;".to_vec()
        )])
        .is_empty());
    }

    /// With timings a unit weighs its seconds and a test the file does not name weighs the mean;
    /// without them it weighs its count; by time a 100 s module gets a shard to itself, by count it
    /// shares one.
    #[test]
    fn a_deal_balances_on_recorded_time_when_there_is_any() {
        let units: BTreeMap<String, Vec<String>> = [
            ("slow".to_owned(), s(&["slow::t"])),
            (
                "many".to_owned(),
                (0..9).map(|i| format!("many::{i}")).collect(),
            ),
            ("few".to_owned(), s(&["few::a", "few::b"])),
        ]
        .into_iter()
        .collect();
        let mut t: BTreeMap<String, f64> = [("slow::t", 100.0), ("few::a", 1.0), ("few::b", 1.0)]
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v))
            .collect();
        t.extend((0..9).map(|i| (format!("many::{i}"), 1.0)));
        let w = unit_weights(&units, &t);
        assert_eq!((w["slow"], w["many"], w["few"]), (100.0, 9.0, 2.0));
        let c = unit_weights(&units, &BTreeMap::new());
        assert_eq!((c["slow"], c["many"], c["few"]), (1.0, 9.0, 2.0));
        let mean = unit_weights(
            &BTreeMap::from([("new".to_owned(), s(&["new::x"]))]),
            &[("a", 2.0), ("b", 4.0)]
                .into_iter()
                .map(|(k, v)| (k.to_owned(), v))
                .collect(),
        );
        assert_eq!(mean["new"], 3.0);
        assert_eq!(deal_shards(&w, 2), vec![s(&["slow"]), s(&["many", "few"])]);
        assert_eq!(deal_shards(&c, 2), vec![s(&["many"]), s(&["few", "slow"])]);
    }

    /// Both dat binaries are dealt by module, and any other binary falls back to modules.
    #[test]
    fn every_binary_is_dealt_by_module_unless_asked() {
        assert_eq!(
            (
                shard_by("dereth-testkit", "dat"),
                shard_by("dereth-client", "dat")
            ),
            (By::Module, By::Module)
        );
        assert_eq!(shard_by("dereth-ui", "dat"), By::Module);
        assert_eq!(
            shard_count("dereth-client-model", "cpu", None, false, None, Some(16)),
            1
        );
    }

    /// The unit ceiling and the RAM ceiling both bite, and a host that will not say its RAM says
    /// so rather than guessing.
    #[test]
    fn the_process_count_is_capped_by_units_and_by_memory() {
        let cap =
            |want, items, ram| cap_shards(want, items, ram, SHARD_PEAK_GB, SHARD_RAM_BUDGET).0;
        assert_eq!(
            (
                cap(4, 2, Some(64.0)),
                cap(4, 99, Some(8.0)),
                cap(4, 99, Some(64.0))
            ),
            (2, 2, 4)
        );
        assert!(cap_shards(4, 99, None, SHARD_PEAK_GB, SHARD_RAM_BUDGET)
            .1
            .contains("NOT applied"));
    }

    /// A test-level deal of a large binary would need a command line past the cap, so it is dealt by
    /// module instead; a module deal of the same binary fits.
    #[test]
    fn a_test_level_deal_that_would_overflow_the_command_line_is_refused() {
        let tests: Vec<String> = (0..1200)
            .map(|i| {
                format!(
                    "area::module_{}::a_test_named_for_what_it_proves_{i}",
                    i % 40
                )
            })
            .collect();
        let by_test = vec![tests[..600].to_vec(), tests[600..].to_vec()];
        assert!(longest_command("t.exe", &by_test, By::Test) > MAX_CMDLINE);
        let by_module: Vec<Vec<String>> =
            vec![(0..20).map(|i| format!("area::module_{i}")).collect()];
        assert!(longest_command("t.exe", &by_module, By::Module) < MAX_CMDLINE);
    }

    /// Timings written and read back are the same, slowest first on disk.
    #[test]
    fn timings_round_trip_through_their_file() {
        let dir = std::env::temp_dir().join(format!("xtask-sweep-timings-{}", std::process::id()));
        let path = dir.join("t.tsv");
        let t: BTreeMap<String, f64> = [("a", 1.5), ("b", 20.25)]
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v))
            .collect();
        write_timings(&path, &t).expect("write");
        assert_eq!(load_timings(&path), t);
        let text = std::fs::read_to_string(&path).expect("read");
        assert!(text.starts_with("test\tseconds\nb\t20.250\n"), "{text}");
        std::fs::remove_dir_all(&dir).ok();
    }
}
