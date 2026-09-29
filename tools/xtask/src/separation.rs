//! `cargo xtask separation [--client-only] [ROOT]`: the client/server separation rules, checked
//! mechanically.
//!
//! The shared crates, the client and the server are one Cargo workspace, laid out as `core/` (the
//! shared crates and the `dereth-client-sdk` facade), `dereth/` (the client), `tools/` (research
//! and build tools) and `empyrean/` (the server). What keeps the server apart is this check, run by
//! `cargo xtask ci tier0`.
//!
//! 1. No `dereth-*` (or other non-server) crate depends on an `empyrean-*` crate or on a path into
//!    `empyrean/`, in any dependency table, dev and build included (the one-way rule), and nothing
//!    under `empyrean/` is anything but an `empyrean-*` crate.
//! 2. A server crate depends on `dereth-*` crates only from [`SHARED`] (the `core/` crates). The
//!    client-only crates (`dereth/`, `tools/`) are allowed only as the dev-dependencies listed in
//!    [`DEV_EXCEPTIONS`] (test-only cross-checks).
//! 3. Server crates consume the shared crates by path into `core/`, never copied or fetched.
//! 4. Licences: every `empyrean-*` crate carries the server's licence (`empyrean/LICENSE`), every
//!    other member `MIT`, and `empyrean/` holds no second `[workspace]` of its own.
//! 5. No copyleft provenance marker appears anywhere under `core/`, `dereth/` or `tools/` (the
//!    licence-leak rule).
//!
//! `--client-only` enforces the client boundary without the server-side checks: 1, 3 and 5, plus
//! the licence of every crate, for a checkout that has no server crates at all.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;

use crate::util::workspace_root;

const SERVER_PREFIX: &str = "empyrean-";
const CLIENT_PREFIX: &str = "dereth-";

/// The shared crates (`core/`): what a server crate may name. Every other `dereth-*` crate is
/// client-only.
pub const SHARED: &[&str] = &[
    "dereth-animation",
    "dereth-assets",
    "dereth-audio",
    "dereth-client-contract",
    "dereth-client-model",
    "dereth-client-net",
    "dereth-client-runtime",
    "dereth-client-sdk",
    "dereth-dat",
    "dereth-landscape",
    "dereth-physics",
    "dereth-primitives",
    "dereth-protocol",
    "dereth-rules",
    "dereth-transport",
    "dereth-world-data",
];

/// `(server crate, client-only crate)` pairs allowed as dev-dependencies only. Empty: the one pair
/// it held became a shared crate when its client-side half moved into `core/`.
pub const DEV_EXCEPTIONS: &[(&str, &str)] = &[];

/// The server's licence, spelled in pieces so this file does not itself carry the marker rule 5
/// looks for.
static SERVER_LICENSE: LazyLock<String> = LazyLock::new(|| format!("{}-3.0-only", copyleft()));
const CLIENT_LICENSE: &str = "MIT";
/// The MIT folders the licence-leak scan covers.
const MIT_DIRS: &[&str] = &["core", "dereth", "tools"];

fn copyleft() -> String {
    ["A", "GPL"].concat()
}

/// The provenance marker ported files carry. Its presence anywhere under an MIT folder means
/// copyleft code has leaked into an MIT crate.
static MARKER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        "(?i)Ported from ({}|GDLE)|{}",
        ["ACE", "mulator"].concat(),
        copyleft()
    ))
    .expect("MARKER")
});

/// What a check found: the passes worth saying and the failures.
#[derive(Debug, Default)]
pub struct Findings {
    pub notes: Vec<String>,
    pub failures: Vec<String>,
}

fn norm(p: &str) -> String {
    let s = p.replace('\\', "/");
    let s = s.trim_end_matches('/');
    if cfg!(windows) {
        s.to_lowercase()
    } else {
        s.to_owned()
    }
}

fn under(path: &str, dir: &Path) -> bool {
    let (p, d) = (norm(path), norm(&dir.to_string_lossy()));
    p == d || p.starts_with(&format!("{d}/"))
}

fn kind(d: &Value) -> &str {
    d["kind"].as_str().unwrap_or("normal")
}

fn name(v: &Value) -> &str {
    v["name"].as_str().unwrap_or("")
}

fn deps(p: &Value) -> &[Value] {
    p["dependencies"].as_array().map_or(&[], Vec::as_slice)
}

