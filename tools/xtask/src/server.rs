//! The server's steps in the CI tiers: its unit tier and that tier's time budget, its source
//! hygiene (tier 0), and its real-content tier (tier 1).
//!
//! The server's crates are `empyrean-*`.

use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::util::{run, workspace_root, NotRun, Outcome, Profile, Report};

/// `-p empyrean-*`: every server crate (cargo expands the glob).
pub const SERVER_CRATES: &[&str] = &["-p", "empyrean-*"];

/// Time a command's run, streaming its output.
fn timed(dir: &Path, program: &str, args: &[&str]) -> (bool, f64) {
    let t0 = Instant::now();
    let ok = run(dir, program, args);
    (ok, t0.elapsed().as_secs_f64())
}

fn pass_fail(ok: bool) -> Outcome {
    if ok {
        Outcome::Pass
    } else {
        Outcome::Fail
    }
}

/// The server's unit tier: its tests built (`--no-run`), then run, so that the run can be timed
/// on its own and judged against this host's history ([`Budget`]).
pub fn unit_tier(profile: Profile) -> Vec<Report> {
    let ws = workspace_root();
    let mut build = vec!["test"];
    build.extend_from_slice(profile.cargo_args());
    build.extend_from_slice(SERVER_CRATES);
    let test = build.clone();
    build.push("--no-run");
    let (built, _) = timed(&ws, "cargo", &build);
    if !built {
        return vec![Report::new(
            "server unit tier (cargo test -p empyrean-*)",
            Outcome::Fail,
            "the tests did not build",
        )];
    }
    // The run alone, which is what the budget is about.
    let (ok, secs) = timed(&ws, "cargo", &test);
    let mut rows = vec![Report::new(
        "server unit tier (cargo test -p empyrean-*)",
        pass_fail(ok),
        format!("{secs:.1}s"),
    )];
    // A failed run's time says nothing about the host: it is neither judged nor recorded.
    if ok {
        let budget = Budget::here(profile);
        let (within, note) = budget.judge(secs, true);
        rows.push(Report::new(
            "server unit-tier budget",
            pass_fail(within),
            note,
        ));
    }
    rows
}

/// The unit tier's time budget, relative to the host it runs on.
///
/// Each passing run's time is kept in a history under cargo's target directory, one file per
/// profile and logical-core count (a dev run is not judged against optimised ones, and a run
/// pinned to fewer cores is not judged against the whole machine). With [`HISTORY_MIN_RUNS`] or
/// more runs recorded, a run fails when it is slower than the history's median by more than half
/// and by more than [`REGRESSION_FLOOR_S`]. With fewer, the limit is [`TEST_BUDGET_S`] scaled to
/// the host's cores: `TEST_BUDGET_S * max(1, sqrt(16 / cores))`, so 60 s from 16 cores up, 120 s
/// on 4 and 170 s on 2 -- a small host is not a regression. A run over its limit is not recorded,
/// so one slow run cannot raise the bar for the next.
#[derive(Debug, Clone)]
pub struct Budget {
    /// The history file.
    pub path: PathBuf,
    /// The logical cores this process may run on.
    pub cores: usize,
}

/// The fixed budget of a host with [`BUDGET_REFERENCE_CORES`] or more, in seconds.
pub const TEST_BUDGET_S: f64 = 60.0;
/// The core count the fixed budget is set for.
pub const BUDGET_REFERENCE_CORES: usize = 16;
/// Runs recorded before the host's own median replaces the fixed budget.
pub const HISTORY_MIN_RUNS: usize = 3;
/// Runs kept in a history.
pub const HISTORY_KEEP: usize = 20;
/// How much slower than the median a run may be, as a factor.
pub const REGRESSION_FACTOR: f64 = 1.5;
/// How much slower than the median a run may always be, in seconds.
pub const REGRESSION_FLOOR_S: f64 = 10.0;

impl Budget {
    /// The budget of this host for `profile`, its history in cargo's target directory.
    pub fn here(profile: Profile) -> Self {
        let cores = std::thread::available_parallelism().map_or(1, usize::from);
        let name = match profile {
            Profile::TestRelease => "test-release",
            Profile::Dev => "dev",
        };
        let path = crate::util::target_dir()
            .join("xtask")
            .join("unit-tier-budget")
            .join(format!("{name}-{cores}-cores.json"));
        Budget { path, cores }
    }

    /// The fixed budget scaled to `cores`.
    pub fn scaled(cores: usize) -> f64 {
        let ratio = BUDGET_REFERENCE_CORES as f64 / cores.max(1) as f64;
        TEST_BUDGET_S * ratio.sqrt().max(1.0)
    }

