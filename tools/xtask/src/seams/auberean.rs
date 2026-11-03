//! The seams of Asheron's Call 2's crates, checked rather than described: its core
//! (`core/auberean-*`), its client (`auberean/`), its server (`yalain/`) and its world builder
//! (`athenaeum/`). Rows of `cargo xtask seams`, beside the first game's.
//!
//! 1. **A portable core.** A core crate's production tree holds no crate but the core's own and
//!    the shared crates it is built on ([`SHARED`]), pruned there because their own seams hold them
//!    to the same rules; `auberean-primitives` depends on nothing; `auberean-client-contract`
//!    only on `auberean-primitives`.
//! 2. **No I/O in the core.** A core crate's code names no file, network, thread, process,
//!    environment or clock facility ([`NO_IO`]) outside the files a feature gates for native hosts
//!    and tests ([`IO_FILES`], each declared behind its feature).
//! 3. **The core builds for a browser**: every core crate, for `wasm32-unknown-unknown`, with its
//!    default features. A host without that target cannot ask, and says so.
//! 4. **One place finds the data.** No source of the four folders outside [`LOCATORS`] spells a
//!    data file's name or a test-data variable ([`DATA_NAMES`]).
//! 5. **The action seam.** The contract, the runtime, the SDK and the headless client name no key,
//!    mouse button or window message ([`DEVICE_NAMES`]): a front end turns its device into actions.
//! 6. **Four parts.** The client's crates use the core, their own and [`CLIENT_MAY_USE`]; the
//!    server's the core, their own and [`SERVER_MAY_USE`] (the network's protocol and its link, and
//!    the game servers' WebSocket endpoint);
//!    the world builder's the core, its own and
//!    [`ATHENAEUM_MAY_USE`] (the world's drawing and the world database, never the server's game,
//!    network, store or program: it runs Yalain as a program); a core crate's
//!    tests may use the client's test kit and the server's network crate (the server double), the
//!    server's tests the client's test kit. Nothing of the first game's uses any of them, but the
//!    launcher's knowledge the friends stream's codec ([`OUTSIDE_MAY_USE`]). No crate
//!    but a test kit names a test kit outside its tests, and no library names a log subscriber
//!    ([`super::LIBRARY_NO_SUBSCRIBER`]): only an executable installs one, and the libraries in
//!    [`OUTERMOST`], loaded by a program that is not ours.
//! 7. **The application's halves reach no platform.** The client's shell, scene and interface
//!    ([`HALVES`])
//!    depend directly on no window system, operating system binding, sound device, pad or desktop
//!    host ([`HALVES_FORBIDDEN`]; what the shared crates under them reach is held by their own
//!    seams), and their code names none ([`HALVES_NAMES_NOT`]): the platform arrives through the
//!    shell's host.
//!
//! The scans are textual, with comments blanked: the crates' docs may talk about the other side of
//! a seam, their code may not reach across it.

use std::path::{Path, PathBuf};
use std::process::Command;

use super::{blank_comments, workspace_packages, WorkspacePackage};
use crate::util::{NotRun, Outcome, Report};

/// The folder prefix of the core crates.
const CORE_PREFIX: &str = "core/auberean-";

/// The shared crates the core is built on: the public Dereth workspace's own core crates, held to
/// the same rules by their own seams. Adding one is a decision made here, in review.
pub const SHARED: &[&str] = &[
    "dereth-primitives",
    "dereth-dat",
    "dereth-landscape",
    "dereth-terrain",
    "dereth-physics",
    "dereth-transport",
    "dereth-client-net",
    "dereth-protocol",
    "dereth-pack",
];

/// What core code must not name.
pub const NO_IO: &[&str] = &[
    "std::fs",
    "std::net",
    "std::thread",
    "std::process",
    "std::env",
    "std::time",
    "Instant::now",
    "SystemTime",
    "File::open",
    "env!(\"CARGO",
];

/// The core files allowed I/O: `(file, the crate root declaring it, the declaration)`.
pub const IO_FILES: &[(&str, &str, &str)] = &[
    (
        "core/auberean-dat/src/fs.rs",
        "core/auberean-dat/src/lib.rs",
        "#[cfg(feature = \"fs\")]\npub mod fs;",
    ),
    (
        "core/auberean-dat/src/locate.rs",
        "core/auberean-dat/src/lib.rs",
        "#[cfg(feature = \"fs\")]\npub mod locate;",
    ),
    (
        "core/auberean-dat/src/testing.rs",
        "core/auberean-dat/src/lib.rs",
        "#[cfg(feature = \"test-support\")]\npub mod testing;",
    ),
];