/// Rules 1 to 4's dependency and licence halves, over `cargo metadata` output for the workspace at
/// `ws`.
pub fn check_metadata(meta: &Value, ws: &Path, client_only: bool, f: &mut Findings) {
    let server_dir = ws.join("empyrean");
    let core_dir = ws.join("core");
    let packages = meta["packages"].as_array().map_or(&[][..], Vec::as_slice);
    let (server, others): (Vec<&Value>, Vec<&Value>) = packages
        .iter()
        .partition(|p| name(p).starts_with(SERVER_PREFIX));
    f.notes.push(format!(
        "workspace: {} packages, {} of them {SERVER_PREFIX}*",
        packages.len(),
        server.len()
    ));
    if client_only {
        f.notes
            .push("client-only mode: server-side checks not requested".to_owned());
    }
    // 1. The one-way rule.
    for p in &others {
        for d in deps(p) {
            if name(d).starts_with(SERVER_PREFIX) {
                f.failures.push(format!(
                    "client crate {} depends on server crate {} ({})",
                    name(p),
                    name(d),
                    kind(d)
                ));
            }
            let path = d["path"].as_str().unwrap_or("");
            if !path.is_empty() && under(path, &server_dir) {
                f.failures.push(format!(
                    "client crate {} has a path dependency into empyrean/: {path}",
                    name(p)
                ));
            }
        }
        if under(p["manifest_path"].as_str().unwrap_or(""), &server_dir) {
            f.failures.push(format!(
                "{} lives under empyrean/ but is not an {SERVER_PREFIX}* crate",
                name(p)
            ));
        }
    }
    // 3. The shared crates by path into core/.
    for p in &server {
        for d in deps(p) {
            if !name(d).starts_with(CLIENT_PREFIX) {
                continue;
            }
            let path = d["path"].as_str().unwrap_or("");
            if path.is_empty() {
                f.failures.push(format!(
                    "{} depends on {} without a path (must be a path dependency into core/)",
                    name(p),
                    name(d)
                ));
            } else if !under(path, &core_dir) && !DEV_EXCEPTIONS.contains(&(name(p), name(d))) {
                f.failures.push(format!(
                    "{} resolves {} to an unexpected path: {path}",
                    name(p),
                    name(d)
                ));
            }
        }
    }
    // 2. What a server crate may name.
    if !client_only {
        let mut allowed = 0;
        for p in &server {
            if !under(p["manifest_path"].as_str().unwrap_or(""), &server_dir) {
                f.failures
                    .push(format!("server crate {} lives outside empyrean/", name(p)));
            }
            for d in deps(p) {
                let n = name(d);
                if !n.starts_with(CLIENT_PREFIX) || SHARED.contains(&n) {
                    continue;
                }
                if kind(d) == "dev" && DEV_EXCEPTIONS.contains(&(name(p), n)) {
                    allowed += 1;
                    continue;
                }
                f.failures.push(format!(
                    "server crate {} depends on client-only crate {n} ({}); a server crate may \
                     name only the shared crates: {}",
                    name(p),
                    kind(d),
                    SHARED.join(", ")
                ));
            }
        }
        f.notes.push(format!(
            "server crates name only shared crates ({allowed} allowed dev exception(s))"
        ));
    }
    // 4. Licences.
    let before = f.failures.len();
    for p in packages {
        let want = if name(p).starts_with(SERVER_PREFIX) {
            SERVER_LICENSE.as_str()
        } else {
            CLIENT_LICENSE
        };
        let have = p["license"].as_str();
        if have != Some(want) {
            f.failures.push(format!(
                "{} has license {have:?}, expected {want:?}",
                name(p)
            ));
        }
    }
    if f.failures.len() == before {
        f.notes.push(format!(
            "licences: {SERVER_PREFIX}* {}, everything else {CLIENT_LICENSE}",
            SERVER_LICENSE.as_str()
        ));
    }
}

fn walk(dir: &Path, skip: &[&str], out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for e in entries {
        let p = e.path();
        if p.is_dir() {
            if !skip.iter().any(|s| e.file_name() == **s) {
                walk(&p, skip, out);
            }
        } else {
            out.push(p);
        }
    }
}

