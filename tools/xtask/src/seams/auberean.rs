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
//!    default features. The row that builds every crate under `core/` for the browser
//!    ([`super::browser`]) holds it, and names no crate of this core as an exception.
//! 4. **One place finds the data.** No source of the four folders outside [`LOCATORS`] spells a
//!    data file's name or a test-data variable ([`DATA_NAMES`]).
//! 5. **The action seam.** The contract, the runtime, the world without a window, the SDK and the
//!    headless client name no key, mouse button or window message, nor the client shell's input
//!    ([`DEVICE_NAMES`]): a front end turns its device into actions, its keys and buttons into the
//!    contract's controls.
//! 6. **Four parts.** The client's crates use the second game's core, their own, of the first
//!    game's core only [`CLIENT_MAY_USE_CORE`] (never its contract, runtime or model: the client
//!    reads the game through its own), and [`CLIENT_MAY_USE`]; nothing of [`CLIENT_TREE_NOT`] is
//!    anywhere in the production tree of a crate of the client's or the world builder's
//!    ([`client_tree_violations`]), so a crate they may use cannot bring the first game's contract
//!    in under them; the
//!    server's the core, their own and [`SERVER_MAY_USE`] (the network's protocol and its link);
//!    the world builder's the core, its own and
//!    [`ATHENAEUM_MAY_USE`] (the client's presentation, which it plays the game in its view with,
//!    and the world database, never the server's game, network, store or program: it runs Yalain
//!    as a program), and nothing uses the world builder's; a core crate's
//!    tests may use the client's test kit and the server's network crate (the server double), the
//!    server's tests the client's test kit. Nothing of the first game's uses any of them, but the
//!    launcher's knowledge the friends stream's codec, the art tool the install's records and the
//!    session recorder the messages ([`OUTSIDE_MAY_USE`]). No crate
//!    but a test kit names a test kit outside its tests, and no library names a log subscriber
//!    ([`super::LIBRARY_NO_SUBSCRIBER`]): only an executable installs one, and the libraries in
//!    [`OUTERMOST`], loaded by a program that is not ours.
//! 7. **The application's halves reach no platform.** The client's shell, scene and interface
//!    ([`HALVES`])
//!    depend directly on no window system, operating system binding, sound device, pad or desktop
//!    host ([`HALVES_FORBIDDEN`]; what the shared crates under them reach is held by their own
//!    seams), and their code names none ([`HALVES_NAMES_NOT`]): the platform arrives through the
//!    shell's host.
//! 8. **Headless on the SDK alone.** The SDK reaches nothing of the client's crates
//!    (`auberean/client/`); the headless client depends, of this workspace's crates, on the SDK
//!    alone, and reaches nothing of the client's crates either ([`headless_violations`]): what a
//!    program without a window may drive the game with is the SDK's. Its code names none of the
//!    SDK's re-exports the window's front end still reads past the contract with
//!    ([`SDK_INTERNALS`], [`headless_names_violations`]).
//! 9. **A front end never holds the client.** The interfaces the shell shows ([`FRONT_ENDS`]) read
//!    the game through the shell's `Game` (the contract's view, the events, the notices) and change
//!    it with actions: their code names none of [`FRONT_END_NAMES_NOT`] (the client, its scripts'
//!    host, the runtime's, the scripts' and the model's crates), nor the SDK's re-exports of the
//!    model, the session or the messages ([`FRONT_END_SDK_INTERNALS`]). The front ends in
//!    [`RUNTIME_FREE_FRONT_ENDS`] name not even the SDK's runtime: the client's strings reach them
//!    as the text crate's trait, through the shell's `Game`.
//! 10. **The plugin host reaches no client.** Auberean's plugin host ([`PLUGIN_HOST`]) reaches
//!     nothing of the client's crates (`auberean/client/`) in its production tree
//!     ([`plugin_host_violations`]): plugins drive the game as a front end does, through the
//!     contract and the frame's hook. It is on the action seam (rule 5).
//!
//! The scans are textual, with comments blanked: the crates' docs may talk about the other side of
//! a seam, their code may not reach across it.

use std::path::Path;