    /// The limit in seconds for a run with `history` behind it, and how it was set.
    pub fn limit(history: &[f64], cores: usize) -> (f64, String) {
        if history.len() >= HISTORY_MIN_RUNS {
            let mut sorted = history.to_vec();
            sorted.sort_by(f64::total_cmp);
            let mid = sorted.len() / 2;
            let median = if sorted.len().is_multiple_of(2) {
                (sorted[mid - 1] + sorted[mid]) / 2.0
            } else {
                sorted[mid]
            };
            let limit = (median * REGRESSION_FACTOR).max(median + REGRESSION_FLOOR_S);
            (
                limit,
                format!(
                    "host median {median:.1}s over {} runs, +50%/+10s",
                    history.len()
                ),
            )
        } else {
            (
                Self::scaled(cores),
                format!("no host history yet: {TEST_BUDGET_S:.0}s scaled to {cores} cores"),
            )
        }
    }

    /// The recorded run times, oldest first; none when the file is absent or damaged.
    pub fn history(&self) -> Vec<f64> {
        std::fs::read_to_string(&self.path)
            .ok()
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
            .and_then(|v| {
                v["unit_tier_s"]
                    .as_array()
                    .map(|a| a.iter().filter_map(serde_json::Value::as_f64).collect())
            })
            .unwrap_or_default()
    }

    fn write(&self, history: &[f64]) -> std::io::Result<()> {
        let keep = &history[history.len().saturating_sub(HISTORY_KEEP)..];
        let rounded: Vec<f64> = keep.iter().map(|x| (x * 100.0).round() / 100.0).collect();
        let doc = serde_json::json!({
            "format": "dereth-gate-history/1",
            "logical_cores": self.cores,
            "unit_tier_s": rounded,
        });
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = serde_json::to_string_pretty(&doc).map_err(std::io::Error::other)?;
        std::fs::write(&self.path, text + "\n")
    }

    /// Judge one passing run of `secs` seconds, and record it when it is within the limit and
    /// `record` is set: whether it is within, and a note for the table.
    pub fn judge(&self, secs: f64, record: bool) -> (bool, String) {
        let history = self.history();
        let (limit, mut how) = Self::limit(&history, self.cores);
        let within = secs <= limit;
        if within && record {
            let mut next = history;
            next.push(secs);
            if let Err(e) = self.write(&next) {
                how.push_str(&format!("; history not written: {e}"));
            }
        }
        (within, format!("{secs:.1}s of {limit:.1}s ({how})"))
    }
}

/// The source rules the server's code keeps, one finding per line.
///
/// No `todo!()` or `unimplemented!()` (an unported path says so with `not_ported!`), no path into
/// the pinned ACE checkout or the knowledge base (ACE is cited as `Source/...`), no attribution
/// trailer, and every file with `// ACE:` anchors opens with the port header, which names ACE and
/// the licence the port carries.
pub fn hygiene_findings(ws: &Path) -> Vec<String> {
    let rules: Vec<(regex::Regex, &str)> = [
        (r"\btodo!\s*\(", "todo!() - use not_ported!"),
        (
            r"\bunimplemented!\s*\(",
            "unimplemented!() - use not_ported!",
        ),
        (
            r"references/ACE",
            "cite ACE as Source/..., never references/...",
        ),
        (
            concat!("know", r"ledge/\d\d-"),
            "no knowledge-base paths in server code",
        ),
        (
            concat!("Co-Authored", "-By|Generated with \\[Claude"),
            "no attribution trailers",
        ),
    ]
    .into_iter()
    .map(|(rx, why)| (regex::Regex::new(rx).expect("a valid rule"), why))
    .collect();
    let anchor = regex::Regex::new(r"(?m)^\s*//\s*ACE:").expect("a valid rule");
    const HEADER: &str = "Ported from ACE (ACEmulator),";

    let mut files = Vec::new();
    collect_rs(&ws.join("empyrean"), &mut files);
    files.sort();
    let mut bad = Vec::new();
    for path in files {
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let text = String::from_utf8_lossy(&bytes);
        let rel = path
            .strip_prefix(ws)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        for (rx, why) in &rules {
            for m in rx.find_iter(&text) {
                let line = text[..m.start()].matches('\n').count() + 1;
                bad.push(format!("{rel}:{line}: {why}"));
            }
        }
        let head = &text[..text.char_indices().nth(600).map_or(text.len(), |(i, _)| i)];
        if anchor.is_match(&text) && !head.contains(HEADER) {
            bad.push(format!(
                "{rel}:1: has `// ACE:` anchors but no '{HEADER}' header"
            ));
        }
    }
    bad
}