/// The files that may name the data files and the test-data variables.
pub const LOCATORS: &[&str] = &[
    "core/auberean-dat/src/locate.rs",
    "core/auberean-dat/src/testing.rs",
    "auberean/testkit/src/data.rs",
];

/// Data file names and test-data variables.
pub const DATA_NAMES: &[&str] = &[
    "portal.dat",
    "highres.dat",
    "cell_1.dat",
    "cell_2.dat",
    "local_English",
    "\"AUBEREAN_TEST_DAT_DIR\"",
    "\"AUBEREAN_TEST_CAPTURE_INDEX\"",
];

/// The crates on the action seam.
pub const ACTION_SEAM: &[&str] = &[
    "auberean-client-contract",
    "auberean-client-runtime",
    "auberean-client-sdk",
    "auberean-headless",
];

/// Device-input names the action seam's crates must not use.
pub const DEVICE_NAMES: &[&str] = &[
    "KeyCode",
    "VirtualKey",
    "MouseButton",
    "WM_KEYDOWN",
    "winit",
    "KeyMap",
    "InputMap",
    "dereth_input",
];

/// The first game's client crates the second's client may use: the picture codecs, the sound
/// device, and the operating system's error box.
pub const CLIENT_MAY_USE: &[&str] = &["dereth-render-cpu", "dereth-audio-out", "dereth-console"];

/// The crates outside the four parts the server may use: the Dereth Network's protocol and a
/// world's end of its link, which name no game (the network is self-contained), so that the server
/// speaks the network with the code every world shares rather than a copy of its own; and the
/// WebSocket endpoint and the updater both game servers run, which name no game either.
pub const SERVER_MAY_USE: &[&str] = &[
    "dereth-network-protocol",
    "dereth-network-link",
    "dereth-server-update",
    "dereth-server-websocket",
];

/// The client's and the server's crates the world builder may use: the world's frame and its
/// device, which it draws the world with, and the world database and the server's tables, which
/// it reads, checks and compiles worlds with. Nothing of the server's game, network, store or
/// program: the builder starts Yalain as a program of its own.
pub const ATHENAEUM_MAY_USE: &[&str] = &[
    "auberean-draw",
    "auberean-gpu",
    "yalain-world-db",
    "yalain-content",
];

/// The crates of the second game's a crate outside the four parts may use, by that crate: the
/// launcher's knowledge, which serves both games, asks a world whether it serves the friends stream
/// and writes the friends ticket with the stream's own codec (a core crate with no I/O and no
/// dependencies), rather than a copy of its framing.
pub const OUTSIDE_MAY_USE: &[(&str, &str)] = &[("dereth-launch", "auberean-friends")];

/// The four parts' folders.
const PARTS: [&str; 4] = ["core/auberean-", "auberean/", "yalain/", "athenaeum/"];

/// The client's application halves: the shell, the scene and the game's interface.
pub const HALVES: &[&str] = &["auberean-shell", "auberean-scene", "auberean-interface"];

/// What an application half must not depend on directly: the window system, the operating
/// system's bindings, the sound device, the pads, and the desktop host.
pub const HALVES_FORBIDDEN: &[&str] = &[
    "winit",
    "windows",
    "windows-sys",
    "cpal",
    "gilrs",
    "auberean-client",
];

/// The paths an application half's code must never name.
pub const HALVES_NAMES_NOT: &[&str] = &[
    "winit::",
    "windows::",
    "windows_sys::",
    "cpal::",
    "gilrs::",
    "auberean_client::",
];

/// The executables: each installs the one log subscriber of its process.
pub const EXECUTABLES: &[&str] = &[
    "auberean-client",
    "yalain",
    "auberean-viewer",
    "auberean-headless",
    "athenaeum-cli",
    "athenaeum-editor",
];

/// The libraries that are the outermost Rust of their process, loaded by a program that is not
/// ours, so that each installs its own log subscriber as an executable does: the friends service's
/// library for the retail client, when it is built, and the client's module in a browser.
pub const OUTERMOST: &[&str] = &["auberean-friends-dll", "auberean-web"];

