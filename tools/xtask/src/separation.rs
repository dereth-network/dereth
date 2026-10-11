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
//!    licence-leak rule), nor in the network's MIT crates.
//! 6. The network (`network/`) is a third part, self-contained so that it can become a repository
//!    of its own: a crate under it depends on no workspace crate outside it. A server crate may
//!    name its protocol ([`NETWORK_SHARED`]) by path into `network/`, and its hub and conformance
//!    suite ([`NETWORK_DEV`]) as dev-dependencies only; no client crate, and none of the network's
//!    own MIT crates outside a dev-dependency, may name its hub, which carries the server's
//!    licence ([`NETWORK_COPYLEFT`]); the rest of it is MIT ([`UNICODE_DATA`] adds the Unicode
//!    licence of the data it embeds), and the copy of Windows' best-fit table it carries is the
//!    client's ([`BEST_FIT_TABLES`]).
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

/// The network's folder: the third part of the workspace (rule 6).
const NETWORK_DIR: &str = "network";

/// The network crates a server crate may name in any table.
pub const NETWORK_SHARED: &[&str] = &["dereth-network-protocol"];

/// The network crates a server crate may name as dev-dependencies only (to run the network's own
/// routing and checks in its tests).
pub const NETWORK_DEV: &[&str] = &["dereth-network-hub", "dereth-network-conformance"];

/// The network crates under the server's licence; every other network crate is MIT.
pub const NETWORK_COPYLEFT: &[&str] = &["dereth-network-hub"];

/// The crates that embed Unicode data files (Windows' best-fit table for code page 1252) and so
/// carry the Unicode licence beside MIT.
pub const UNICODE_DATA: &[&str] = &["dereth-network-protocol"];

/// The licence of a crate in [`UNICODE_DATA`].
const UNICODE_DATA_LICENSE: &str = "MIT AND Unicode-3.0";