fn collect_rs(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            if p.file_name().is_some_and(|n| n == "target") {
                continue;
            }
            collect_rs(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

/// `cargo xtask server-hygiene`.
pub fn server_hygiene() -> i32 {
    let bad = hygiene_findings(&workspace_root());
    for b in &bad {
        println!("  {b}");
    }
    println!("server hygiene: {} finding(s)", bad.len());
    i32::from(!bad.is_empty())
}

/// `world.pack` as the real-content tests find it: `EMPYREAN_TEST_WORLD_PACK`, else `world.pack`
/// in this workspace, else in the main checkout's when this checkout is a linked git worktree.
pub fn world_pack() -> Option<PathBuf> {
    if let Some(v) = std::env::var_os("EMPYREAN_TEST_WORLD_PACK").filter(|v| !v.is_empty()) {
        let p = PathBuf::from(v);
        return p.is_file().then_some(p);
    }
    let mut bases = vec![workspace_root()];
    bases.extend(main_workspace());
    bases
        .into_iter()
        .map(|b| b.join("world.pack"))
        .find(|p| p.is_file())
}

/// The workspace of the main checkout when this checkout is a linked git worktree: the same
/// place below the main checkout as this workspace is below its own checkout.
fn main_workspace() -> Option<PathBuf> {
    let ws = workspace_root().canonicalize().ok()?;
    let rev_parse = |what: &str| -> Option<PathBuf> {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(&ws)
            .args(["rev-parse", "--path-format=absolute", what])
            .output()
            .ok()
            .filter(|o| o.status.success())?;
        PathBuf::from(String::from_utf8_lossy(&out.stdout).trim())
            .canonicalize()
            .ok()
    };
    let checkout = rev_parse("--show-toplevel")?;
    // The main checkout holds the common git directory; a linked worktree's own top differs.
    let main = rev_parse("--git-common-dir")?.parent()?.to_path_buf();
    if main == checkout {
        return None;
    }
    let below = ws.strip_prefix(&checkout).ok()?;
    Some(main.join(below))
}

/// The server's real-content tier (tier 1): the tests that read the retail dats and `world.pack`,
/// each input passed to cargo explicitly. An absent input is an oracle nobody consulted, so every
/// row reports NO-ORACLE naming the variable, and nothing is built.
pub fn real_content(profile: Profile, dats: Option<&Path>) -> Vec<Report> {
    const TIERS: &[(&str, &[&str])] = &[
        ("empyrean-world", &[]),
        ("empyrean-testkit", &["--test", "all"]),
        ("empyrean-server", &["--test", "all"]),
    ];
    let pack = world_pack();
    let mut missing = Vec::new();
    if dats.is_none() {
        missing.push("the retail dats (set DERETH_TEST_DAT_DIR)");
    }
    if pack.is_none() {
        missing.push("world.pack (set EMPYREAN_TEST_WORLD_PACK)");
    }
    let (Some(dats), Some(pack)) = (dats, pack) else {
        return TIERS
            .iter()
            .map(|(krate, _)| {
                Report::new(
                    format!("server real content: {krate}"),
                    Outcome::NotRun(NotRun::OracleAbsent),
                    format!("not found: {}", missing.join(", ")),
                )
            })
            .collect();
    };
    println!(
        "--- server real content: DERETH_TEST_DAT_DIR={} EMPYREAN_TEST_WORLD_PACK={}",
        dats.display(),
        pack.display()
    );
    let ws = workspace_root();
    TIERS
        .iter()
        .map(|(krate, extra)| {
            let mut argv: Vec<&str> = vec!["test", "-q"];
            argv.extend_from_slice(profile.cargo_args());
            argv.extend_from_slice(&["-p", krate, "--features", "real-content"]);
            argv.extend_from_slice(extra);
            println!("$ cargo {}", argv.join(" "));
            let t0 = Instant::now();
            let ok = std::process::Command::new("cargo")
                .args(&argv)
                .current_dir(&ws)
                .env("DERETH_TEST_DAT_DIR", dats)
                .env("EMPYREAN_TEST_WORLD_PACK", &pack)
                .status()
                .is_ok_and(|s| s.success());
            Report::new(
                format!("server real content: {krate}"),
                pass_fail(ok),
                format!("{:.1}s", t0.elapsed().as_secs_f64()),
            )
        })
        .collect()
}

/// The ACE-World release the server pins, read from the constant the importer's tests and its
/// `fetch` use (`PINNED_TAG` in `empyrean-common`'s `world_release.rs`).
fn pinned_world_tag() -> Option<String> {
    let src = std::fs::read_to_string(
        workspace_root().join("empyrean/crates/common/src/world_release.rs"),
    )
    .ok()?;
    let rest = src.split("pub const PINNED_TAG: &str = \"").nth(1)?;
    rest.split('"').next().map(str::to_owned)
}

/// The per-user folder `empyrean-import fetch` caches ACE-World's dumps in: `%LOCALAPPDATA%` on
/// Windows, `~/Library/Caches` on macOS, `$XDG_CACHE_HOME` or `~/.cache` elsewhere.
fn world_fetch_cache() -> Option<PathBuf> {
    let var = |name: &str| {
        std::env::var_os(name)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
    };
    let base = if cfg!(windows) {
        var("LOCALAPPDATA")?.join("Empyrean")
    } else if cfg!(target_os = "macos") {
        var("HOME")?.join("Library/Caches/Empyrean")
    } else {
        var("XDG_CACHE_HOME")
            .or_else(|| var("HOME").map(|h| h.join(".cache")))?
            .join("empyrean")
    };
    Some(base.join("world-database"))
}

/// ACE's world-database dump as the importer's real-dump tests find it: `EMPYREAN_WORLD_SQL`,
/// else the pinned dump under `world-database/` in this workspace or the main checkout's, else
/// the pinned dump in `empyrean-import fetch`'s cache.
pub fn world_sql() -> Option<PathBuf> {
    if let Some(v) = std::env::var_os("EMPYREAN_WORLD_SQL").filter(|v| !v.is_empty()) {
        let p = PathBuf::from(v);
        return p.is_file().then_some(p);
    }
    let name = format!("ACE-World-Database-{}.sql", pinned_world_tag()?);
    let mut places: Vec<PathBuf> = std::iter::once(workspace_root())
        .chain(main_workspace())
        .map(|b| b.join("world-database"))
        .collect();
    places.extend(world_fetch_cache());
    places
        .into_iter()
        .map(|d| d.join(&name))
        .find(|p| p.is_file())
}

/// The importer's real-dump tests (tier 1): the pack built from ACE's real world dump. They read
/// the dump only, so they run whenever it is found; an absent dump is an oracle nobody
/// consulted, reported NO-ORACLE with the command that fetches it.
pub fn real_dump(profile: Profile) -> Report {
    const NAME: &str = "server real content: empyrean-import (real dump)";
    let Some(sql) = world_sql() else {
        return Report::new(
            NAME,
            Outcome::NotRun(NotRun::OracleAbsent),
            "not found: ACE's world dump (run `cargo run -p empyrean-import -- fetch`, or set EMPYREAN_WORLD_SQL)",
        );
    };
    println!(
        "--- server real content: EMPYREAN_WORLD_SQL={}",
        sql.display()
    );
    let mut argv: Vec<&str> = vec!["test", "-q"];
    argv.extend_from_slice(profile.cargo_args());
    argv.extend_from_slice(&[
        "-p",
        "empyrean-import",
        "--features",
        "real-content",
        "--test",
        "all",
        "import::real_dump::",
    ]);
    println!("$ cargo {}", argv.join(" "));
    let t0 = Instant::now();
    let ok = std::process::Command::new("cargo")
        .args(&argv)
        .current_dir(workspace_root())
        .env("EMPYREAN_WORLD_SQL", &sql)
        .status()
        .is_ok_and(|s| s.success());
    Report::new(
        NAME,
        pass_fail(ok),
        format!("{:.1}s", t0.elapsed().as_secs_f64()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **The real-dump row looks for the release the server pins.** The pin is read out of the
    /// constant's source, so a renamed or reshaped constant fails here instead of silently
    /// turning the row into a permanent NO-ORACLE.
    #[test]
    fn the_real_dump_row_reads_the_pinned_release() {
        let tag = pinned_world_tag().expect("PINNED_TAG in world_release.rs");
        let digits = tag.strip_prefix('v').expect("a v-tag");
        assert_eq!(digits.split('.').count(), 3, "{tag}");
        assert!(digits.split('.').all(|p| p.parse::<u32>().is_ok()), "{tag}");
    }

    /// **The hygiene rules find what they name and pass what they do not.** Planted in a scratch
    /// tree laid out like the workspace, so the calibration does not depend on the real server
    /// code being dirty or clean.
    #[test]
    fn hygiene_finds_each_planted_violation_and_passes_a_clean_file() {
        let ws = std::env::temp_dir().join(format!("xtask-hygiene-{}", std::process::id()));
        let dir = ws.join("empyrean").join("crates").join("probe").join("src");
        std::fs::create_dir_all(&dir).expect("a scratch tree");
        let todo = concat!("to", "do!()");
        let unimpl = concat!("unimple", "mented!()");
        let trailer = concat!("Co-Authored", "-By: someone");
        std::fs::write(
            dir.join("dirty.rs"),
            format!(
                "fn a() {{ {todo} }}\nfn b() {{ {unimpl} }}\n// see references/ACE/Source\n// {trailer}\n// ACE: Foo.cs\n"
            ),
        )
        .expect("write");
        std::fs::write(
            dir.join("clean.rs"),
            "//! Ported from ACE (ACEmulator), under its licence: Source/Foo.cs\n// ACE: Foo.cs\nfn c() {}\n",
        )
        .expect("write");
        let bad = hygiene_findings(&ws);
        std::fs::remove_dir_all(&ws).ok();
        assert_eq!(bad.len(), 5, "{bad:#?}");
        assert!(bad.iter().all(|b| b.contains("dirty.rs")), "{bad:#?}");
        assert!(bad.iter().any(|b| b.contains("header")), "{bad:#?}");
    }

    /// **Without a history the budget is the fixed one, scaled up on a small host.**
    #[test]
    fn without_history_the_budget_scales_with_cores() {
        assert_eq!(Budget::limit(&[], 32).0, 60.0);
        assert_eq!(Budget::limit(&[], 16).0, 60.0);
        assert_eq!(Budget::limit(&[], 4).0, 120.0);
        assert!((Budget::limit(&[11.0, 12.0], 2).0 - 169.7).abs() < 0.05);
        assert_eq!(Budget::limit(&[], 0).0, 240.0);
    }

    /// **With a history a run is judged only against the host's own median.**
    #[test]
    fn with_history_a_run_fails_only_against_the_hosts_own_median() {
        // A fast host: a 12 s median, so the median plus ten seconds is the limit.
        assert_eq!(Budget::limit(&[11.0, 12.0, 13.0], 32).0, 22.0);
        // A slow host: a 42.7 s median, so half as much again, above the fixed 60 s.
        assert!((Budget::limit(&[42.0, 42.7, 43.5], 2).0 - 64.05).abs() < 1e-9);
        // An even count takes the mean of the middle two.
        assert_eq!(Budget::limit(&[10.0, 12.0, 14.0, 16.0], 32).0, 23.0);
    }

    /// **A passing run is recorded; a regression is failed and not recorded; a damaged history
    /// reads as none.**
    #[test]
    fn a_run_is_recorded_and_judged_against_the_history() {
        let dir = std::env::temp_dir().join(format!("xtask-budget-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let budget = Budget {
            path: dir.join("h.json"),
            cores: 16,
        };
        for secs in [10.0, 11.0, 12.0] {
            assert!(budget.judge(secs, true).0);
        }
        let (within, note) = budget.judge(20.9, true);
        assert!(within, "{note}");
        let (within, note) = budget.judge(30.0, true);
        assert!(!within, "{note}");
        assert!(note.contains("host median"), "{note}");
        assert_eq!(budget.history(), vec![10.0, 11.0, 12.0, 20.9]);
        // A run that is not recorded leaves the history alone.
        budget.judge(11.0, false);
        assert_eq!(budget.history().len(), 4);
        // Only the newest runs are kept.
        for _ in 0..HISTORY_KEEP {
            budget.judge(11.0, true);
        }
        assert_eq!(budget.history().len(), HISTORY_KEEP);
        std::fs::write(&budget.path, "{not json").expect("write");
        assert!(budget.history().is_empty());
        std::fs::remove_dir_all(&dir).ok();
        assert!(budget.history().is_empty(), "an absent history is none");
    }

    /// **Each profile keeps its own history, in cargo's target directory.**
    #[test]
    fn each_profile_keeps_its_own_history_in_the_target_directory() {
        let dev = Budget::here(Profile::Dev);
        let rel = Budget::here(Profile::TestRelease);
        assert_ne!(dev.path, rel.path);
        assert!(dev.path.starts_with(crate::util::target_dir()));
        let name = rel.path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        assert!(name.starts_with("test-release-"), "{name}");
    }
}