/// The test kits: test support, which only tests may name.
const TEST_KITS: &[&str] = &["auberean-testkit", "yalain-testkit"];

fn part_of(dir: &str) -> Option<&'static str> {
    PARTS
        .iter()
        .copied()
        .find(|p| dir.starts_with(p) || format!("{dir}/") == *p)
}

/// Rule 6 over the workspace's packages.
#[must_use]
pub fn part_violations(packages: &[WorkspacePackage]) -> Vec<String> {
    let dir_of = |name: &str| {
        packages
            .iter()
            .find(|p| p.name == name)
            .map(|p| p.dir.as_str())
    };
    let mut out = Vec::new();
    for p in packages {
        let ours = part_of(&p.dir);
        let executable =
            EXECUTABLES.contains(&p.name.as_str()) || OUTERMOST.contains(&p.name.as_str());
        for d in &p.deps {
            let theirs = dir_of(&d.name).and_then(part_of);
            let test = if d.dev { " (for its tests)" } else { "" };
            let in_core = dir_of(&d.name).is_some_and(|x| x.starts_with("core/"));
            let allowed = match ours {
                None => {
                    theirs.is_none()
                        || OUTSIDE_MAY_USE.contains(&(p.name.as_str(), d.name.as_str()))
                }
                Some("core/auberean-") => {
                    in_core
                        || dir_of(&d.name).is_none()
                        || (d.dev
                            && (d.name == "auberean-testkit"
                                || d.name == "yalain-net"
                                || theirs == Some("auberean/")))
                }
                Some("auberean/") => {
                    in_core
                        || theirs == Some("auberean/")
                        || CLIENT_MAY_USE.contains(&d.name.as_str())
                        || (p.name == "auberean-testkit" && d.name == "yalain-net")
                        || dir_of(&d.name).is_none()
                }
                Some("yalain/") => {
                    in_core
                        || theirs == Some("yalain/")
                        || SERVER_MAY_USE.contains(&d.name.as_str())
                        || (d.dev && d.name == "auberean-testkit")
                        || dir_of(&d.name).is_none()
                }
                Some("athenaeum/") => {
                    in_core
                        || theirs == Some("athenaeum/")
                        || ATHENAEUM_MAY_USE.contains(&d.name.as_str())
                        || dir_of(&d.name).is_none()
                }
                Some(_) => in_core || theirs == ours || dir_of(&d.name).is_none(),
            };
            if !allowed {
                out.push(match ours {
                    None => format!(
                        "{} depends on `{}`{test}: nothing outside Asheron's Call 2's folders may",
                        p.name, d.name
                    ),
                    Some(part) => format!(
                        "{} depends on `{}`{test}: {part} uses the core's crates and its own",
                        p.name, d.name
                    ),
                });
            }
            if ours.is_some()
                && !d.dev
                && TEST_KITS.contains(&d.name.as_str())
                && !TEST_KITS.contains(&p.name.as_str())
            {
                out.push(format!(
                    "{} depends on the test kit `{}` outside its tests",
                    p.name, d.name
                ));
            }
            if ours.is_some()
                && !executable
                && super::LIBRARY_NO_SUBSCRIBER.contains(&d.name.as_str())
            {
                out.push(format!(
                    "{} names the log subscriber `{}`: only an executable installs one",
                    p.name, d.name
                ));
            }
        }
    }
    super::dedup(out)
}

/// What a crate's production tree reaches, the shared crates' own subtrees left out: their seams
/// are their own.
fn cargo_tree_pruned(ws: &Path, krate: &str) -> Result<Vec<String>, String> {
    let tree = super::cargo_tree(ws, krate, "normal")?;
    Ok(prune_shared(krate, &tree))
}

/// The names in `tree` (a `--prefix depth` listing), below none of [`SHARED`], `krate` left out.
fn prune_shared(krate: &str, tree: &[super::TreeEntry]) -> Vec<String> {
    let mut names = Vec::new();
    let mut inside: Option<usize> = None;
    for e in tree {
        if inside.is_some_and(|d| e.depth > d) {
            continue;
        }
        inside = None;
        if SHARED.contains(&e.name.as_str()) {
            inside = Some(e.depth);
        }
        if e.name != krate {
            names.push(e.name.clone());
        }
    }
    names.sort();
    names.dedup();
    names
}

