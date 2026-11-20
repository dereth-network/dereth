//! `cargo xtask check`: the check to run while working, before each commit.
//!
//! It runs clippy and the tests of the crates this branch changed, not of the whole workspace,
//! and the whole-repository lints that take seconds. What "changed" means:
//!
//! * the files `git diff` reports against the merge base of `HEAD` and the base branch, staged,
//!   unstaged and untracked alike (so on the base branch itself it is the uncommitted work);
//! * the base is `--base <ref>`, else `$XTASK_BASE`, else the first of `research`, `origin/HEAD`,
//!   `origin/main` and `main` that exists -- the research branch in the research repository, the
//!   remote's default branch in a clone of the public one;
//! * each file belongs to the workspace crate whose directory holds it (`cargo metadata`);
//! * `-p <crate>` (repeatable) names the crates instead, and nothing else is selected;
//! * `--game dereth|auberean` keeps one game's crates of either selection ([`crate::game`]).
//!
//! The changed crates are linted and tested. Their reverse dependents (every workspace crate that
//! depends on one of them, directly or not) are linted too -- clippy is a type check, so a caller
//! broken by the change fails here -- but not tested: their tests are `ci tier0`'s. A change to a
//! workspace-level file (`Cargo.toml`, `Cargo.lock`, `clippy.toml`, `.cargo/config.toml`) lints
//! every crate for the same reason.
//!
//! A few tests check one crate's files against another's: [`WATCHERS`] names them, and a change
//! to a crate or file one watches runs that one's tests too, though its own crate is only a
//! reverse dependent (or no crate at all holds the file); a crate named with `-p` runs them the
//! same. Athenaeum's format reference is checked so against the rows the server reads, and
//! Empyrean's and Yalain's release declarations against the code and their `releases.toml`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;

use crate::game::Game;
use crate::util::{print_table, run, workspace_root, Outcome, Profile, Report};

/// The branches tried, in order, when neither `--base` nor `$XTASK_BASE` names one.
const BASE_CANDIDATES: &[&str] = &["research", "origin/HEAD", "origin/main", "main"];

/// Workspace-root files that change how every crate builds or lints.
const WORKSPACE_FILES: &[&str] = &[
    "Cargo.toml",
    "Cargo.lock",
    "clippy.toml",
    "rustfmt.toml",
    ".cargo/config.toml",
];

/// Tests in one crate that check what another crate, or a file outside every crate, holds.
struct Watcher {
    /// The crate whose tests these are.
    package: &'static str,
    /// The test target.
    test: Target,
    /// The target's module holding the tests, run by its path as a filter.
    module: &'static str,
    /// The crates whose change runs them.
    crates: &'static [&'static str],
    /// The files, relative to the workspace, whose change runs them.
    files: &'static [&'static str],
    /// The crate whose game (`--game`) the watcher runs for: what it checks, not where its tests
    /// live.
    game_of: &'static str,
}

