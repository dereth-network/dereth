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
//! * `-p <crate>` (repeatable) names the crates instead, and nothing else is selected.
//!
//! The changed crates are linted and tested. Their reverse dependents (every workspace crate that
//! depends on one of them, directly or not) are linted too -- clippy is a type check, so a caller
//! broken by the change fails here -- but not tested: their tests are `ci tier0`'s. A change to a
//! workspace-level file (`Cargo.toml`, `Cargo.lock`, `clippy.toml`, `.cargo/config.toml`) lints
//! every crate for the same reason.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;

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

/// The workspace crates: name, directory (normalised), and each one's workspace dependencies.
struct Workspace {
    dirs: Vec<(String, String)>,
    deps: BTreeMap<String, BTreeSet<String>>,
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
    Ok(Workspace { dirs, deps })
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

pub fn check(args: &[String]) -> i32 {
    let ws = workspace_root();
    let profile = Profile::from_args(args);
    let meta = match workspace(&ws) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("check: {e}");
            return 2;
        }
    };
    let all: BTreeSet<String> = meta.dirs.iter().map(|(n, _)| n.clone()).collect();

    let named = packages(args);
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
        let sel = select(&files, &norm(&ws.to_string_lossy()), &meta.dirs);
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
    if tested.is_empty() {
        println!("  no crate changed: clippy and the tests are not run, the lints are");
    } else {
        println!(
            "  changed, linted and tested ({}): {}",
            tested.len(),
            tested.iter().cloned().collect::<Vec<_>>().join(", ")
        );
    }

    let mut reports = Vec::new();
    if !linted.is_empty() {
        let mut argv = vec!["clippy", "--all-targets", "--no-deps"];
        argv.extend(package_args(&linted));
        argv.extend(["--", "-D", "warnings"]);
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