/// Rule 1: what each core crate reaches, given its pruned tree (`trees`: crate, names reached).
#[must_use]
pub fn core_dependency_violations(core: &[String], trees: &[(String, Vec<String>)]) -> Vec<String> {
    let mut out = Vec::new();
    for (krate, reached) in trees {
        for d in reached {
            if !core.contains(d) && !SHARED.contains(&d.as_str()) {
                out.push(format!(
                    "{krate} reaches `{d}`: the core reaches only the core and {}",
                    SHARED.join(", ")
                ));
            }
            if krate == "auberean-primitives" {
                out.push(format!(
                    "auberean-primitives depends on `{d}`: it depends on nothing"
                ));
            }
            if krate == "auberean-client-contract" && d != "auberean-primitives" {
                out.push(format!(
                    "auberean-client-contract reaches `{d}`: only auberean-primitives"
                ));
            }
        }
    }
    out
}

fn sources(ws: &Path, dir: &str, sub: &[&str]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for s in sub {
        for p in crate::util::rust_sources(&ws.join(dir).join(s)) {
            if let Ok(text) = std::fs::read_to_string(&p) {
                let rel = p
                    .strip_prefix(ws)
                    .unwrap_or(&p)
                    .to_string_lossy()
                    .replace('\\', "/");
                out.push((rel, text));
            }
        }
    }
    out
}

/// Rule 2 over the core's production sources (`files`: path relative to the workspace, text) and
/// the crate roots that declare the I/O files (`roots`: path, text).
#[must_use]
pub fn no_io_violations(files: &[(String, String)], roots: &[(String, String)]) -> Vec<String> {
    let mut out = Vec::new();
    for (path, text) in files {
        if IO_FILES.iter().any(|(f, _, _)| f == path) {
            continue;
        }
        let code = blank_comments(text);
        for bad in NO_IO {
            if code.contains(bad) {
                out.push(format!("{path} names {bad}"));
            }
        }
    }
    for (file, root, decl) in IO_FILES {
        let declared = roots
            .iter()
            .find(|(p, _)| p == root)
            .is_some_and(|(_, t)| t.replace("\r\n", "\n").contains(decl));
        if files.iter().any(|(p, _)| p == file) && !declared {
            out.push(format!(
                "{file} is not declared behind its feature in {root}"
            ));
        }
    }
    out
}

/// Rule 4 over every source of the four folders.
#[must_use]
pub fn data_name_violations(files: &[(String, String)]) -> Vec<String> {
    let mut out = Vec::new();
    for (path, text) in files {
        if LOCATORS.contains(&path.as_str()) {
            continue;
        }
        let code = blank_comments(text);
        for name in DATA_NAMES {
            if code.contains(name) {
                out.push(format!(
                    "{path} spells {name}; find the data through the locators"
                ));
            }
        }
    }
    out
}

/// Rule 5 over the action seam's production sources.
#[must_use]
pub fn device_violations(files: &[(String, String)]) -> Vec<String> {
    let mut out = Vec::new();
    for (path, text) in files {
        let code = blank_comments(text);
        for name in DEVICE_NAMES {
            if code.contains(name) {
                out.push(format!("{path} names {name}"));
            }
        }
    }
    out
}

/// Rule 7: what each application half depends on directly (`trees`: crate, names) and what its code
/// names (`files`: path, text).
#[must_use]
pub fn halves_violations(
    trees: &[(String, Vec<String>)],
    files: &[(String, String)],
) -> Vec<String> {
    let mut out = Vec::new();
    for (krate, reached) in trees {
        for d in reached {
            if HALVES_FORBIDDEN.contains(&d.as_str()) {
                out.push(format!(
                    "{krate} reaches `{d}`: the platform arrives through the host"
                ));
            }
        }
    }
    for (path, text) in files {
        let code = blank_comments(text);
        for name in HALVES_NAMES_NOT {
            if code.contains(name) {
                out.push(format!("{path} names {name}"));
            }
        }
    }
    out
}