/// A watcher's test target.
#[derive(Debug, Clone, Copy)]
enum Target {
    /// A folder of `tests/` with a `main.rs`.
    Tests(&'static str),
    /// A binary's own tests, its modules under `src/`.
    Bin(&'static str),
}

/// Every watcher. Athenaeum's format reference describes the world database's rows, kinds and
/// fields, and its tests check it names every one the server reads. Empyrean's release declaration
/// is read from its version, its databases' migrations, the world pack's format and schema, the
/// configuration's renamed keys and `empyrean/releases.toml`; Yalain's from its version, its
/// database's schema, the world database it pins and `yalain/releases.toml`. Their tests check
/// the release each tree ships as declares cleanly.
const WATCHERS: &[Watcher] = &[
    Watcher {
        package: "athenaeum-foundry",
        test: Target::Tests("cpu"),
        module: "format",
        crates: &["yalain-world-db"],
        files: &["athenaeum/FORMAT.md"],
        game_of: "athenaeum-foundry",
    },
    Watcher {
        package: "xtask",
        test: Target::Bin("xtask"),
        module: "package::tests",
        crates: &[
            "empyrean-store",
            "empyrean-content",
            "empyrean-common",
            "empyrean-server",
        ],
        files: &["empyrean/releases.toml"],
        game_of: "empyrean-server",
    },
    Watcher {
        package: "xtask",
        test: Target::Bin("xtask"),
        module: "package::yalain_tests",
        crates: &["yalain-world-db", "yalain-store", "yalain"],
        files: &["yalain/releases.toml"],
        game_of: "yalain",
    },
];

impl Watcher {
    /// `cargo test`'s arguments after the profile: the target and its module.
    fn test_args(&self) -> Vec<String> {
        let (kind, target) = match self.test {
            Target::Tests(t) => ("--test", t),
            Target::Bin(b) => ("--bin", b),
        };
        [
            "-p",
            self.package,
            kind,
            target,
            "--",
            &format!("{}::", self.module),
        ]
        .map(str::to_owned)
        .to_vec()
    }
}

/// The watchers a change runs: one watching a tested crate or a changed file (relative to the
/// workspace, normalised), for a game `game` covers, whose own crate is not tested whole already.
fn watching<'w>(
    watchers: &'w [Watcher],
    tested: &BTreeSet<String>,
    files: &[String],
    game: Game,
) -> Vec<&'w Watcher> {
    watchers
        .iter()
        .filter(|w| !tested.contains(w.package) && game.covers(w.game_of))
        .filter(|w| {
            w.crates.iter().any(|c| tested.contains(*c))
                || w.files.iter().any(|f| files.contains(&norm(f)))
        })
        .collect()
}

fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// A path as a comparable string: forward slashes, and case-folded where the filesystem folds.
fn norm(p: &str) -> String {
    let p = p.replace('\\', "/");
    let p = p.trim_end_matches('/').to_owned();
    if cfg!(windows) {
        p.to_lowercase()
    } else {
        p
    }
}

/// The base ref: `--base`, else `$XTASK_BASE`, else the first candidate that exists.
fn base_ref(dir: &Path, explicit: Option<&str>) -> Option<String> {
    let env = std::env::var("XTASK_BASE").ok().filter(|v| !v.is_empty());
    if let Some(b) = explicit.map(str::to_owned).or(env) {
        return Some(b);
    }
    BASE_CANDIDATES
        .iter()
        .find(|c| {
            git(
                dir,
                &[
                    "rev-parse",
                    "--verify",
                    "--quiet",
                    &format!("{c}^{{commit}}"),
                ],
            )
            .is_some()
        })
        .map(|c| (*c).to_owned())
}

/// The workspace crates: name, directory (normalised), each one's workspace dependencies, and
/// the crates with a `retail-dats` or a `retail-corpus` feature.
struct Workspace {
    dirs: Vec<(String, String)>,
    deps: BTreeMap<String, BTreeSet<String>>,
    retail_dats: BTreeSet<String>,
    retail_corpus: BTreeSet<String>,
}