use super::{blank_comments, workspace_packages, WorkspacePackage};
use crate::util::{Outcome, Report};

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
    // The plugin interface every game shares: its types, its packages, the compiled plugins'
    // engine and the Luau engine (reached only behind a feature, so the browser row holds).
    "dereth-plugin-api",
    "dereth-plugin-package",
    "dereth-plugin-wasm",
    "dereth-plugin-lua",
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
    (
        "core/auberean-client-sdk/src/install.rs",
        "core/auberean-client-sdk/src/lib.rs",
        "#[cfg(feature = \"fs\")]\npub mod install;",
    ),
    (
        "core/auberean-client-sdk/src/plugin_folders.rs",
        "core/auberean-client-sdk/src/lib.rs",
        "#[cfg(feature = \"fs\")]\npub mod plugin_folders;",
    ),
    (
        "core/auberean-client-sdk/src/udp.rs",
        "core/auberean-client-sdk/src/lib.rs",
        "#[cfg(feature = \"net\")]\npub mod udp;",
    ),
    (
        "core/auberean-client-sdk/src/host_clock.rs",
        "core/auberean-client-sdk/src/lib.rs",
        "#[cfg(feature = \"net\")]\npub mod host_clock;",
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
    "auberean-client-sim",
    "auberean-client-sdk",
    "auberean-headless",
    "auberean-plugin-host",
];

/// Device-input names the action seam's crates must not use: the window systems', the first
/// game's input layer's, and the client shell's own (its events, keys and buttons and their input
/// words), so nothing on the seam reaches back into the shell's input: a control comes as a
/// contract action.
pub const DEVICE_NAMES: &[&str] = &[
    "KeyCode",
    "VirtualKey",
    "MouseButton",
    "WM_KEYDOWN",
    "winit",
    "KeyMap",
    "InputMap",
    "dereth_input",
    "auberean_shell",
    "input::Event",
    "input::Key",
    "input::Button",
    "Event::Key",
    "Event::Button",
    "Event::Wheel",
    "Button::Left",
    "Button::Right",
    "Button::Middle",
    "input_word",
];

/// The first game's client crates the second's client may use: the sound device, the operating
/// system's error box and its clipboard, the remaster's backend layer and the passes both games'
/// remasters share (which name nothing of either game), what a browser's canvas asks of the
/// graphics device, and the widget kit and Horizon's art, which name no game and which the second
/// game's Horizon is drawn with. Adding one is a decision made here, in review.
pub const CLIENT_MAY_USE: &[&str] = &[
    "dereth-audio-out",
    "dereth-canvas-gpu",
    "dereth-console",
    "dereth-clipboard",
    "dereth-gfx",
    "dereth-gfx-passes",
    "dereth-ui-kit",
    "dereth-horizon-art",
];

/// The first game's core crates the second's client may name: the primitives, the sound, the
/// landscape and the physics both games' worlds are built on, the picture codecs, the wire's
/// framing, the overlay every interface draws, and the world map's drawing both games' maps share,
/// which name no game. Never the first game's contract, runtime, model or SDK. Adding one is a
/// decision made here, in review.
pub const CLIENT_MAY_USE_CORE: &[&str] = &[
    "dereth-primitives",
    "dereth-audio",
    "dereth-landscape",
    "dereth-physics",
    "dereth-render-cpu",
    "dereth-transport",
    "dereth-ui-overlay",
    "dereth-map-picture",
];

/// The first game's game crates that no crate of the client's or the world builder's holds
/// anywhere in its production tree, with every feature and for every target: its contract and
/// what is built on it. The first game's crates the client may use ([`CLIENT_MAY_USE`],
/// [`CLIENT_MAY_USE_CORE`]) name no game, and this is what holds them to it.
pub const CLIENT_TREE_NOT: &[&str] = &[
    "dereth-client-contract",
    "dereth-client-model",
    "dereth-client-runtime",
    "dereth-client-sdk",
];

/// The parts whose crates [`CLIENT_TREE_NOT`] holds: the client and the world builder, which
/// plays the game in the client's presentation.
const CLIENT_TREE_PARTS: &[&str] = &["auberean/", "athenaeum/"];

/// The crates outside the four parts and the core the server may use: the Dereth Network's
/// protocol and a world's end of its link, which name no game (the network is self-contained), so
/// that the server speaks the network with the code every world shares rather than a copy of its
/// own. (The WebSocket endpoint and the updater both game servers run are core crates.)
pub const SERVER_MAY_USE: &[&str] = &["dereth-network-protocol", "dereth-network-link"];