/// The network's MIT crates, which the licence-leak scan covers too.
const NETWORK_MIT_DIRS: &[&str] = &["network/protocol", "network/conformance"];

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
    let network_dir = ws.join(NETWORK_DIR);
    let packages = meta["packages"].as_array().map_or(&[][..], Vec::as_slice);
    let is_network = |p: &Value| under(p["manifest_path"].as_str().unwrap_or(""), &network_dir);
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
    // 1. The one-way rule (and rule 6's: no client crate names the network's copyleft hub).
    for p in &others {
        for d in deps(p) {
            if NETWORK_COPYLEFT.contains(&name(d)) && !is_network(p) {
                f.failures.push(format!(
                    "client crate {} depends on the network's hub {} ({})",
                    name(p),
                    name(d),
                    kind(d)
                ));
            } else if NETWORK_COPYLEFT.contains(&name(d))
                && !NETWORK_COPYLEFT.contains(&name(p))
                && kind(d) != "dev"
            {
                f.failures.push(format!(
                    "the network's MIT crate {} depends on its hub {} ({})",
                    name(p),
                    name(d),
                    kind(d)
                ));
            }
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
            } else if !under(path, &core_dir)
                && !(under(path, &network_dir) && is_network_crate(name(d)))
                && !DEV_EXCEPTIONS.contains(&(name(p), name(d)))
            {
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
                if !n.starts_with(CLIENT_PREFIX)
                    || SHARED.contains(&n)
                    || NETWORK_SHARED.contains(&n)
                {
                    continue;
                }
                if kind(d) == "dev"
                    && (DEV_EXCEPTIONS.contains(&(name(p), n)) || NETWORK_DEV.contains(&n))
                {
                    allowed += 1;
                    continue;
                }
                f.failures.push(format!(
                    "server crate {} depends on client-only crate {n} ({}); a server crate may \
                     name only the shared crates: {} (and the network's {}; {} as dev-dependencies)",
                    name(p),
                    kind(d),
                    SHARED.join(", "),
                    NETWORK_SHARED.join(", "),
                    NETWORK_DEV.join(", ")
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
        let want = if name(p).starts_with(SERVER_PREFIX) || NETWORK_COPYLEFT.contains(&name(p)) {
            SERVER_LICENSE.as_str()
        } else if UNICODE_DATA.contains(&name(p)) {
            UNICODE_DATA_LICENSE
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
            "licences: {SERVER_PREFIX}* and {} {}, {} {UNICODE_DATA_LICENSE}, everything else {CLIENT_LICENSE}",
            NETWORK_COPYLEFT.join(", "),
            SERVER_LICENSE.as_str(),
            UNICODE_DATA.join(", ")
        ));
    }
    // 6. The network is self-contained.
    let before = f.failures.len();
    for p in packages.iter().filter(|p| is_network(p)) {
        for d in deps(p) {
            let path = d["path"].as_str().unwrap_or("");
            if !path.is_empty() && !under(path, &network_dir) {
                f.failures.push(format!(
                    "network crate {} depends on {} outside network/ ({path})",
                    name(p),
                    name(d)
                ));
            }
        }
    }
    if f.failures.len() == before {
        f.notes
            .push("network/ depends on nothing outside it".to_owned());
    }
}

/// The client's copy of Windows' best-fit table for code page 1252, which a Windows parity test
/// holds to Windows, and the network's own copy (`network/` depends on nothing outside it, so it
/// carries one): their tables must be the same, whatever their headers say.
const BEST_FIT_TABLES: (&str, &str) = (
    "core/primitives/src/text/cp1252_best_fit.rs",
    "network/protocol/src/text/cp1252_best_fit.rs",
);

/// The table of a best-fit file: everything from its first `const`, its header left out.
fn best_fit_table(text: &str) -> Option<&str> {
    text.find("pub(super) const ").map(|at| &text[at..])
}

/// The network's best-fit table is the client's (when both are in the tree).
fn best_fit_tables_agree(ws: &Path, f: &mut Findings) {
    let (client, network) = BEST_FIT_TABLES;
    let (Ok(a), Ok(b)) = (
        std::fs::read_to_string(ws.join(client)),
        std::fs::read_to_string(ws.join(network)),
    ) else {
        return;
    };
    match (best_fit_table(&a), best_fit_table(&b)) {
        (Some(x), Some(y)) if x.replace('\r', "") == y.replace('\r', "") => f.notes.push(format!(
            "{network} holds the same best-fit table as {client}"
        )),
        _ => f.failures.push(format!(
            "{network} does not hold the same best-fit table as {client}, which the Windows parity \
             test holds to Windows"
        )),
    }
}

/// Whether `n` names one of the network's crates.
fn is_network_crate(n: &str) -> bool {
    NETWORK_SHARED.contains(&n) || NETWORK_DEV.contains(&n)
}

fn rel(root: &Path, p: &Path) -> String {
    p.strip_prefix(root)
        .unwrap_or(p)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Rule 4's file half (no second `[workspace]` under `empyrean/`, its licence text) and rule 5.
pub fn check_files(ws: &Path, client_only: bool, f: &mut Findings) {
    best_fit_tables_agree(ws, f);
    let server_dir = ws.join("empyrean");
    if !client_only {
        if server_dir.join("Cargo.toml").exists() {
            f.failures.push(
                "empyrean/Cargo.toml exists: the server is part of the workspace's Cargo.toml and \
                 must not declare its own"
                    .to_owned(),
            );
        }
        let files = crate::util::source_files(&server_dir, &["fixtures"]);
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
    for top in MIT_DIRS.iter().chain(NETWORK_MIT_DIRS) {
        let files = crate::util::source_files(&ws.join(top), &[]);
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
            "no {} provenance markers under core/, dereth/, tools/ or the network's MIT crates",
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

    /// The network's rules: a server crate names the protocol, and the hub and the suite in its
    /// tests only; no client crate, and no MIT network crate outside its tests, names the hub;
    /// the hub carries the server's licence; nothing under `network/` reaches outside it.
    #[test]
    fn the_network_is_a_third_part_with_its_own_rules() {
        let protocol = pkg(
            "dereth-network-protocol",
            "network/protocol",
            UNICODE_DATA_LICENSE,
            json!([]),
        );
        let hub = pkg(
            "dereth-network-hub",
            "network/hub",
            &server_licence(),
            json!([{ "name": "dereth-network-protocol", "path": "/w/network/protocol" }]),
        );
        let world = |deps: Value| {
            pkg(
                "empyrean-world",
                "empyrean/crates/world",
                &server_licence(),
                deps,
            )
        };
        let clean = run(
            vec![
                protocol.clone(),
                hub.clone(),
                world(json!([
                    { "name": "dereth-network-protocol", "path": "/w/network/protocol" },
                    { "name": "dereth-network-hub", "path": "/w/network/hub", "kind": "dev" },
                ])),
            ],
            false,
        );
        assert!(clean.failures.is_empty(), "{:?}", clean.failures);

        let hub_in_production = run(
            vec![
                protocol.clone(),
                hub.clone(),
                world(json!([{ "name": "dereth-network-hub", "path": "/w/network/hub" }])),
            ],
            false,
        );
        assert_eq!(
            hub_in_production.failures.len(),
            1,
            "{:?}",
            hub_in_production.failures
        );

        let client_names_hub = run(
            vec![
                protocol.clone(),
                hub.clone(),
                pkg(
                    "dereth-client",
                    "dereth/client",
                    "MIT",
                    json!([{ "name": "dereth-network-hub", "path": "/w/network/hub", "kind": "dev" }]),
                ),
            ],
            false,
        );
        assert!(client_names_hub
            .failures
            .iter()
            .any(|x| x.contains("the network's hub")));

        let hub_mit = run(
            vec![
                protocol.clone(),
                pkg("dereth-network-hub", "network/hub", "MIT", json!([])),
            ],
            false,
        );
        assert!(hub_mit
            .failures
            .iter()
            .any(|x| x.contains("dereth-network-hub has license")));

        let reaches_out = run(
            vec![
                pkg(
                    "dereth-network-protocol",
                    "network/protocol",
                    "MIT",
                    json!([{ "name": "dereth-primitives", "path": "/w/core/primitives" }]),
                ),
                pkg("dereth-primitives", "core/primitives", "MIT", json!([])),
            ],
            false,
        );
        assert!(reaches_out
            .failures
            .iter()
            .any(|x| x.contains("outside network/")));

        let suite_names_hub = run(
            vec![
                protocol.clone(),
                hub.clone(),
                pkg(
                    "dereth-network-conformance",
                    "network/conformance",
                    "MIT",
                    json!([{ "name": "dereth-network-hub", "path": "/w/network/hub" }]),
                ),
            ],
            false,
        );
        assert!(suite_names_hub
            .failures
            .iter()
            .any(|x| x.contains("the network's MIT crate dereth-network-conformance")));

        let protocol_mit_only = run(
            vec![pkg(
                "dereth-network-protocol",
                "network/protocol",
                "MIT",
                json!([]),
            )],
            false,
        );
        assert!(protocol_mit_only
            .failures
            .iter()
            .any(|x| x.contains("dereth-network-protocol has license")));
    }

    /// The network's best-fit table must be the client's, whatever the headers say.
    #[test]
    fn the_networks_best_fit_table_must_be_the_clients() {
        let dir = std::env::temp_dir().join(format!("xtask-best-fit-{}", std::process::id()));
        let (client, network) = BEST_FIT_TABLES;
        for p in [client, network] {
            std::fs::create_dir_all(dir.join(p).parent().expect("a folder")).expect("folders");
        }
        let table = "pub(super) const BEST_FIT: [(u16, u8); 1] = [(0x0100, b'A')];\n";
        std::fs::write(dir.join(client), format!("// one header\n{table}")).expect("written");
        std::fs::write(dir.join(network), format!("// another header\n{table}")).expect("written");
        let mut f = Findings::default();
        best_fit_tables_agree(&dir, &mut f);
        assert!(f.failures.is_empty(), "{:?}", f.failures);
        std::fs::write(
            dir.join(network),
            "// h\npub(super) const BEST_FIT: [(u16, u8); 1] = [(0x0100, b'B')];\n",
        )
        .expect("written");
        let mut f = Findings::default();
        best_fit_tables_agree(&dir, &mut f);
        assert_eq!(f.failures.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
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