fn workspace(ws: &Path) -> Result<Workspace, String> {
    let out = Command::new("cargo")
        .args(["metadata", "--format-version", "1"])
        .current_dir(ws)
        .output()
        .map_err(|e| format!("cargo metadata: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let meta: serde_json::Value =
        serde_json::from_slice(&out.stdout).map_err(|e| format!("cargo metadata: {e}"))?;
    let members: BTreeSet<&str> = meta["workspace_members"]
        .as_array()
        .ok_or("cargo metadata: no workspace_members")?
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    let mut name_of = BTreeMap::new();
    let mut dirs = Vec::new();
    let mut retail_dats = BTreeSet::new();
    let mut retail_corpus = BTreeSet::new();
    for p in meta["packages"]
        .as_array()
        .ok_or("cargo metadata: no packages")?
    {
        let (Some(id), Some(name), Some(manifest)) = (
            p["id"].as_str(),
            p["name"].as_str(),
            p["manifest_path"].as_str(),
        ) else {
            continue;
        };
        if !members.contains(id) {
            continue;
        }
        name_of.insert(id.to_owned(), name.to_owned());
        if p["features"].get("retail-dats").is_some() {
            retail_dats.insert(name.to_owned());
        }
        if p["features"].get("retail-corpus").is_some() {
            retail_corpus.insert(name.to_owned());
        }
        let dir = Path::new(manifest)
            .parent()
            .map(|d| norm(&d.to_string_lossy()))
            .unwrap_or_default();
        dirs.push((name.to_owned(), dir));
    }
    let mut deps: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for node in meta["resolve"]["nodes"].as_array().into_iter().flatten() {
        let Some(name) = node["id"].as_str().and_then(|id| name_of.get(id)) else {
            continue;
        };
        let set = deps.entry(name.clone()).or_default();
        for d in node["deps"].as_array().into_iter().flatten() {
            if let Some(dep) = d["pkg"].as_str().and_then(|id| name_of.get(id)) {
                set.insert(dep.clone());
            }
        }
    }
    Ok(Workspace {
        dirs,
        deps,
        retail_dats,
        retail_corpus,
    })
}

/// What a set of changed files selects.
#[derive(Debug, Default, PartialEq, Eq)]
struct Selection {
    /// Crates holding a changed file: linted and tested.
    changed: BTreeSet<String>,
    /// Whether a workspace-level file changed: every crate is linted.
    workspace_file: bool,
}

/// The crates the changed files (absolute, normalised) belong to. `dirs` is each crate's
/// normalised directory; the longest one that holds a file owns it, so a nested crate is not
/// mistaken for its parent's.
fn select(files: &[String], ws_dir: &str, dirs: &[(String, String)]) -> Selection {
    let mut sel = Selection::default();
    for f in files {
        let owner = dirs
            .iter()
            .filter(|(_, d)| f.starts_with(&format!("{d}/")))
            .max_by_key(|(_, d)| d.len());
        if let Some((name, _)) = owner {
            sel.changed.insert(name.clone());
        } else if let Some(rel) = f.strip_prefix(&format!("{ws_dir}/")) {
            if WORKSPACE_FILES.iter().any(|w| norm(w) == rel) {
                sel.workspace_file = true;
            }
        }
    }
    sel
}

/// Every crate that depends on one of `roots`, directly or through others, `roots` excluded.
fn reverse_dependents(
    roots: &BTreeSet<String>,
    deps: &BTreeMap<String, BTreeSet<String>>,
) -> BTreeSet<String> {
    let mut found: BTreeSet<String> = roots.clone();
    loop {
        let more: Vec<String> = deps
            .iter()
            .filter(|(k, ds)| !found.contains(*k) && ds.iter().any(|d| found.contains(d)))
            .map(|(k, _)| k.clone())
            .collect();
        if more.is_empty() {
            break;
        }
        found.extend(more);
    }
    found.difference(roots).cloned().collect()
}

/// The changed files, absolute and normalised, or why they could not be listed.
fn changed_files(ws: &Path, base: &str) -> Result<(String, Vec<String>), String> {
    let top = git(ws, &["rev-parse", "--show-toplevel"]).ok_or("not a git checkout")?;
    let top = norm(top.trim());
    let merge_base = git(ws, &["merge-base", "HEAD", base])
        .ok_or_else(|| format!("no merge base between HEAD and {base}"))?;
    let merge_base = merge_base.trim().to_owned();
    let mut rel: Vec<String> = Vec::new();
    for args in [
        &["diff", "--name-only", merge_base.as_str()][..],
        &["ls-files", "--others", "--exclude-standard", "--full-name"],
    ] {
        let out = git(ws, args).ok_or_else(|| format!("git {} failed", args.join(" ")))?;
        rel.extend(out.lines().filter(|l| !l.is_empty()).map(str::to_owned));
    }
    // `ls-files` lists paths relative to the directory it runs in unless `--full-name`; both
    // lists are relative to the top of the checkout.
    let files = rel
        .into_iter()
        .map(|r| norm(&format!("{top}/{r}")))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    Ok((merge_base, files))
}

fn value_of<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}

/// The `-p`/`--package` names on the command line.
fn packages(args: &[String]) -> Vec<String> {
    args.iter()
        .enumerate()
        .filter(|(_, a)| *a == "-p" || *a == "--package")
        .filter_map(|(i, _)| args.get(i + 1).cloned())
        .collect()
}

fn step(name: impl Into<String>, ok: bool, detail: impl Into<String>) -> Report {
    Report::new(name, if ok { Outcome::Pass } else { Outcome::Fail }, detail)
}

/// `-p a -p b ...` for cargo.
fn package_args(names: &BTreeSet<String>) -> Vec<&str> {
    names.iter().flat_map(|n| ["-p", n.as_str()]).collect()
}

/// Clippy's arguments for `linted`: every target, with `retail-dats` on when one of them has the
/// feature (cargo turns it on in each selected crate that has it, and refuses it when none does),
/// so the tests that read the retail dats are linted too; likewise `retail-corpus`, the tier switch
/// of Asheron's Call 2's recorded sessions.
fn clippy_args<'a>(
    linted: &'a BTreeSet<String>,
    retail_dats: &BTreeSet<String>,
    retail_corpus: &BTreeSet<String>,
) -> Vec<&'a str> {
    let mut argv = vec!["clippy", "--all-targets", "--no-deps"];
    match (
        !linted.is_disjoint(retail_dats),
        !linted.is_disjoint(retail_corpus),
    ) {
        (true, true) => argv.extend(["--features", "retail-dats,retail-corpus"]),
        (true, false) => argv.extend(["--features", "retail-dats"]),
        (false, true) => argv.extend(["--features", "retail-corpus"]),
        (false, false) => {}
    }
    argv.extend(package_args(linted));
    argv.extend(["--", "-D", "warnings"]);
    argv
}