fn rel(root: &Path, p: &Path) -> String {
    p.strip_prefix(root)
        .unwrap_or(p)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Rule 4's file half (no second `[workspace]` under `empyrean/`, its licence text) and rule 5.
pub fn check_files(ws: &Path, client_only: bool, f: &mut Findings) {
    let server_dir = ws.join("empyrean");
    if !client_only {
        if server_dir.join("Cargo.toml").exists() {
            f.failures.push(
                "empyrean/Cargo.toml exists: the server is part of the workspace's Cargo.toml and \
                 must not declare its own"
                    .to_owned(),
            );
        }
        let mut files = Vec::new();
        walk(&server_dir, &["target", "fixtures"], &mut files);
        for p in files
            .iter()
            .filter(|p| p.file_name().is_some_and(|n| n == "Cargo.toml"))
        {
            let body = std::fs::read_to_string(p).unwrap_or_default();
            if body.lines().any(|l| l.starts_with("[workspace]")) {
                f.failures
                    .push(format!("{} declares its own [workspace]", rel(ws, p)));
            }
        }
        let licence = std::fs::read(server_dir.join("LICENSE")).unwrap_or_default();
        let head = String::from_utf8_lossy(&licence[..licence.len().min(4096)]).into_owned();
        if !head.contains("GNU AFFERO GENERAL PUBLIC LICENSE") {
            f.failures.push(format!(
                "empyrean/LICENSE is missing or is not the {}-3.0 text",
                copyleft()
            ));
        }
    }
    let mut hits = Vec::new();
    for top in MIT_DIRS {
        let mut files = Vec::new();
        walk(&ws.join(top), &["target"], &mut files);
        for p in files {
            let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
            if ext != "rs" && ext != "toml" {
                continue;
            }
            let body = std::fs::read(&p).unwrap_or_default();
            if MARKER.is_match(&String::from_utf8_lossy(&body)) {
                hits.push(rel(ws, &p));
            }
        }
    }
    if hits.is_empty() {
        f.notes.push(format!(
            "no {} provenance markers under core/, dereth/ or tools/",
            copyleft()
        ));
    } else {
        f.failures.push(format!(
            "{}/ACE provenance marker found in MIT crates (licence leak): {}",
            copyleft(),
            hits.join(", ")
        ));
    }
}

/// `cargo metadata` for the workspace at `ws`, offline and locked.
fn metadata(ws: &Path) -> Result<Value, String> {
    let out = Command::new("cargo")
        .args([
            "metadata",
            "--no-deps",
            "--locked",
            "--offline",
            "--format-version",
            "1",
        ])
        .current_dir(ws)
        .output()
        .map_err(|e| format!("cargo metadata: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!(
            "cargo metadata failed in {}:\n{}",
            ws.display(),
            err.trim().chars().take(400).collect::<String>()
        ));
    }
    serde_json::from_slice(&out.stdout).map_err(|e| format!("cargo metadata: {e}"))
}

/// Run every rule over the workspace at `ws`.
pub fn check(ws: &Path, client_only: bool) -> Findings {
    let mut f = Findings::default();
    match metadata(ws) {
        Ok(meta) => check_metadata(&meta, ws, client_only, &mut f),
        Err(e) => f.failures.push(e),
    }
    check_files(ws, client_only, &mut f);
    f
}

/// Print the findings; the exit code.
pub fn report(f: &Findings) -> i32 {
    for n in &f.notes {
        println!("ok    {n}");
    }
    for x in &f.failures {
        println!("FAIL  {x}");
    }
    println!();
    if f.failures.is_empty() {
        println!("client/server separation intact");
        0
    } else {
        println!("{} separation rule(s) violated", f.failures.len());
        1
    }
}