/// The client's and the server's crates the world builder may use: the client's presentation, which
/// it draws the world with and plays the game in its view with (the world's frame and its device,
/// the application's shell, scene and interface, the composing of a frame, the window system's keys
/// as the shell's, and the sound device the shell plays through), and the world database and the
/// server's tables (`yalain-world-db`, `yalain-content`: the world's data, not the server), which
/// it reads, checks and compiles worlds with. Nothing of the server's game, network, store or
/// program: the builder starts Yalain as a program of its own.
pub const ATHENAEUM_MAY_USE: &[&str] = &[
    "auberean-draw",
    "auberean-gpu",
    "auberean-shell",
    "auberean-scene",
    "auberean-interface",
    "auberean-horizon",
    "auberean-present",
    "auberean-client",
    "dereth-audio-out",
    "yalain-world-db",
    "yalain-content",
];

/// The crates of the second game's a crate outside the four parts may use, by that crate: the
/// launcher's knowledge, which serves both games, asks a world whether it serves the friends stream
/// and writes the friends ticket with the stream's own codec (a core crate with no I/O and no
/// dependencies), rather than a copy of its framing; and the art tool reads a player's install to
/// build the AC2 look and the AC2-in-AC1 world from it.
pub const OUTSIDE_MAY_USE: &[(&str, &str)] = &[
    ("dereth-launch", "auberean-friends"),
    // The art tool builds the AC2 look from the player's own install: it reads the install's
    // records and its region, and nothing of the second game's client, server or world builder.
    ("dereth-art", "auberean-assets"),
    ("dereth-art", "auberean-dat"),
    ("dereth-art", "auberean-world"),
    // It builds the AC2-in-AC1 world from the same install too: AC2's placed objects posed as AC2
    // poses them, its map labels' words, and its shapes' coordinates.
    ("dereth-art", "auberean-animation"),
    ("dereth-art", "auberean-text"),
    ("dereth-art", "auberean-primitives"),
    // And its sky: AC2's materials made ready as AC2 draws them, to work out each sky picture's
    // texture stages; it draws nothing.
    ("dereth-art", "auberean-draw"),
    // The session recorder names each recorded message of the second game by its type and the
    // object it is about, and nothing of its client, server or world builder.
    ("dereth-recorder", "auberean-protocol"),
    ("dereth-recorder", "auberean-client-model"),
];

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

/// The headless client, which depends on the SDK alone.
pub const HEADLESS: &str = "auberean-headless";
/// The SDK, which reaches nothing of the client's crates.
pub const SDK: &str = "auberean-client-sdk";

/// Auberean's plugin host, which reaches nothing of the client's crates.
pub const PLUGIN_HOST: &str = "auberean-plugin-host";

/// The SDK's re-exports a program without a window names none of: what the window's front end
/// still reads past the contract until the game's views are part of it.
pub const SDK_INTERNALS: &[&str] = &["runtime", "model", "net", "protocol", "script", "text"];

/// The front ends: the interfaces the shell shows. Rule 9.
pub const FRONT_ENDS: &[&str] = &["auberean-interface", "auberean-horizon"];

/// What a front end's code never names: the client and what holds it, and the crates behind it.
pub const FRONT_END_NAMES_NOT: &[&str] = &[
    "Client",
    "ClientHost",
    "Scripts",
    "with_host",
    "auberean_client_runtime",
    "auberean_script",
    "auberean_client_model",
];

/// The SDK's re-exports a front end never names: the model, the session and the messages (what
/// it reads of them comes in the view).
pub const FRONT_END_SDK_INTERNALS: &[&str] = &["model", "net", "protocol"];

/// The front ends that name not even the SDK's runtime. (The retail interface still loads the
/// install's data and links its scripts through it, and writes a few of the runtime's lines.)
pub const RUNTIME_FREE_FRONT_ENDS: &[&str] = &["auberean-horizon"];

/// Whether a crate's folder is the client's or one of its crates'.
fn client_dir(dir: &str) -> bool {
    dir == "auberean/client" || dir.starts_with("auberean/client/")
}

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
                    theirs == Some("core/auberean-")
                        || (in_core && CLIENT_MAY_USE_CORE.contains(&d.name.as_str()))
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
                    Some("auberean/") if in_core => format!(
                        "{} depends on `{}`{test}: the client names of the first game's core only {}",
                        p.name,
                        d.name,
                        CLIENT_MAY_USE_CORE.join(", ")
                    ),
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