/// Whether this host's toolchain has the browser target's standard library.
fn has_browser_target() -> bool {
    Command::new("rustc")
        .args(["--print", "sysroot"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| {
            PathBuf::from(String::from_utf8_lossy(&o.stdout).trim())
                .join("lib/rustlib/wasm32-unknown-unknown")
        })
        .is_some_and(|p| p.is_dir())
}

/// Rule 3: the core, built for the browser.
fn browser_build(ws: &Path, core: &[String]) -> Report {
    const NAME: &str = "seam: Auberean core in a browser";
    if !has_browser_target() {
        return Report::new(
            NAME,
            Outcome::NotRun(NotRun::Unhostable),
            "the toolchain has no wasm32-unknown-unknown target (rustup target add it)",
        );
    }
    let mut cmd = Command::new("cargo");
    cmd.current_dir(ws).args([
        "build",
        "--profile",
        "test-release",
        "--target",
        "wasm32-unknown-unknown",
    ]);
    for p in core {
        cmd.args(["-p", p]);
    }
    match cmd.output() {
        Ok(o) if o.status.success() => Report::new(
            NAME,
            Outcome::Pass,
            format!(
                "{} core crates build for wasm32-unknown-unknown",
                core.len()
            ),
        ),
        Ok(o) => {
            println!("seam: {}", String::from_utf8_lossy(&o.stderr));
            Report::new(
                NAME,
                Outcome::Fail,
                "the core does not build for the browser; see above",
            )
        }
        Err(e) => Report::new(NAME, Outcome::Fail, format!("cargo build: {e}")),
    }
}

/// Every Asheron's Call 2 seam, as rows for the seams table.
pub fn reports(ws: &Path) -> Vec<Report> {
    let row = |name: &str, result: Result<Vec<String>, String>, ok: String| match result {
        Ok(v) if v.is_empty() => Report::new(name, Outcome::Pass, ok),
        Ok(v) => {
            for line in &v {
                println!("seam: {line}");
            }
            Report::new(
                name,
                Outcome::Fail,
                format!("{} violation(s); see above", v.len()),
            )
        }
        Err(e) => {
            println!("seam: {e}");
            Report::new(name, Outcome::Fail, "could not read the workspace")
        }
    };
    let packages = workspace_packages(ws);
    let core: Vec<(String, String)> = packages
        .as_ref()
        .map(|ps| {
            ps.iter()
                .filter(|p| p.dir.starts_with(CORE_PREFIX))
                .map(|p| (p.name.clone(), p.dir.clone()))
                .collect()
        })
        .unwrap_or_default();
    let core_names: Vec<String> = core.iter().map(|(n, _)| n.clone()).collect();
    let deps = if core.len() < 10 {
        Err(format!(
            "found {} crate(s) under {CORE_PREFIX}*; expected at least 10",
            core.len()
        ))
    } else {
        core.iter()
            .map(|(n, _)| cargo_tree_pruned(ws, n).map(|t| (n.clone(), t)))
            .collect::<Result<Vec<_>, _>>()
            .map(|trees| core_dependency_violations(&core_names, &trees))
    };
    let core_src: Vec<(String, String)> = core
        .iter()
        .flat_map(|(_, d)| sources(ws, d, &["src"]))
        .collect();
    let roots: Vec<(String, String)> = core_src
        .iter()
        .filter(|(p, _)| p.ends_with("/src/lib.rs"))
        .cloned()
        .collect();
    let no_io = Ok(no_io_violations(&core_src, &roots));
    let all_src: Vec<(String, String)> = packages
        .as_ref()
        .map(|ps| {
            ps.iter()
                .filter(|p| part_of(&p.dir).is_some())
                .flat_map(|p| sources(ws, &p.dir, &["src", "tests", "benches", "examples"]))
                .collect()
        })
        .unwrap_or_default();
    let data = if all_src.len() < 100 {
        Err(format!(
            "found {} Rust file(s) in Asheron's Call 2's folders; expected hundreds",
            all_src.len()
        ))
    } else {
        Ok(data_name_violations(&all_src))
    };
    let seam_src: Vec<(String, String)> = packages
        .as_ref()
        .map(|ps| {
            ps.iter()
                .filter(|p| ACTION_SEAM.contains(&p.name.as_str()))
                .flat_map(|p| sources(ws, &p.dir, &["src"]))
                .collect()
        })
        .unwrap_or_default();
    let device = if seam_src.is_empty() {
        Err(format!("no sources found for {}", ACTION_SEAM.join(", ")))
    } else {
        Ok(device_violations(&seam_src))
    };
    let parts = packages
        .as_ref()
        .map(|p| part_violations(p))
        .map_err(Clone::clone);
    let halves_src: Vec<(String, String)> = packages
        .as_ref()
        .map(|ps| {
            ps.iter()
                .filter(|p| HALVES.contains(&p.name.as_str()))
                .flat_map(|p| sources(ws, &p.dir, &["src"]))
                .collect()
        })
        .unwrap_or_default();
    let halves = if halves_src.is_empty() {
        Err(format!("no sources found for {}", HALVES.join(", ")))
    } else {
        HALVES
            .iter()
            .map(|k| {
                super::cargo_tree(ws, k, "normal").map(|t| {
                    let direct = t.into_iter().filter(|e| e.depth == 1).map(|e| e.name);
                    ((*k).to_owned(), direct.collect())
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .map(|trees| halves_violations(&trees, &halves_src))
    };
    vec![
        row(
            "seam: Auberean core deps",
            deps,
            format!(
                "core/auberean-* reach the core and {} only; primitives nothing, the contract the primitives",
                SHARED.join(", ")
            ),
        ),
        row(
            "seam: Auberean core code",
            no_io,
            "no file, network, thread, process, environment or clock outside the gated files".to_owned(),
        ),
        browser_build(ws, &core_names),
        row(
            "seam: Auberean data locators",
            data,
            format!("only {} name the data", LOCATORS.join(", ")),
        ),
        row(
            "seam: Auberean action seam",
            device,
            format!("{} name no device input", ACTION_SEAM.join(", ")),
        ),
        row(
            "seam: Auberean application halves",
            halves,
            format!(
                "{} depend on no window system, device or desktop host and name none; the host                  brings them",
                HALVES.join(", ")
            ),
        ),
        row(
            "seam: Asheron's Call 2 parts",
            parts,
            "the client, the server and the world builder use the core, their own and their \
             reviewed lists; nothing else uses them; test kits only in tests; executables alone \
             install a log subscriber"
                .to_owned(),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::super::ManifestDep;
    use super::*;

    #[test]
    fn an_application_half_reaching_the_window_system_is_caught() {
        let trees = vec![
            (
                "auberean-shell".to_owned(),
                vec!["auberean-scene".to_owned()],
            ),
            ("auberean-scene".to_owned(), vec!["winit".to_owned()]),
        ];
        let files = vec![
            (
                "a.rs".to_owned(),
                "// winit::event is the host's\nfn f() {}".to_owned(),
            ),
            (
                "b.rs".to_owned(),
                "use winit::event::WindowEvent;".to_owned(),
            ),
        ];
        let v = halves_violations(&trees, &files);
        assert_eq!(v.len(), 2, "{v:?}");
        assert!(v[0].contains("auberean-scene reaches `winit`"));
        assert!(v[1].starts_with("b.rs"));
        assert!(halves_violations(&trees[..1], &files[..1]).is_empty());
    }

    fn pkg(name: &str, dir: &str, deps: &[(&str, bool)], _bin_only: bool) -> WorkspacePackage {
        WorkspacePackage {
            name: name.to_owned(),
            dir: dir.to_owned(),
            deps: deps
                .iter()
                .map(|(n, dev)| ManifestDep {
                    name: (*n).to_owned(),
                    dev: *dev,
                })
                .collect(),
        }
    }

    fn base() -> Vec<WorkspacePackage> {
        vec![
            pkg(
                "auberean-dat",
                "core/auberean-dat",
                &[("dereth-dat", false), ("auberean-testkit", true)],
                false,
            ),
            pkg("dereth-dat", "core/dat", &[], false),
            pkg(
                "dereth-render-cpu",
                "dereth/client/crates/render-cpu",
                &[],
                false,
            ),
            pkg(
                "auberean-draw",
                "auberean/client/crates/draw",
                &[("auberean-dat", false), ("dereth-render-cpu", false)],
                false,
            ),
            pkg(
                "auberean-testkit",
                "auberean/testkit",
                &[("auberean-dat", false), ("yalain-net", false)],
                false,
            ),
            pkg(
                "yalain-net",
                "yalain/crates/net",
                &[("dereth-dat", false)],
                false,
            ),
            pkg(
                "yalain",
                "yalain/server",
                &[
                    ("yalain-net", false),
                    ("tracing-subscriber", false),
                    ("auberean-testkit", true),
                    ("dereth-network-protocol", false),
                    ("dereth-network-link", false),
                ],
                true,
            ),
            pkg(
                "athenaeum-cli",
                "athenaeum/cli",
                &[("auberean-dat", false)],
                true,
            ),
            pkg("empyrean-world", "empyrean/crates/world", &[], false),
            pkg("dereth-client", "dereth/client", &[], false),
            pkg(
                "athenaeum-editor",
                "athenaeum/editor",
                &[
                    ("auberean-draw", false),
                    ("yalain-world-db", false),
                    ("athenaeum-foundry", false),
                ],
                true,
            ),
            pkg(
                "yalain-world-db",
                "yalain/crates/world-db",
                &[("auberean-dat", false)],
                false,
            ),
            pkg(
                "athenaeum-foundry",
                "athenaeum/crates/foundry",
                &[("yalain-world-db", false)],
                false,
            ),
            pkg("dereth-network-protocol", "network/protocol", &[], false),
            pkg(
                "dereth-network-link",
                "network/link",
                &[("dereth-network-protocol", false)],
                false,
            ),
            pkg(
                "auberean-friends-dll",
                "auberean/friends-dll",
                &[("tracing-subscriber", false)],
                false,
            ),
            pkg(
                "dereth-network-hub",
                "network/hub",
                &[("dereth-network-protocol", false)],
                true,
            ),
        ]
    }

    /// **The four parts keep to their own**: the base layout passes, and each planted breach is
    /// found, naming the crate.
    #[test]
    fn each_part_uses_the_core_and_its_own_and_nothing_else_uses_them() {
        assert!(
            part_violations(&base()).is_empty(),
            "{:?}",
            part_violations(&base())
        );
        let plant = |i: usize, dep: (&str, bool)| {
            let mut w = base();
            w[i].deps.push(ManifestDep {
                name: dep.0.to_owned(),
                dev: dep.1,
            });
            part_violations(&w)
        };
        assert!(
            plant(6, ("empyrean-world", false))[0].contains("yalain depends on `empyrean-world`")
        );
        assert!(
            plant(7, ("yalain-net", false))[0].contains("athenaeum-cli depends on `yalain-net`")
        );
        assert!(plant(3, ("dereth-client", false))[0]
            .contains("auberean-draw depends on `dereth-client`"));
        assert!(plant(9, ("auberean-dat", false))[0].contains("nothing outside"));
        assert!(plant(0, ("yalain-net", false))[0].contains("auberean-dat depends on `yalain-net`"));
        assert!(plant(3, ("auberean-testkit", false))
            .iter()
            .any(|v| v.contains("test kit `auberean-testkit` outside its tests")));
        assert!(plant(5, ("tracing-subscriber", false))[0].contains("log subscriber"));
        // The world builder draws with the client's frame and device and builds with the world
        // database, but never takes in the server's network, game or program.
        assert!(plant(10, ("yalain-net", false))[0]
            .contains("athenaeum-editor depends on `yalain-net`"));
        assert!(plant(12, ("yalain", false))[0].contains("athenaeum-foundry depends on `yalain`"));
        assert!(plant(7, ("auberean-testkit", true))[0]
            .contains("athenaeum-cli depends on `auberean-testkit`"));
        // The server speaks the network with its shared crates; the client and the world builder
        // do not take them in, and no other crate of the network's.
        assert!(plant(3, ("dereth-network-link", false))[0]
            .contains("auberean-draw depends on `dereth-network-link`"));
        assert!(plant(12, ("dereth-network-protocol", false))[0]
            .contains("athenaeum-foundry depends on `dereth-network-protocol`"));
        assert!(plant(5, ("dereth-network-hub", false))[0]
            .contains("yalain-net depends on `dereth-network-hub`"));
        // The retail client's friends library is the outermost Rust of its process, and installs
        // its own log subscriber (the base layout's); a library beside it may not.
        assert!(plant(3, ("tracing-subscriber", false))[0].contains("log subscriber"));
    }

    /// **The launcher's knowledge may use the friends stream's codec**, and nothing else of the
    /// second game's; no other crate of the first game's may use even that.
    #[test]
    fn only_the_launchers_knowledge_uses_the_friends_stream_from_outside() {
        let with = |name: &str, dir: &str, dep: &str| {
            let mut w = base();
            w.push(pkg("auberean-friends", "core/auberean-friends", &[], false));
            w.push(pkg(name, dir, &[(dep, false)], false));
            part_violations(&w)
        };
        let launch = "launcher/crates/launch";
        assert!(with("dereth-launch", launch, "auberean-friends").is_empty());
        assert!(with("dereth-launch", launch, "auberean-dat")[0]
            .contains("dereth-launch depends on `auberean-dat`"));
        assert!(
            with("dereth-client-x", "dereth/client/x", "auberean-friends")[0]
                .contains("dereth-client-x depends on `auberean-friends`")
        );
    }

    /// **The core reaches only the core and the shared crates**, the primitives nothing and the
    /// contract the primitives.
    #[test]
    fn the_core_reaches_only_the_core_and_the_shared_crates() {
        let core: Vec<String> = [
            "auberean-primitives",
            "auberean-dat",
            "auberean-client-contract",
        ]
        .map(str::to_owned)
        .to_vec();
        let tree =
            |k: &str, d: &[&str]| (k.to_owned(), d.iter().map(|s| (*s).to_owned()).collect());
        assert!(core_dependency_violations(
            &core,
            &[tree("auberean-dat", &["auberean-primitives", "dereth-dat"])]
        )
        .is_empty());
        assert_eq!(
            core_dependency_violations(&core, &[tree("auberean-dat", &["serde"])]).len(),
            1
        );
        assert_eq!(
            core_dependency_violations(&core, &[tree("auberean-primitives", &["dereth-dat"])])
                .len(),
            1
        );
        assert_eq!(
            core_dependency_violations(
                &core,
                &[tree("auberean-client-contract", &["auberean-dat"])]
            )
            .len(),
            1
        );
    }

    /// **A shared crate's subtree is its own**: what lies under it is left out, the crate itself
    /// kept, and what lies beside it judged.
    #[test]
    fn a_shared_crates_subtree_is_left_out() {
        let tree = super::super::parse_tree(
            "0auberean-world v0.0.0
1dereth-terrain v0.0.0
2dereth-assets v0.0.0
3toml v1
1serde v1
",
        );
        assert_eq!(
            prune_shared("auberean-world", &tree),
            ["dereth-terrain", "serde"]
        );
    }

    /// **The text rules**: I/O outside the gated files, a data file named outside the locators and
    /// device input on the action seam are found in code and never in a comment.
    #[test]
    fn the_text_rules_find_code_and_skip_comments() {
        let f = |p: &str, t: &str| vec![(p.to_owned(), t.to_owned())];
        assert_eq!(
            no_io_violations(
                &f(
                    "core/auberean-world/src/a.rs",
                    "let t = std::time::Instant::now();"
                ),
                &[]
            )
            .len(),
            2
        );
        assert!(no_io_violations(
            &f("core/auberean-world/src/a.rs", "// std::fs is for hosts"),
            &[]
        )
        .is_empty());
        let lib = f("core/auberean-dat/src/lib.rs", "pub mod fs;");
        let fs = f("core/auberean-dat/src/fs.rs", "use std::fs::File;");
        assert_eq!(no_io_violations(&fs, &lib), ["core/auberean-dat/src/fs.rs is not declared behind its feature in core/auberean-dat/src/lib.rs"]);
        assert_eq!(
            data_name_violations(&f(
                "yalain/server/src/main.rs",
                "let p = dir.join(\"portal.dat\");"
            ))
            .len(),
            1
        );
        assert!(
            data_name_violations(&f("core/auberean-dat/src/locate.rs", "\"portal.dat\""))
                .is_empty()
        );
        assert_eq!(
            device_violations(&f(
                "core/auberean-client-runtime/src/a.rs",
                "fn on(k: KeyCode) {}"
            ))
            .len(),
            1
        );
        assert!(device_violations(&f(
            "core/auberean-client-runtime/src/a.rs",
            "/// not a KeyCode"
        ))
        .is_empty());
    }
}