/// The crates of `selected` that `game` covers, and the ones it leaves out.
fn keep_game(game: Game, selected: BTreeSet<String>) -> (BTreeSet<String>, BTreeSet<String>) {
    selected.into_iter().partition(|c| game.covers(c))
}

pub fn check(args: &[String]) -> i32 {
    let ws = workspace_root();
    let profile = Profile::from_args(args);
    let game = match Game::from_args(args) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("check: {e}");
            return 2;
        }
    };
    let meta = match workspace(&ws) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("check: {e}");
            return 2;
        }
    };
    let all: BTreeSet<String> = meta.dirs.iter().map(|(n, _)| n.clone()).collect();

    let named = packages(args);
    let mut relative: Vec<String> = Vec::new();
    let (tested, linted) = if named.is_empty() {
        let Some(base) = base_ref(&ws, value_of(args, "--base")) else {
            eprintln!(
                "check: no base branch found (tried {}); name one with --base <ref> or \
                 XTASK_BASE, or name the crates with -p",
                BASE_CANDIDATES.join(", ")
            );
            return 2;
        };
        let (merge_base, files) = match changed_files(&ws, &base) {
            Ok(x) => x,
            Err(e) => {
                eprintln!("check: {e}");
                return 2;
            }
        };
        let short = &merge_base[..merge_base.len().min(10)];
        println!(
            "check: {} file(s) changed since {base} (merge base {short})",
            files.len()
        );
        let ws_dir = norm(&ws.to_string_lossy());
        relative = files
            .iter()
            .filter_map(|f| f.strip_prefix(&format!("{ws_dir}/")))
            .map(str::to_owned)
            .collect();
        let sel = select(&files, &ws_dir, &meta.dirs);
        let mut linted = sel.changed.clone();
        if sel.workspace_file {
            println!("  a workspace-level file changed: clippy covers every crate");
            linted = all.clone();
        } else {
            let rdeps = reverse_dependents(&sel.changed, &meta.deps);
            if !rdeps.is_empty() {
                println!(
                    "  reverse dependents, linted but not tested ({}): {}",
                    rdeps.len(),
                    rdeps.iter().cloned().collect::<Vec<_>>().join(", ")
                );
            }
            linted.extend(rdeps);
        }
        (sel.changed, linted)
    } else {
        let unknown: Vec<&String> = named.iter().filter(|n| !all.contains(*n)).collect();
        if !unknown.is_empty() {
            eprintln!("check: not a workspace crate: {unknown:?}");
            return 2;
        }
        let set: BTreeSet<String> = named.into_iter().collect();
        (set.clone(), set)
    };
    let (tested, left_out) = keep_game(game, tested);
    let (linted, _) = keep_game(game, linted);
    if !left_out.is_empty() {
        println!(
            "  left out by {} {}: {}",
            crate::game::FLAG,
            if game.dereth() { "dereth" } else { "auberean" },
            left_out.into_iter().collect::<Vec<_>>().join(", ")
        );
    }
    if tested.is_empty() {
        println!("  no crate changed: clippy and the tests are not run, the lints are");
    } else {
        println!(
            "  changed, linted and tested ({}): {}",
            tested.len(),
            tested.iter().cloned().collect::<Vec<_>>().join(", ")
        );
    }

    let watchers = watching(WATCHERS, &tested, &relative, game);
    for w in &watchers {
        println!(
            "  also tested, for what it checks: {}'s {}::",
            w.package, w.module
        );
    }

    let mut reports = Vec::new();
    if !linted.is_empty() {
        let argv = clippy_args(&linted, &meta.retail_dats, &meta.retail_corpus);
        reports.push(step(
            "cargo clippy -D warnings",
            run(&ws, "cargo", &argv),
            format!("{} crate(s)", linted.len()),
        ));
    }
    if !tested.is_empty() {
        let mut argv = vec!["test"];
        argv.extend_from_slice(profile.cargo_args());
        argv.extend(package_args(&tested));
        reports.push(step(
            "cargo test",
            run(&ws, "cargo", &argv),
            format!("{} crate(s), no data", tested.len()),
        ));
    }
    for w in &watchers {
        let rest = w.test_args();
        let mut argv = vec!["test"];
        argv.extend_from_slice(profile.cargo_args());
        argv.extend(rest.iter().map(String::as_str));
        reports.push(step(
            format!("cargo test {} {}::", w.package, w.module),
            run(&ws, "cargo", &argv),
            "what it checks changed",
        ));
    }
    reports.extend(crate::cheap_lints());
    print_table("xtask check -- before each commit", &reports)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dirs() -> Vec<(String, String)> {
        [
            ("dereth-client", "/w/dereth/client"),
            ("dereth-ui", "/w/dereth/client/crates/ui"),
            ("dereth-primitives", "/w/core/primitives"),
            ("empyrean-world", "/w/empyrean/crates/world"),
        ]
        .iter()
        .map(|(n, d)| ((*n).to_owned(), (*d).to_owned()))
        .collect()
    }

    fn files(paths: &[&str]) -> Vec<String> {
        paths.iter().map(|p| norm(p)).collect()
    }

    /// **`--game` keeps one game's crates of a selection and names the rest**; with both games
    /// nothing is left out.
    #[test]
    fn the_game_filter_keeps_one_games_crates_and_names_the_rest() {
        let selected: BTreeSet<String> = [
            "dereth-dat",
            "empyrean-world",
            "xtask",
            "auberean-dat",
            "yalain",
            "athenaeum-cli",
        ]
        .map(str::to_owned)
        .into_iter()
        .collect();
        let names = |s: BTreeSet<String>| s.into_iter().collect::<Vec<_>>();
        let (kept, out) = keep_game(Game::Auberean, selected.clone());
        assert_eq!(names(kept), ["athenaeum-cli", "auberean-dat", "yalain"]);
        assert_eq!(names(out), ["dereth-dat", "empyrean-world", "xtask"]);
        let (kept, out) = keep_game(Game::Dereth, selected.clone());
        assert_eq!(names(kept), ["dereth-dat", "empyrean-world", "xtask"]);
        assert_eq!(names(out), ["athenaeum-cli", "auberean-dat", "yalain"]);
        // Negative control: the default keeps the selection whole.
        let (kept, out) = keep_game(Game::All, selected.clone());
        assert_eq!(kept, selected);
        assert!(out.is_empty());
    }

    /// **A file belongs to the innermost crate that holds it**, a file outside every crate
    /// selects none, and only the workspace-level files widen the lint to every crate.
    #[test]
    fn a_changed_file_selects_the_innermost_crate_that_holds_it() {
        let sel = select(
            &files(&[
                "/w/dereth/client/crates/ui/src/lib.rs",
                "/w/empyrean/crates/world/tests/cpu/main.rs",
            ]),
            "/w",
            &dirs(),
        );
        assert_eq!(
            sel.changed.into_iter().collect::<Vec<_>>(),
            ["dereth-ui", "empyrean-world"],
            "the client's nested crate, not the client"
        );
        assert!(!sel.workspace_file);

        let none = select(
            &files(&[
                "/w/README.md",
                "/elsewhere/notes.md",
                "/w/core/primitivesx/a.rs",
            ]),
            "/w",
            &dirs(),
        );
        assert_eq!(none, Selection::default(), "no crate holds these");

        let lock = select(&files(&["/w/Cargo.lock"]), "/w", &dirs());
        assert!(lock.workspace_file && lock.changed.is_empty());
        let nested_manifest = select(&files(&["/w/dereth/client/Cargo.toml"]), "/w", &dirs());
        assert!(
            !nested_manifest.workspace_file,
            "a crate's manifest is that crate's"
        );
        assert!(nested_manifest.changed.contains("dereth-client"));
    }

    /// Reverse dependents are found through intermediate crates and never include the roots.
    #[test]
    fn reverse_dependents_are_transitive_and_exclude_the_changed_crates() {
        let mut deps: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut edge = |k: &str, d: &[&str]| {
            deps.insert(k.to_owned(), d.iter().map(|s| (*s).to_owned()).collect());
        };
        edge("primitives", &[]);
        edge("dat", &["primitives"]);
        edge("client", &["dat"]);
        edge("ui", &["primitives"]);
        edge("unrelated", &[]);
        let roots: BTreeSet<String> = ["dat".to_owned()].into();
        assert_eq!(
            reverse_dependents(&roots, &deps)
                .into_iter()
                .collect::<Vec<_>>(),
            ["client"]
        );
        let roots: BTreeSet<String> = ["primitives".to_owned()].into();
        assert_eq!(
            reverse_dependents(&roots, &deps)
                .into_iter()
                .collect::<Vec<_>>(),
            ["client", "dat", "ui"]
        );
        assert!(reverse_dependents(&BTreeSet::new(), &deps).is_empty());
    }

    /// **Clippy lints the retail-dats tests of the crates that have them**: the feature is asked
    /// for when a linted crate has it, and not when none does, which cargo would refuse.
    #[test]
    fn clippy_turns_retail_dats_on_when_a_linted_crate_has_it() {
        let set = |names: &[&str]| -> BTreeSet<String> {
            names.iter().map(|s| (*s).to_owned()).collect()
        };
        let with = set(&["dereth-client", "dereth-scene"]);
        let linted = set(&["dereth-client", "dereth-primitives"]);
        assert_eq!(
            clippy_args(&linted, &with, &set(&[])),
            [
                "clippy",
                "--all-targets",
                "--no-deps",
                "--features",
                "retail-dats",
                "-p",
                "dereth-client",
                "-p",
                "dereth-primitives",
                "--",
                "-D",
                "warnings"
            ]
        );
        let without = set(&["dereth-primitives"]);
        assert!(!clippy_args(&without, &with, &set(&[])).contains(&"--features"));
        let corpus = set(&["dereth-client"]);
        assert!(clippy_args(&linted, &with, &corpus).contains(&"retail-dats,retail-corpus"));
        assert!(clippy_args(&linted, &set(&[]), &corpus).contains(&"retail-corpus"));
    }

    /// **A change to the world database's crate or to Athenaeum's format reference tests the
    /// format reference**, and a change elsewhere does not; nor does one whose crate is tested
    /// whole already, or one `--game` leaves the watcher out of.
    #[test]
    fn a_change_to_the_rows_athenaeum_describes_runs_its_format_tests() {
        let set = |names: &[&str]| -> BTreeSet<String> {
            names.iter().map(|s| (*s).to_owned()).collect()
        };
        let format_of = |ws: Vec<&'static Watcher>| -> Vec<&'static Watcher> {
            ws.into_iter()
                .filter(|w| w.package == "athenaeum-foundry")
                .collect()
        };
        let packages =
            |ws: Vec<&'static Watcher>| format_of(ws).iter().map(|w| w.package).collect::<Vec<_>>();
        let rows = set(&["yalain-world-db"]);
        assert_eq!(
            packages(watching(WATCHERS, &rows, &[], Game::All)),
            ["athenaeum-foundry"]
        );
        let reference = files(&["athenaeum/FORMAT.md"]);
        assert_eq!(
            packages(watching(WATCHERS, &set(&[]), &reference, Game::Auberean)),
            ["athenaeum-foundry"],
            "the reference is in no crate"
        );
        let format = format_of(watching(WATCHERS, &rows, &[], Game::All))[0];
        assert_eq!(
            format.test_args(),
            ["-p", "athenaeum-foundry", "--test", "cpu", "--", "format::"]
        );

        // Negative controls.
        let elsewhere = set(&["yalain-world", "dereth-dat"]);
        let notes = files(&["athenaeum/README.md", "yalain/CHANGES.md"]);
        assert!(packages(watching(WATCHERS, &elsewhere, &notes, Game::All)).is_empty());
        let whole = set(&["yalain-world-db", "athenaeum-foundry"]);
        assert!(
            packages(watching(WATCHERS, &whole, &[], Game::All)).is_empty(),
            "its crate's tests run whole"
        );
        assert!(watching(WATCHERS, &rows, &reference, Game::Dereth).is_empty());
    }

    /// **A change to what Yalain's release declaration reads (its version, database schema,
    /// pinned world database or `yalain/releases.toml`) runs xtask's Yalain packaging tests**,
    /// which a world database bump otherwise reaches through no crate it changes; a change
    /// elsewhere does not, nor does a check of the first game alone.
    #[test]
    fn a_change_to_what_yalains_declaration_reads_runs_its_tests() {
        let set = |names: &[&str]| -> BTreeSet<String> {
            names.iter().map(|s| (*s).to_owned()).collect()
        };
        let declaration = |tested: &BTreeSet<String>, changed: &[String], game: Game| {
            watching(WATCHERS, tested, changed, game)
                .into_iter()
                .filter(|w| w.module == "package::yalain_tests")
                .collect::<Vec<_>>()
        };
        // A world database bump: the pin's crate and the release entry change.
        let bump = files(&[
            "yalain/releases.toml",
            "yalain/crates/world-db/src/release.rs",
        ]);
        let ran = declaration(&set(&["yalain-world-db"]), &bump, Game::All);
        assert_eq!(ran.len(), 1);
        assert_eq!(
            ran[0].test_args(),
            [
                "-p",
                "xtask",
                "--bin",
                "xtask",
                "--",
                "package::yalain_tests::"
            ]
        );
        for tested in [&["yalain"][..], &["yalain-store"][..]] {
            assert_eq!(declaration(&set(tested), &[], Game::Auberean).len(), 1);
        }
        assert_eq!(
            declaration(&set(&[]), &files(&["yalain/releases.toml"]), Game::Auberean).len(),
            1,
            "the entries are in no crate"
        );

        // Negative controls.
        assert!(declaration(&set(&["yalain-world", "dereth-dat"]), &[], Game::All).is_empty());
        assert!(declaration(&set(&["yalain-world-db"]), &bump, Game::Dereth).is_empty());
        assert!(
            declaration(&set(&["yalain-world-db", "xtask"]), &bump, Game::All).is_empty(),
            "xtask's tests run whole"
        );
    }

    /// **A change to what Empyrean's release declaration reads (its version, migrations, world
    /// pack, renamed keys or `empyrean/releases.toml`) runs xtask's package tests**, and a change
    /// elsewhere, or a check of the second game alone, does not.
    #[test]
    fn a_change_to_what_empyreans_declaration_reads_runs_its_tests() {
        let set = |names: &[&str]| -> BTreeSet<String> {
            names.iter().map(|s| (*s).to_owned()).collect()
        };
        let declaration = |tested: &BTreeSet<String>, changed: &[String], game: Game| {
            watching(WATCHERS, tested, changed, game)
                .into_iter()
                .filter(|w| w.module == "package::tests")
                .collect::<Vec<_>>()
        };
        let pack = files(&[
            "empyrean/releases.toml",
            "empyrean/crates/content/src/pack/format.rs",
        ]);
        let ran = declaration(&set(&["empyrean-content"]), &pack, Game::All);
        assert_eq!(ran.len(), 1);
        assert_eq!(
            ran[0].test_args(),
            ["-p", "xtask", "--bin", "xtask", "--", "package::tests::"]
        );
        for tested in ["empyrean-store", "empyrean-common", "empyrean-server"] {
            assert_eq!(
                declaration(&set(&[tested]), &[], Game::Dereth).len(),
                1,
                "{tested}"
            );
        }
        assert_eq!(
            declaration(&set(&[]), &files(&["empyrean/releases.toml"]), Game::Dereth).len(),
            1,
            "the entries are in no crate"
        );

        // Negative controls.
        let notes = files(&["empyrean/CHANGES.md", "empyrean/DIVERGENCES.md"]);
        assert!(declaration(&set(&["empyrean-world", "dereth-dat"]), &notes, Game::All).is_empty());
        assert!(declaration(&set(&["empyrean-content"]), &pack, Game::Auberean).is_empty());
        assert!(
            declaration(&set(&["empyrean-content", "xtask"]), &pack, Game::All).is_empty(),
            "xtask's tests run whole"
        );
    }

    /// **Every watcher names crates, a test module and files that exist**, so a rename can't
    /// leave it running no tests.
    #[test]
    fn every_watcher_names_tests_and_files_that_exist() {
        let root = workspace_root();
        let workspace = std::fs::read_to_string(root.join("Cargo.toml")).unwrap();
        // A workspace crate's directory, from its line in the workspace's dependencies.
        let dir_of = |name: &str| -> Option<String> {
            let line = workspace
                .lines()
                .find(|l| l.starts_with(&format!("{name} = ")))?;
            Some(
                line.split("path = \"")
                    .nth(1)?
                    .split('"')
                    .next()?
                    .to_owned(),
            )
        };
        for w in WATCHERS {
            let dir =
                dir_of(w.package).unwrap_or_else(|| panic!("{} is a workspace crate", w.package));
            let (folder, path) = match w.test {
                Target::Tests(t) => (root.join(&dir).join("tests").join(t), w.module.to_owned()),
                Target::Bin(_) => (root.join(&dir).join("src"), w.module.replace("::", "/")),
            };
            let file = folder.join(format!("{path}.rs"));
            assert!(
                file.is_file(),
                "{}'s {:?}: {}",
                w.package,
                w.test,
                file.display()
            );
            // The module is declared in its parent: the target's `main.rs`, or the folder's
            // `mod.rs` above it.
            let (parent, name) = match path.rsplit_once('/') {
                Some((p, n)) => (folder.join(p).join("mod.rs"), n),
                None => (folder.join("main.rs"), path.as_str()),
            };
            let declares = std::fs::read_to_string(&parent)
                .unwrap_or_else(|e| panic!("{}: {e}", parent.display()));
            assert!(
                declares.contains(&format!("mod {name};")),
                "{} declares mod {name}",
                parent.display()
            );
            for f in w.files {
                assert!(root.join(f).is_file(), "{f}");
            }
            for c in w.crates {
                // A dependency line names a library; a binary (Yalain's server) has only its
                // members entry and its own manifest.
                let member = || {
                    workspace
                        .lines()
                        .filter_map(|l| l.trim().strip_prefix('"')?.strip_suffix("\","))
                        .any(|dir| {
                            std::fs::read_to_string(root.join(dir).join("Cargo.toml")).is_ok_and(
                                |m| m.lines().any(|l| l.trim() == format!("name = \"{c}\"")),
                            )
                        })
                };
                assert!(dir_of(c).is_some() || member(), "{c} is a workspace crate");
            }
        }
    }

    /// `-p` is read wherever it appears, in either spelling.
    #[test]
    fn named_crates_and_the_base_are_read_from_the_command_line() {
        let argv: Vec<String> = ["-p", "a", "--base", "main", "--package", "b", "--debug"]
            .iter()
            .map(|s| (*s).to_owned())
            .collect();
        assert_eq!(packages(&argv), ["a", "b"]);
        assert_eq!(value_of(&argv, "--base"), Some("main"));
        assert_eq!(value_of(&argv, "--missing"), None);
    }
}