/// `cargo xtask separation`.
pub fn separation(args: &[String]) -> i32 {
    let client_only = args.iter().any(|a| a == "--client-only");
    let roots: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    if roots.len() > 1
        || args
            .iter()
            .any(|a| a.starts_with("--") && a != "--client-only")
    {
        eprintln!("usage: cargo xtask separation [--client-only] [workspace root]");
        return 2;
    }
    let ws = roots
        .first()
        .map_or_else(workspace_root, |r| PathBuf::from(r.as_str()));
    let ws = std::fs::canonicalize(&ws).unwrap_or(ws);
    let ws = PathBuf::from(
        ws.to_string_lossy()
            .strip_prefix(r"\\?\")
            .map_or_else(|| ws.to_string_lossy().into_owned(), str::to_owned),
    );
    report(&check(&ws, client_only))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ws() -> PathBuf {
        PathBuf::from("/w")
    }

    fn pkg(name: &str, dir: &str, license: &str, deps: Value) -> Value {
        json!({
            "name": name,
            "manifest_path": format!("/w/{dir}/Cargo.toml"),
            "license": license,
            "dependencies": deps,
        })
    }

    fn run(packages: Vec<Value>, client_only: bool) -> Findings {
        let mut f = Findings::default();
        check_metadata(&json!({ "packages": packages }), &ws(), client_only, &mut f);
        f
    }

    fn server_licence() -> String {
        SERVER_LICENSE.clone()
    }

    /// A clean pair -- a client crate and a server crate naming a shared crate by path into core/
    /// -- passes.
    #[test]
    fn a_server_crate_naming_a_shared_crate_by_path_passes() {
        let f = run(
            vec![
                pkg("dereth-primitives", "core/primitives", "MIT", json!([])),
                pkg(
                    "empyrean-world",
                    "empyrean/crates/world",
                    &server_licence(),
                    json!([{ "name": "dereth-primitives", "path": "/w/core/primitives" }]),
                ),
            ],
            false,
        );
        assert!(f.failures.is_empty(), "{:?}", f.failures);
    }

    /// A client crate may not depend on a server crate or reach into empyrean/ by path, in
    /// client-only mode too.
    #[test]
    fn a_client_crate_may_not_depend_on_the_server() {
        for client_only in [false, true] {
            let f = run(
                vec![pkg(
                    "dereth-example",
                    "dereth/example",
                    "MIT",
                    json!([{ "name": "empyrean-example", "path": "/w/empyrean/example", "kind": "dev" }]),
                )],
                client_only,
            );
            assert!(
                f.failures
                    .iter()
                    .any(|x| x.contains("depends on server crate")),
                "{:?}",
                f.failures
            );
            assert!(f
                .failures
                .iter()
                .any(|x| x.contains("path dependency into empyrean/")));
        }
    }

    /// A server crate may name only the shared crates; the client-only rule is a server-side
    /// check that client-only mode does not ask.
    #[test]
    fn a_server_crate_may_name_only_shared_crates() {
        let bad = vec![pkg(
            "empyrean-world",
            "empyrean/crates/world",
            &server_licence(),
            json!([{ "name": "dereth-ui", "path": "/w/core/ui" }]),
        )];
        assert!(run(bad.clone(), false)
            .failures
            .iter()
            .any(|x| x.contains("client-only crate dereth-ui")));
        assert!(run(bad, true).failures.is_empty());
    }

    /// A server crate takes a shared crate by a path into core/, never fetched or from elsewhere.
    #[test]
    fn a_shared_crate_is_taken_by_path_into_core() {
        let fetched = run(
            vec![pkg(
                "empyrean-world",
                "empyrean/crates/world",
                &server_licence(),
                json!([{ "name": "dereth-primitives" }]),
            )],
            false,
        );
        assert!(fetched
            .failures
            .iter()
            .any(|x| x.contains("without a path")));
        let elsewhere = run(
            vec![pkg(
                "empyrean-world",
                "empyrean/crates/world",
                &server_licence(),
                json!([{ "name": "dereth-primitives", "path": "/w/vendor/primitives" }]),
            )],
            false,
        );
        assert!(elsewhere
            .failures
            .iter()
            .any(|x| x.contains("unexpected path")));
    }

    /// Each crate carries its half's licence, and a crate under empyrean/ is a server crate.
    #[test]
    fn each_crate_carries_its_halfs_licence_and_lives_in_its_half() {
        let f = run(
            vec![
                pkg(
                    "dereth-example",
                    "dereth/example",
                    &server_licence(),
                    json!([]),
                ),
                pkg("empyrean-world", "empyrean/crates/world", "MIT", json!([])),
                pkg("dereth-stray", "empyrean/stray", "MIT", json!([])),
            ],
            false,
        );
        assert_eq!(
            f.failures
                .iter()
                .filter(|x| x.contains("has license"))
                .count(),
            2,
            "{:?}",
            f.failures
        );
        assert!(f
            .failures
            .iter()
            .any(|x| x.contains("lives under empyrean/")));
    }

    /// A provenance marker anywhere under an MIT folder is a licence leak, client-only included;
    /// a tree without one is clean.
    #[test]
    fn a_provenance_marker_under_an_mit_folder_is_a_licence_leak() {
        let root = std::env::temp_dir().join(format!("xtask-separation-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        std::fs::create_dir_all(root.join("dereth/example/src")).expect("dirs");
        let lib = root.join("dereth/example/src/lib.rs");
        std::fs::write(&lib, "pub fn example() {}\n").expect("write");
        let mut clean = Findings::default();
        check_files(&root, true, &mut clean);
        assert!(clean.failures.is_empty(), "{:?}", clean.failures);
        std::fs::write(&lib, format!("// {}\npub fn example() {{}}\n", copyleft())).expect("write");
        let mut leaked = Findings::default();
        check_files(&root, true, &mut leaked);
        assert!(
            leaked
                .failures
                .iter()
                .any(|x| x.contains("provenance marker found")),
            "{:?}",
            leaked.failures
        );
        // The server-side file rules: a second [workspace] and a missing licence both fail.
        std::fs::create_dir_all(root.join("empyrean/example")).expect("dirs");
        std::fs::write(
            root.join("empyrean/example/Cargo.toml"),
            "[package]\n[workspace]\n",
        )
        .expect("write");
        let mut server = Findings::default();
        check_files(&root, false, &mut server);
        assert!(server
            .failures
            .iter()
            .any(|x| x.contains("declares its own [workspace]")));
        assert!(server
            .failures
            .iter()
            .any(|x| x.contains("empyrean/LICENSE")));
        std::fs::remove_dir_all(&root).ok();
    }
}