/// Rule 6 over what holds the first game's game crates: `dependents` gives each of
/// [`CLIENT_TREE_NOT`] with every crate whose production tree holds it (its inverted tree). A
/// crate of the client's or the world builder's among them is a violation.
#[must_use]
pub fn client_tree_violations(
    packages: &[WorkspacePackage],
    dependents: &[(String, Vec<String>)],
) -> Vec<String> {
    let dir_of = |name: &str| {
        packages
            .iter()
            .find(|p| p.name == name)
            .map(|p| p.dir.as_str())
    };
    let mut out = Vec::new();
    for (refused, holders) in dependents {
        for h in holders {
            if h != refused
                && dir_of(h)
                    .and_then(part_of)
                    .is_some_and(|part| CLIENT_TREE_PARTS.contains(&part))
            {
                out.push(format!(
                    "{h} holds `{refused}` in its tree: the client reads the game through its own \
                     contract, and what it uses of the first game's names no game"
                ));
            }
        }
    }
    super::dedup(out)
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

/// Rule 8: the headless client's own dependencies (from `packages`), and what it and the SDK reach
/// (`trees`: crate, every name in its production tree).
#[must_use]
pub fn headless_violations(
    packages: &[WorkspacePackage],
    trees: &[(String, Vec<String>)],
) -> Vec<String> {
    let dir_of = |name: &str| {
        packages
            .iter()
            .find(|p| p.name == name)
            .map(|p| p.dir.as_str())
    };
    let mut out = Vec::new();
    for p in packages.iter().filter(|p| p.name == HEADLESS) {
        for d in p.deps.iter().filter(|d| !d.dev) {
            if d.name != SDK && dir_of(&d.name).is_some() {
                out.push(format!(
                    "{HEADLESS} depends on `{}`: of this workspace's crates it uses {SDK} alone",
                    d.name
                ));
            }
        }
    }
    for (krate, reached) in trees {
        for d in reached {
            if dir_of(d).is_some_and(client_dir) {
                out.push(format!(
                    "{krate} reaches `{d}`: nothing of the client's crates is under it"
                ));
            }
        }
    }
    out
}

/// Rule 10: what the plugin host's production tree reaches (`tree`: every name in it) of the
/// client's crates.
#[must_use]
pub fn plugin_host_violations(packages: &[WorkspacePackage], tree: &[String]) -> Vec<String> {
    let dir_of = |name: &str| {
        packages
            .iter()
            .find(|p| p.name == name)
            .map(|p| p.dir.as_str())
    };
    tree.iter()
        .filter(|d| dir_of(d).is_some_and(client_dir))
        .map(|d| {
            format!("{PLUGIN_HOST} reaches `{d}`: plugins reach the game through the contract")
        })
        .collect()
}

/// Rule 8 over the headless client's sources (`files`: path, text): none names one of
/// [`SDK_INTERNALS`] through the SDK, by path or in a group of names.
#[must_use]
pub fn headless_names_violations(files: &[(String, String)]) -> Vec<String> {
    sdk_names_violations(files, SDK_INTERNALS)
}

/// Rule 9 over the front ends' sources (`files`: path, text): none names one of
/// [`FRONT_END_NAMES_NOT`], nor one of [`FRONT_END_SDK_INTERNALS`] through the SDK.
#[must_use]
pub fn front_end_violations(files: &[(String, String)]) -> Vec<String> {
    let mut out = Vec::new();
    for (path, text) in files {
        let code = blank_comments(text);
        for (i, line) in code.lines().enumerate() {
            for name in FRONT_END_NAMES_NOT {
                if super::names_ident(line, name) {
                    out.push(format!("{path}:{}: names `{name}`", i + 1));
                }
            }
        }
    }
    out.extend(sdk_names_violations(files, FRONT_END_SDK_INTERNALS));
    out
}

/// Rule 9 over the sources of the front ends in [`RUNTIME_FREE_FRONT_ENDS`] (`files`: path,
/// text): none names the SDK's runtime, by path or in a group of names.
#[must_use]
pub fn runtime_free_violations(files: &[(String, String)]) -> Vec<String> {
    sdk_names_violations(files, &["runtime"])
}

/// The sources in `files` that name one of `internals` through the SDK, by path or in a group of
/// names.
fn sdk_names_violations(files: &[(String, String)], internals: &[&str]) -> Vec<String> {
    const SDK_PATH: &str = "auberean_client_sdk::";
    let ident = |c: char| c.is_alphanumeric() || c == '_';
    let mut out = Vec::new();
    for (path, text) in files {
        let code = blank_comments(text);
        let mut named = Vec::new();
        for (at, _) in code.match_indices(SDK_PATH) {
            let rest = &code[at + SDK_PATH.len()..];
            if let Some(group) = rest.strip_prefix('{') {
                // The first name of each item in the group, to its closing brace.
                let mut depth = 1usize;
                let end = group
                    .char_indices()
                    .find(|&(_, c)| {
                        match c {
                            '{' => depth += 1,
                            '}' => depth -= 1,
                            _ => {}
                        }
                        depth == 0
                    })
                    .map_or(group.len(), |(i, _)| i);
                let mut depth = 0usize;
                let mut item_start = true;
                let mut word = String::new();
                for c in group[..end].chars() {
                    if ident(c) {
                        word.push(c);
                        continue;
                    }
                    if item_start && depth == 0 && !word.is_empty() {
                        named.push(std::mem::take(&mut word));
                        item_start = false;
                    }
                    word.clear();
                    match c {
                        '{' => depth += 1,
                        '}' => depth = depth.saturating_sub(1),
                        ',' if depth == 0 => item_start = true,
                        _ => {}
                    }
                }
                if item_start && !word.is_empty() {
                    named.push(word);
                }
            } else {
                named.push(rest.chars().take_while(|c| ident(*c)).collect());
            }
        }
        for name in named {
            if internals.contains(&name.as_str())
                && !out.iter().any(|l: &String| {
                    l.starts_with(path.as_str()) && l.ends_with(&format!("`{SDK_PATH}{name}`"))
                })
            {
                out.push(format!("{path} names `{SDK_PATH}{name}`"));
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
    let headless = packages.as_ref().map_err(Clone::clone).and_then(|ps| {
        let src: Vec<(String, String)> = ps
            .iter()
            .filter(|p| p.name == HEADLESS)
            .flat_map(|p| sources(ws, &p.dir, &["src"]))
            .collect();
        if src.is_empty() {
            return Err(format!("no sources found for {HEADLESS}"));
        }
        [HEADLESS, SDK]
            .iter()
            .map(|k| {
                super::cargo_tree(ws, k, "normal")
                    .map(|t| ((*k).to_owned(), t.into_iter().map(|e| e.name).collect()))
            })
            .collect::<Result<Vec<_>, _>>()
            .map(|trees| {
                let mut v = headless_violations(ps, &trees);
                v.extend(headless_names_violations(&src));
                v
            })
    });
    let client_tree = packages.as_ref().map_err(Clone::clone).and_then(|ps| {
        CLIENT_TREE_NOT
            .iter()
            .map(|k| {
                super::cargo_tree_args(
                    ws,
                    &[
                        "--workspace",
                        "--all-features",
                        "--target",
                        "all",
                        "-e",
                        "normal",
                        "-i",
                        k,
                    ],
                )
                .map(|t| ((*k).to_owned(), t.into_iter().map(|e| e.name).collect()))
            })
            .collect::<Result<Vec<_>, _>>()
            .map(|dependents| client_tree_violations(ps, &dependents))
    });
    let plugin_host = packages.as_ref().map_err(Clone::clone).and_then(|ps| {
        super::cargo_tree(ws, PLUGIN_HOST, "normal").map(|t| {
            let names: Vec<String> = t.into_iter().map(|e| e.name).collect();
            plugin_host_violations(ps, &names)
        })
    });
    let front_src: Vec<(String, String)> = packages
        .as_ref()
        .map(|ps| {
            ps.iter()
                .filter(|p| FRONT_ENDS.contains(&p.name.as_str()))
                .flat_map(|p| sources(ws, &p.dir, &["src"]))
                .collect()
        })
        .unwrap_or_default();
    let runtime_free_src: Vec<(String, String)> = packages
        .as_ref()
        .map(|ps| {
            ps.iter()
                .filter(|p| RUNTIME_FREE_FRONT_ENDS.contains(&p.name.as_str()))
                .flat_map(|p| sources(ws, &p.dir, &["src"]))
                .collect()
        })
        .unwrap_or_default();
    let front_end = if front_src.is_empty() || runtime_free_src.is_empty() {
        Err(format!("no sources found for {}", FRONT_ENDS.join(", ")))
    } else {
        let mut v = front_end_violations(&front_src);
        v.extend(runtime_free_violations(&runtime_free_src));
        Ok(v)
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
            "seam: Auberean headless and SDK",
            headless,
            format!(
                "{HEADLESS} uses {SDK} alone of this workspace's crates and names none of its {}; \
                 neither reaches the client's crates",
                SDK_INTERNALS.join(", ")
            ),
        ),
        row(
            "seam: Auberean plugin host",
            plugin_host,
            format!("{PLUGIN_HOST} reaches nothing of the client's crates"),
        ),
        row(
            "seam: Auberean front end -> view",
            front_end,
            format!(
                "{} name none of {}, nor the SDK's {}; {} not even its runtime",
                FRONT_ENDS.join(", "),
                FRONT_END_NAMES_NOT.join(", "),
                FRONT_END_SDK_INTERNALS.join(", "),
                RUNTIME_FREE_FRONT_ENDS.join(", ")
            ),
        ),
        row(
            "seam: Auberean client tree",
            client_tree,
            format!(
                "nothing of the client's or the world builder's holds {}",
                CLIENT_TREE_NOT.join(", ")
            ),
        ),
        row(
            "seam: Asheron's Call 2 parts",
            parts,
            "the client, the server and the world builder use the core, their own and their \
             reviewed lists (the world builder the client's presentation, never the server's \
             game, network, store or program); nothing else uses them; test kits only in tests; \
             executables alone install a log subscriber"
                .to_owned(),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::super::ManifestDep;
    use super::*;

    #[test]
    fn a_front_end_naming_the_client_or_the_model_is_caught() {
        let files = vec![
            (
                "a.rs".to_owned(),
                "// the Client is the shell's\nstruct ChatClient;\nuse auberean_client_sdk::contract::view::View;\nuse auberean_client_sdk::runtime::text::from_view;".to_owned(),
            ),
            (
                "b.rs".to_owned(),
                "fn f(c: &mut Client) {}\nuse auberean_client_sdk::{model::Object, Action};\nuse auberean_client_sdk::protocol::types::VisualDesc;".to_owned(),
            ),
        ];
        let v = front_end_violations(&files);
        assert_eq!(v.len(), 3, "{v:?}");
        assert!(v[0].starts_with("b.rs:1: names `Client`"), "{v:?}");
        assert!(
            v.iter()
                .any(|l| l.ends_with("`auberean_client_sdk::model`")),
            "{v:?}"
        );
        assert!(
            v.iter()
                .any(|l| l.ends_with("`auberean_client_sdk::protocol`")),
            "{v:?}"
        );
        assert!(front_end_violations(&files[..1]).is_empty());
    }

    #[test]
    fn a_runtime_free_front_end_naming_the_sdks_runtime_is_caught() {
        let files = vec![
            (
                "a.rs".to_owned(),
                "// auberean_client_sdk::runtime writes it
use auberean_client_sdk::text::CachedStrings;
use auberean_client_sdk::contract::text::TextRef;"
                    .to_owned(),
            ),
            (
                "b.rs".to_owned(),
                "let info = auberean_client_sdk::runtime::text::from_view(t);".to_owned(),
            ),
            (
                "c.rs".to_owned(),
                "use auberean_client_sdk::{contract::Action, runtime::strings::StringDownloads};"
                    .to_owned(),
            ),
        ];
        let v = runtime_free_violations(&files);
        assert_eq!(
            v,
            [
                "b.rs names `auberean_client_sdk::runtime`",
                "c.rs names `auberean_client_sdk::runtime`"
            ],
            "{v:?}"
        );
        assert!(
            front_end_violations(&files).is_empty(),
            "the other front ends may"
        );
        assert!(RUNTIME_FREE_FRONT_ENDS
            .iter()
            .all(|f| FRONT_ENDS.contains(f)));
    }

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
            pkg("dereth-render-cpu", "core/render-cpu", &[], false),
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
            pkg("auberean-shell", "auberean/client/crates/shell", &[], false),
            pkg("auberean-scene", "auberean/client/crates/scene", &[], false),
            pkg(
                "auberean-interface",
                "auberean/client/crates/interface",
                &[],
                false,
            ),
            pkg(
                "auberean-present",
                "auberean/client/crates/present",
                &[],
                false,
            ),
            pkg(
                "auberean-ui-game",
                "auberean/client/crates/ui-game",
                &[],
                false,
            ),
            pkg("auberean-client", "auberean/client", &[], true),
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
        // It plays the game in its view with the client's presentation, and no more of the
        // client's than that list.
        for d in [
            "auberean-shell",
            "auberean-scene",
            "auberean-interface",
            "auberean-present",
            "auberean-client",
        ] {
            assert!(plant(10, (d, false)).is_empty(), "{d}");
        }
        assert!(plant(10, ("auberean-ui-game", false))[0]
            .contains("athenaeum-editor depends on `auberean-ui-game`"));
        // Nothing uses the world builder's crates: not the client, the server or the core.
        assert!(plant(3, ("athenaeum-foundry", false))[0]
            .contains("auberean-draw depends on `athenaeum-foundry`"));
        assert!(plant(6, ("athenaeum-foundry", false))[0]
            .contains("yalain depends on `athenaeum-foundry`"));
        assert!(plant(0, ("athenaeum-editor", false))[0]
            .contains("auberean-dat depends on `athenaeum-editor`"));
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

    /// **The client draws with the first game's game-free crates and no more of it**: the widget
    /// kit, Horizon's art and the overlay are allowed, with the core it already names; the first
    /// game's contract, its Horizon or any other of its client crates are found, naming the crate.
    #[test]
    fn the_client_uses_the_first_game_s_kit_art_and_overlay_and_nothing_else_of_its_game() {
        let mut w = base();
        w.extend([
            pkg("dereth-ui-overlay", "core/ui-overlay", &[], false),
            pkg("dereth-client-contract", "core/client-contract", &[], false),
            pkg("dereth-client-runtime", "core/client-runtime", &[], false),
            pkg("dereth-ui-kit", "dereth/client/crates/ui-kit", &[], false),
            pkg(
                "dereth-horizon-art",
                "dereth/client/crates/horizon-art",
                &[],
                false,
            ),
            pkg("dereth-horizon", "dereth/client/crates/horizon", &[], false),
            pkg(
                "dereth-ui-screens",
                "dereth/client/crates/ui-screens",
                &[],
                false,
            ),
        ]);
        let shell = w.iter().position(|p| p.name == "auberean-shell").unwrap();
        let plant = |dep: &str| {
            let mut w = w.clone();
            w[shell].deps.push(ManifestDep {
                name: dep.to_owned(),
                dev: false,
            });
            part_violations(&w)
        };
        for allowed in [
            "dereth-ui-overlay",
            "dereth-ui-kit",
            "dereth-horizon-art",
            "dereth-render-cpu",
        ] {
            assert!(plant(allowed).is_empty(), "{allowed}: {:?}", plant(allowed));
        }
        for refused in [
            "dereth-client-contract",
            "dereth-client-runtime",
            "dereth-dat",
        ] {
            let found = plant(refused);
            assert!(
                found.len() == 1
                    && found[0].contains(&format!("auberean-shell depends on `{refused}`"))
                    && found[0].contains("first game's core only"),
                "{refused}: {found:?}"
            );
        }
        for refused in ["dereth-horizon", "dereth-ui-screens", "dereth-client"] {
            let found = plant(refused);
            assert!(
                found.len() == 1
                    && found[0].contains(&format!("auberean-shell depends on `{refused}`")),
                "{refused}: {found:?}"
            );
        }
    }

    /// **No crate of the client's or the world builder's holds the first game's contract**, even
    /// through a crate it may use; the first game's own crates and the server's test kit may.
    #[test]
    fn the_client_holds_the_first_game_s_contract_nowhere_in_its_tree() {
        let mut w = base();
        w.extend([
            pkg("dereth-client-contract", "core/client-contract", &[], false),
            pkg("dereth-render", "dereth/client/crates/render", &[], false),
            pkg("yalain-testkit", "yalain/testkit", &[], false),
            pkg("athenaeum-editor", "athenaeum/editor", &[], false),
        ]);
        let holders = |names: &[&str]| {
            vec![(
                "dereth-client-contract".to_owned(),
                std::iter::once("dereth-client-contract")
                    .chain(names.iter().copied())
                    .map(str::to_owned)
                    .collect::<Vec<_>>(),
            )]
        };
        assert!(
            client_tree_violations(&w, &holders(&["dereth-render", "yalain-testkit"])).is_empty()
        );
        let found = client_tree_violations(
            &w,
            &holders(&[
                "dereth-render-cpu",
                "auberean-draw",
                "athenaeum-editor",
                "dereth-render",
            ]),
        );
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(found[0].starts_with("athenaeum-editor holds `dereth-client-contract`"));
        assert!(found[1].starts_with("auberean-draw holds `dereth-client-contract`"));
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

    /// **Headless runs on the SDK alone**: another of the workspace's crates beside the SDK, or a
    /// client crate anywhere under headless or the SDK, is caught; a crate from outside the
    /// workspace and a test's dependency are not.
    #[test]
    fn headless_uses_the_sdk_alone_and_neither_reaches_the_clients_crates() {
        let packages = vec![
            pkg(
                HEADLESS,
                "auberean/headless",
                &[(SDK, false), ("serde", false), ("auberean-testkit", true)],
                false,
            ),
            pkg(SDK, "core/auberean-client-sdk", &[], false),
            pkg("auberean-testkit", "auberean/testkit", &[], false),
            pkg("auberean-shell", "auberean/client/crates/shell", &[], false),
            pkg("auberean-client", "auberean/client", &[], false),
        ];
        let tree =
            |k: &str, d: &[&str]| (k.to_owned(), d.iter().map(|s| (*s).to_owned()).collect());
        let clean = [tree(HEADLESS, &[SDK, "serde"]), tree(SDK, &["serde"])];
        assert!(headless_violations(&packages, &clean).is_empty());
        let mut reaching = packages.clone();
        reaching[0].deps.push(ManifestDep {
            name: "auberean-testkit".to_owned(),
            dev: false,
        });
        let v = headless_violations(&reaching, &clean);
        assert_eq!(v.len(), 1, "{v:?}");
        assert!(v[0].contains("`auberean-testkit`"));
        let v = headless_violations(
            &packages,
            &[
                tree(HEADLESS, &[SDK, "auberean-client"]),
                tree(SDK, &["auberean-shell"]),
            ],
        );
        assert_eq!(v.len(), 2, "{v:?}");
        assert!(v[0].starts_with(HEADLESS) && v[1].starts_with(SDK));
    }

    /// **The plugin host reaches no client crate**: a client crate anywhere in its tree is caught;
    /// the contract, the runtime and the shared plugin crates are not.
    #[test]
    fn the_plugin_host_reaches_none_of_the_clients_crates() {
        let packages = vec![
            pkg(PLUGIN_HOST, "core/auberean-plugin-host", &[], false),
            pkg(
                "auberean-client-runtime",
                "core/auberean-client-runtime",
                &[],
                false,
            ),
            pkg("dereth-plugin-api", "core/plugin-api", &[], false),
            pkg("auberean-shell", "auberean/client/crates/shell", &[], false),
            pkg("auberean-client", "auberean/client", &[], false),
        ];
        let names = |d: &[&str]| d.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert!(plugin_host_violations(
            &packages,
            &names(&["auberean-client-runtime", "dereth-plugin-api", "serde"])
        )
        .is_empty());
        let v = plugin_host_violations(
            &packages,
            &names(&[
                "auberean-client-runtime",
                "auberean-shell",
                "auberean-client",
            ]),
        );
        assert_eq!(v.len(), 2, "{v:?}");
        assert!(v[0].contains("`auberean-shell`") && v[1].contains("`auberean-client`"));
        assert!(ACTION_SEAM.contains(&PLUGIN_HOST));
    }

    /// **Headless names none of the SDK's internals**: a path or a group naming one is caught, once
    /// a file and name; the contract, the driver, a longer name that starts like one, and a
    /// comment are not.
    #[test]
    fn headless_names_none_of_the_sdks_internals() {
        let f = |text: &str| vec![("auberean/headless/src/main.rs".to_owned(), text.to_owned())];
        let clean = f("use auberean_client_sdk::contract::event::GameEvent;\n\
             use auberean_client_sdk::{install, Driver, udp::UdpPort};\n\
             use auberean_client_sdk::networking::X;\n\
             // auberean_client_sdk::runtime is the front end's\n\
             fn f() {}");
        assert!(headless_names_violations(&clean).is_empty());
        let v = headless_names_violations(&f("use auberean_client_sdk::net::LocalTime;\n\
             use auberean_client_sdk::{contract::{Action, Motion}, runtime::data, Client};\n\
             let x = auberean_client_sdk::net::LocalTime(1.0);"));
        assert_eq!(
            v,
            [
                "auberean/headless/src/main.rs names `auberean_client_sdk::net`",
                "auberean/headless/src/main.rs names `auberean_client_sdk::runtime`",
            ]
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

    /// **The action seam** refuses the client shell's own input: its events, keys and buttons and
    /// their input words, named from a crate on the seam; a control from the contract passes.
    #[test]
    fn the_action_seam_refuses_the_shells_input() {
        let f = |t: &str| vec![("auberean/headless/src/main.rs".to_owned(), t.to_owned())];
        for code in [
            "use auberean_shell::input::Event;",
            "match e { Event::Key { key, .. } => {} }",
            "let b = Button::Left;",
            "let w = key.input_word();",
        ] {
            assert!(!device_violations(&f(code)).is_empty(), "{code}");
        }
        let control = "client.act(Action::Control(Control::begin(movement::FORWARD)));";
        assert!(device_violations(&f(control)).is_empty());
    }
}
