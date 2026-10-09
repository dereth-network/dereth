//! The seams between the game, the contract and the modern UI, checked rather than described.
//!
//! Four crates, and the client's own crates, sit on the boundaries the code is being cut along,
//! and each has a rule that until now lived only in a doc comment:
//!
//! 0. **`dereth-primitives`** is plain data and pure conversions: no domain logic, no engine code,
//!    no I/O. Its direct production dependencies are exactly [`PRIMITIVES_DIRECT`], nothing in its
//!    tree is another `dereth-*` crate or a platform crate, its code names no I/O facility of
//!    [`PRIMITIVES_NO_IO`], and the only `unsafe` in it is `core/primitives/src/text/windows.rs`, the host NLS arm,
//!    declared behind the `host-nls` feature.
//! 1. **`dereth-client-contract`** is the read-only view onto the game and the request channel
//!    back out. Its direct production dependencies are exactly [`CONTRACT_DIRECT`], and nothing it
//!    pulls in, transitively, is another `dereth-*` crate or a presentation or platform crate.
//! 2. **`dereth-ui-screens`** is the modern UI. It reads the game through the contract only, so it
//!    never names `dereth-client-model` or `dereth-client-runtime` -- not as a dependency of any
//!    kind, and not as a path in its code.
//! 3. **`dereth-client-runtime`** is the game without a UI, a device or a window. Its production
//!    dependency tree holds none of [`CORE_FORBIDDEN`] and no crate under `dereth/client/crates/`,
//!    and its code never names the modern UI or the drawing crates ([`CORE_NAMES_NOT`]).
//! 4. **`dereth-headless`** is the client with no UI, no device and no window, built on the client
//!    SDK alone: its only `dereth-*` dependency, of any kind, is `dereth-client-sdk`
//!    ([`HEADLESS_DERE`]), and nothing under `dereth/client/` is anywhere in its tree.
//! 5. **The client's own crates** (`dereth/client/crates/*`) are presentation. Those that draw or
//!    lay out the game ([`PRESENTATION_ON_CONTRACT`]) depend directly on `dereth-client-contract`,
//!    and none of them has `dereth-client-runtime` anywhere in any dependency table: rendering
//!    sees the game only through the contract.
//! 6. **The SDK's input seam is actions.** `dereth-client-sdk` reaches no crate under
//!    `dereth/client/` (the device input, `dereth-input`, is one of them), and the code of the
//!    three crates the SDK side is built from ([`ACTION_SEAM_CRATES`]) names no key, key map,
//!    input map or device message ([`DEVICE_NAMES_NOT`]): a bot, a script or another front end
//!    tells the runtime what the player does with an action, and turning a device into actions is
//!    the product's own business.
//! 7. **Libraries log through the facade.** No crate under `core/` or `dereth/client/crates/` has
//!    a log subscriber ([`LIBRARY_NO_SUBSCRIBER`]) anywhere in its production tree: they emit
//!    `tracing` events, and the binary that runs them installs the one subscriber that decides
//!    where the events go. (`dereth-primitives` does not log at all; rule 0 keeps `tracing` out of
//!    its tree.)
//! 9. **The application's halves** ([`APPLICATION_HALVES`]): `dereth-scene`, the drawn world,
//!    and the front ends: `dereth-client-shell`, the retail one and the shell that runs any of
//!    them, `dereth-classic-ui`, the classic one, and `dereth-horizon`, the Horizon one. They sit on the runtime, so rule 5's runtime ban
//!    does not apply to them, and they reach no platform: no table of theirs names a platform
//!    crate or the desktop's own platform crates ([`HALVES_FORBIDDEN`]), and their code names none
//!    of them ([`HALVES_NAMES_NOT`]). The platform arrives through the shell's host trait, which
//!    the desktop host (`dereth-desktop`) and the browser client implement. The scene does not depend on the shell.
//! 10. **A front end never holds the application.** A front end's code ([`FRONT_ENDS`]: the
//!     shell's `src/`, but for [`FRONT_END_ASSEMBLY`], where the executable puts the application
//!     and its front end side by side, and the classic and Horizon interfaces') names none of
//!     [`FRONT_END_NAMES_NOT`]: the
//!     application and the runtime's internals behind it. Each step of the frame hands a front end
//!     a `UiContext`, and that is all of the game it reaches -- the same for the modern UI as for
//!     any other. (Its screens, `dereth-ui-screens`, cannot name the runtime at all: rule 2.)
//! 8. **One place finds the retail dats.** No workspace file outside `core/dat/` joins a retail dat
//!    file name onto a path ([`dat_path_violations`]): the client, the server and every test find
//!    the directory with `dereth_dat::locate_modern_dats` (the tests through `dereth_dat::testing`)
//!    and name a file in it with `dereth_dat::ModernDat`. And no tracked file of the workspace
//!    names a retired environment variable ([`env_name_violations`]).
//! 11. **The optional high-fidelity presentation changes only pixels.** `dereth-render-hifi`'s
//!     tree, with every feature and for every target, holds no workspace crate but
//!     [`HIFI_TREE_DERE`] and none of [`HIFI_TREE_FORBIDDEN`], and its manifest declares nothing
//!     but those and [`HIFI_DIRECT_EXTERNAL`] ([`HIFI_DEV_EXTRA`] for its tests); its code and
//!     build script name none of [`HIFI_NAMES_NOT`] and no client-past-the-device or server
//!     crate, and its production code not the landscape crate; `dereth-scene` is its only
//!     dependent; only the scene's bridge ([`HIFI_BRIDGE`]) names it, and the bridge names
//!     nothing that writes the scene or the simulation ([`BRIDGE_NAMES_NOT`],
//!     [`BRIDGE_PREFIXES_NOT`]), makes no write through a shared reference
//!     ([`BRIDGE_CALLS_NOT`]) and calls only reviewed reads on the scene and the world
//!     ([`BRIDGE_READS`]); the device's seam to it ([`HIFI_SEAM`]) writes the device's own state
//!     ([`SEAM_FIELDS_NOT_WRITTEN`]) directly only on the lines of [`SEAM_WRITES_ALLOWED`]; and
//!     the browser and headless clients, each package built on its own for every target
//!     ([`SHIPPED_CLIENTS`]), hold none of the presentation (the desktop client carries it, drawn
//!     only under its Horizon interface). The scans are textual: a write made inside a method the
//!     seam calls, or through a name the bridge gave a value of its own, is outside what they see.
//!
//! The dependency half reads `cargo tree`, so a rule broken two crates down is still caught. The
//! code half is a text scan with comments stripped: the crates' docs are allowed to talk about the
//! other side of a seam, their code is not allowed to reach across it.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::util::{print_table, workspace_root, Outcome, Report};

/// `dereth-primitives`' allowed direct production dependencies, with its default features: the
/// error derive (`thiserror`) and the two portable math libraries `num::math` is built on (`pxfm`,
/// `libm`). Adding one is a decision about the lowest crate in the workspace, made here in review.
/// No geometry library: the shapes are over the crate's own `Vec3`.
pub const PRIMITIVES_DIRECT: &[&str] = &["thiserror", "pxfm", "libm"];

/// Platform crates `dereth-primitives` must not reach, at any depth.
const PRIMITIVES_FORBIDDEN: &[&str] = &["winit", "windows", "windows-sys", "cpal", "ash", "libc"];

/// What `dereth-primitives`' production code must not name: the standard library's I/O, process,
/// thread, environment and clock facilities. (`std::io::Error` as a *type* in an error enum is
/// data, and is allowed.)
pub const PRIMITIVES_NO_IO: &[&str] = &[
    "std::fs",
    "std::net",
    "std::process",
    "std::thread",
    "std::env",
    "std::time",
    "std::io::stdin",
    "std::io::stdout",
    "std::io::stderr",
    "std::io::Read",
    "std::io::Write",
];

/// The one file of `dereth-primitives` allowed `unsafe` and `extern` blocks, and the gate its
/// module declaration must carry.
const PRIMITIVES_UNSAFE_FILE: &str = "src/text/windows.rs";
const PRIMITIVES_UNSAFE_DECL: &str =
    "#[cfg(all(windows, feature = \"host-nls\"))]\n#[allow(unsafe_code)]\nmod windows;";

/// `dereth-client-contract`'s allowed direct production dependencies. Adding one is a decision about the
/// seam, so it is made here, in review, and not by editing a manifest.
pub const CONTRACT_DIRECT: &[&str] = &["dereth-primitives", "glam", "raw-window-handle"];

/// The only `dereth-*` crates allowed anywhere in `dereth-client-contract`'s production tree.
const CONTRACT_DERE: &[&str] = &["dereth-client-contract", "dereth-primitives"];

/// What `dereth-ui-screens` must never depend on, in any dependency table.
const SCREENS_FORBIDDEN: &[&str] = &[
    "dereth-client-model",
    "dereth-client-runtime",
    "dereth-client",
];

/// What `dereth-client-runtime`'s production tree must never contain: a UI, a device, the drawing,
/// the application or a platform. Every crate under `dereth/client/crates/` is added to this list
/// when the check runs, so a new presentation crate is covered without editing it.
pub const CORE_FORBIDDEN: &[&str] = &[
    "dereth-client",
    "dereth-ui",
    "dereth-ui-screens",
    "dereth-render",
    "dereth-render-cpu",
    "dereth-world-render",
    "dereth-clipboard",
    "dereth-console",
    "winit",
    "windows",
    "cpal",
    "ash",
    "gilrs",
];

/// What `dereth-client-runtime`'s code must never name.
pub const CORE_NAMES_NOT: &[&str] = &[
    "dereth_ui_screens",
    "dereth_ui",
    "dereth_render",
    "dereth_render_cpu",
    "dereth_world_render",
];

/// The only `dereth-*` crate `dereth-headless` may depend on directly, in any dependency table.
pub const HEADLESS_DERE: &[&str] = &["dereth-client-sdk"];

/// The crates the SDK side is built from, whose code must speak actions and never devices.
pub const ACTION_SEAM_CRATES: &[&str] = &[
    "dereth-client-runtime",
    "dereth-client-sdk",
    "dereth-headless",
];

/// What the code of [`ACTION_SEAM_CRATES`] must never name: the device input crate, its key,
/// key-map and input-map types, the message it reads, and the keyboard and mouse window messages
/// and virtual keys. The window's lifecycle messages (close, focus, size) are not here: the
/// runtime owns the window procedure's state and feeds it those.
pub const DEVICE_NAMES_NOT: &[&str] = &[
    "dereth_input",
    "InputEvent",
    "InputManager",
    "InputShell",
    "InputMapId",
    "MasterInputMap",
    "ControlCode",
    "ControlChord",
    "ToggleType",
    "Win32Message",
    "KeyCode",
    "KeyboardInput",
    "MouseInput",
    "scan_code",
    "virtual_key",
    "WM_KEYDOWN",
    "WM_KEYUP",
    "WM_SYSKEYDOWN",
    "WM_SYSKEYUP",
    "WM_CHAR",
    "WM_SYSCHAR",
    "WM_MOUSEMOVE",
    "WM_MOUSEWHEEL",
    "WM_MOUSELEAVE",
    "WM_LBUTTONDOWN",
    "WM_LBUTTONUP",
    "WM_RBUTTONDOWN",
    "WM_RBUTTONUP",
    "WM_MBUTTONDOWN",
    "WM_MBUTTONUP",
    "WM_XBUTTONDOWN",
    "WM_XBUTTONUP",
    "VK_RETURN",
    "VK_F4",
];

/// The client's own crates that draw or lay out the game, and so read it through the contract: each
/// depends directly on `dereth-client-contract`.
pub const PRESENTATION_ON_CONTRACT: &[&str] = &[
    "dereth-render",
    "dereth-render-cpu",
    "dereth-ui",
    "dereth-ui-screens",
];

/// What no crate under `dereth/client/crates/` may have in any dependency table, at any depth,
/// except the application's two halves ([`APPLICATION_HALVES`]), which sit on the runtime.
const PRESENTATION_FORBIDDEN: &[&str] = &["dereth-client-runtime"];

/// The application's two halves: the drawn world and the front end over the runtime. Rule 9.
pub const APPLICATION_HALVES: &[&str] = &[
    "dereth-scene",
    "dereth-client-shell",
    "dereth-classic-ui",
    "dereth-horizon",
];

/// The front ends rule 10 holds to their context.
pub const FRONT_ENDS: &[&str] = &["dereth-client-shell", "dereth-classic-ui", "dereth-horizon"];

/// What no dependency table of an application half may name directly: the window system, the
/// operating system's bindings, the sound device, the pads, and the desktop's own platform crates.
pub const HALVES_FORBIDDEN: &[&str] = &[
    "winit",
    "windows",
    "windows-sys",
    "cpal",
    "gilrs",
    "dereth-client",
    "dereth-desktop",
    "dereth-clipboard",
    "dereth-console",
];

/// The crates whose paths the code of an application half must never name: `name::` anywhere, so
/// a `cfg(windows)` or a field called `windows` is not a path.
pub const HALVES_NAMES_NOT: &[&str] = &[
    "winit",
    "windows",
    "cpal",
    "gilrs",
    "dereth_client",
    "dereth_desktop",
    "dereth_clipboard",
    "dereth_console",
];

/// The one file of the retail front end's crate that holds the application: the assembly, which
/// builds the runtime's application beside the front end and feeds the one to the other.
pub const FRONT_END_ASSEMBLY: &str = "src/app.rs";

/// What a front end's code must not name: the application, and the runtime's internals a holder of
/// it would reach (the interaction state, the network link).
pub const FRONT_END_NAMES_NOT: &[&str] = &["App", "CoreApp", "Interaction", "NetLink"];

/// Presentation and platform crates the contract must not reach either.
const CONTRACT_FORBIDDEN: &[&str] = &["winit", "windows", "cpal", "ash"];

/// What the host's production code must not name: the concrete screens. It reaches them through
/// the `dereth_ui::framework::Screen` hooks (`on_pregame`, `on_game`, `on_mode_action`,
/// `on_host_call`, ...) with the screen crate's call vocabularies.
const HOST_NAMES_NO_SCREEN: [&str; 8] = [
    "GamePlayScreen",
    "CharGenScreen",
    "CharacterManagementScreen",
    "DataPatchScreen",
    "DisconnectedScreen",
    "IntroScreen",
    "CreditsScreen",
    "EpilogueScreen",
];

/// What no library crate's production tree may contain, at any depth: a log subscriber. The shared
/// crates under `core/` and the client's crates under `dereth/client/crates/` emit events through
/// the `tracing` facade and nothing else; which subscriber receives them, and where it writes, is
/// the binary's decision (`dereth-client`, `dereth-headless`, and the server through the `log`
/// facade).
pub const LIBRARY_NO_SUBSCRIBER: &[&str] = &["tracing-subscriber"];

/// One package in a `cargo tree --prefix depth` listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeEntry {
    pub depth: usize,
    pub name: String,
}

/// Parse `cargo tree --prefix depth -f "{p}"` output: `<depth><name> v<version> ...` per line.
pub fn parse_tree(text: &str) -> Vec<TreeEntry> {
    text.lines()
        .filter_map(|line| {
            let digits = line.chars().take_while(char::is_ascii_digit).count();
            if digits == 0 {
                return None;
            }
            let depth = line[..digits].parse().ok()?;
            let name = line[digits..].split_whitespace().next()?.to_owned();
            Some(TreeEntry { depth, name })
        })
        .collect()
}

/// The contract rule over its production tree.
pub fn contract_violations(tree: &[TreeEntry]) -> Vec<String> {
    let mut out = Vec::new();
    for e in tree {
        if e.depth == 1 && !CONTRACT_DIRECT.contains(&e.name.as_str()) {
            out.push(format!(
                "dereth-client-contract depends directly on `{}`; allowed: {}",
                e.name,
                CONTRACT_DIRECT.join(", ")
            ));
        }
        if e.name.starts_with("dereth-") && !CONTRACT_DERE.contains(&e.name.as_str()) {
            out.push(format!(
                "dereth-client-contract's tree contains `{}` (depth {})",
                e.name, e.depth
            ));
        }
        if CONTRACT_FORBIDDEN.contains(&e.name.as_str()) {
            out.push(format!(
                "dereth-client-contract's tree contains `{}` (depth {})",
                e.name, e.depth
            ));
        }
    }
    dedup(out)
}

/// The primitives rule over its production tree (default features).
pub fn primitives_violations(tree: &[TreeEntry]) -> Vec<String> {
    let mut out = Vec::new();
    for e in tree {
        if e.depth == 1 && !PRIMITIVES_DIRECT.contains(&e.name.as_str()) {
            out.push(format!(
                "dereth-primitives depends directly on `{}`; allowed: {}",
                e.name,
                PRIMITIVES_DIRECT.join(", ")
            ));
        }
        if e.depth > 0 && e.name.starts_with("dereth-") {
            out.push(format!(
                "dereth-primitives' tree contains `{}` (depth {})",
                e.name, e.depth
            ));
        }
        if PRIMITIVES_FORBIDDEN.contains(&e.name.as_str()) {
            out.push(format!(
                "dereth-primitives' tree contains `{}` (depth {})",
                e.name, e.depth
            ));
        }
    }
    dedup(out)
}

/// The primitives rule over its production code (`src/`, paths relative to the crate): no I/O
/// facility named, and `unsafe`/`extern` only in the NLS arm, whose declaration is feature-gated.
pub fn primitives_code_violations(files: &[(PathBuf, String)]) -> Vec<String> {
    let mut out = code_violations(files, PRIMITIVES_NO_IO);
    let unix = |p: &Path| p.to_string_lossy().replace('\\', "/");
    for (file, text) in files {
        let name = unix(file);
        if name.ends_with(PRIMITIVES_UNSAFE_FILE) {
            continue;
        }
        for (i, line) in text.lines().enumerate() {
            let code = strip_comment(line);
            let is_decl =
                name.ends_with("src/text/mod.rs") && code.trim() == "#[allow(unsafe_code)]";
            if !is_decl
                && (names_ident(code, "unsafe")
                    || names_ident(code, "unsafe_code")
                    || code.contains("extern \""))
            {
                out.push(format!(
                    "{}:{}: unsafe code outside {PRIMITIVES_UNSAFE_FILE}: {}",
                    file.display(),
                    i + 1,
                    line.trim()
                ));
            }
        }
        if name.ends_with("src/text/mod.rs")
            && !text.replace("\r\n", "\n").contains(PRIMITIVES_UNSAFE_DECL)
        {
            out.push(format!(
                "{}: `mod windows` must be declared exactly as `{}`",
                file.display(),
                PRIMITIVES_UNSAFE_DECL.replace('\n', " ")
            ));
        }
    }
    out
}

/// A forbidden-name rule over a crate's tree.
pub fn tree_violations(krate: &str, tree: &[TreeEntry], forbidden: &[&str]) -> Vec<String> {
    let out = tree
        .iter()
        .filter(|e| forbidden.contains(&e.name.as_str()))
        .map(|e| format!("{krate}'s tree contains `{}` (depth {})", e.name, e.depth))
        .collect();
    dedup(out)
}

/// Code lines (comments stripped) in `files` that name any of `paths`, e.g. `dereth_client_model`.
///
/// A name counts when it is a whole identifier: `dereth_ui` does not match inside `dereth_ui_screens`.
pub fn code_violations(files: &[(PathBuf, String)], paths: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    for (file, text) in files {
        for (i, line) in text.lines().enumerate() {
            let code = strip_comment(line);
            for p in paths {
                if names_ident(code, p) {
                    out.push(format!(
                        "{}:{}: names `{p}`: {}",
                        file.display(),
                        i + 1,
                        line.trim()
                    ));
                }
            }
        }
    }
    out
}

/// Code lines (comments stripped) in `files` that name a path in any of `crates`: the whole
/// identifier followed by `::`.
pub fn path_violations(files: &[(PathBuf, String)], crates: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    for (file, text) in files {
        for (i, line) in text.lines().enumerate() {
            let code = strip_comment(line);
            for c in crates {
                if names_path(code, c) {
                    out.push(format!(
                        "{}:{}: names `{c}::`: {}",
                        file.display(),
                        i + 1,
                        line.trim()
                    ));
                }
            }
        }
    }
    out
}

fn names_path(code: &str, krate: &str) -> bool {
    let is_ident = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let mut from = 0;
    while let Some(at) = code[from..].find(krate) {
        let start = from + at;
        let end = start + krate.len();
        let before = code[..start].chars().next_back();
        if !before.is_some_and(is_ident) && code[end..].starts_with("::") {
            return true;
        }
        from = end;
    }
    false
}

fn strip_comment(line: &str) -> &str {
    line.find("//").map_or(line, |i| &line[..i])
}

fn names_ident(code: &str, ident: &str) -> bool {
    let is_ident = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let mut from = 0;
    while let Some(at) = code[from..].find(ident) {
        let start = from + at;
        let end = start + ident.len();
        let before = code[..start].chars().next_back();
        let after = code[end..].chars().next();
        if !before.is_some_and(is_ident) && !after.is_some_and(is_ident) {
            return true;
        }
        from = end;
    }
    false
}

fn dedup(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v.dedup();
    v
}

fn cargo_tree(ws: &Path, krate: &str, edges: &str) -> Result<Vec<TreeEntry>, String> {
    let out = Command::new("cargo")
        .args([
            "tree",
            "-e",
            edges,
            "-p",
            krate,
            "--prefix",
            "depth",
            "-f",
            "{p}",
            "--offline",
        ])
        .current_dir(ws)
        .output()
        .map_err(|e| format!("failed to launch cargo tree: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "cargo tree -p {krate} failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(parse_tree(&String::from_utf8_lossy(&out.stdout)))
}

/// The packages under `dereth/client/crates/`, by their manifests' names, sorted.
fn client_crates(ws: &Path) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(ws.join("dereth/client/crates"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| std::fs::read_to_string(e.path().join("Cargo.toml")).ok())
        .filter_map(|m| {
            m.lines()
                .find_map(|l| l.trim().strip_prefix("name = \"")?.strip_suffix('"'))
                .map(str::to_owned)
        })
        .collect();
    out.sort();
    out
}

/// The package names of the crates directly under `dir` (relative to the workspace), sorted:
/// each one's `name = "..."` from its manifest.
fn crates_under(ws: &Path, dir: &str) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(ws.join(dir))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| std::fs::read_to_string(e.path().join("Cargo.toml")).ok())
        .filter_map(|m| {
            m.lines()
                .find_map(|l| l.trim().strip_prefix("name = \"")?.strip_suffix('"'))
                .map(str::to_owned)
        })
        .collect();
    out.sort();
    out
}

/// The library rule over each library crate's production tree (`trees`, one per crate): none of
/// [`LIBRARY_NO_SUBSCRIBER`].
pub fn library_log_violations(trees: &[(String, Vec<TreeEntry>)]) -> Vec<String> {
    let mut out = Vec::new();
    for (krate, tree) in trees {
        out.extend(tree_violations(krate, tree, LIBRARY_NO_SUBSCRIBER));
    }
    out
}

/// The runtime rule over its production tree: none of [`CORE_FORBIDDEN`], and none of
/// `client_crates` (the packages under `dereth/client/crates/`).
pub fn core_violations(tree: &[TreeEntry], client_crates: &[String]) -> Vec<String> {
    let mut forbidden: Vec<&str> = CORE_FORBIDDEN.to_vec();
    forbidden.extend(client_crates.iter().map(String::as_str));
    tree_violations("dereth-client-runtime", tree, &forbidden)
}

/// The headless rule over its whole tree (every dependency kind): its direct `dereth-*`
/// dependencies are exactly [`HEADLESS_DERE`], and neither `dereth-client` nor any crate under
/// `dereth/client/crates/` (`client_crates`) appears at any depth.
pub fn headless_violations(tree: &[TreeEntry], client_crates: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for e in tree {
        if e.depth == 1
            && e.name.starts_with("dereth-")
            && !HEADLESS_DERE.contains(&e.name.as_str())
        {
            out.push(format!(
                "dereth-headless depends directly on `{}`; allowed: {}",
                e.name,
                HEADLESS_DERE.join(", ")
            ));
        }
    }
    for want in HEADLESS_DERE {
        if !tree.iter().any(|e| e.depth == 1 && e.name == *want) {
            out.push(format!("dereth-headless does not depend on `{want}`"));
        }
    }
    let mut forbidden: Vec<&str> = vec!["dereth-client"];
    forbidden.extend(client_crates.iter().map(String::as_str));
    out.extend(tree_violations("dereth-headless", tree, &forbidden));
    dedup(out)
}

/// The SDK rule over its whole tree (every dependency kind): neither `dereth-client` nor any crate
/// under `dereth/client/crates/` (`client_crates`) -- the device input among them -- appears at
/// any depth.
pub fn sdk_violations(tree: &[TreeEntry], client_crates: &[String]) -> Vec<String> {
    let mut forbidden: Vec<&str> = vec!["dereth-client"];
    forbidden.extend(client_crates.iter().map(String::as_str));
    tree_violations("dereth-client-sdk", tree, &forbidden)
}

/// The client-crates rule: every crate in `crates` has none of [`PRESENTATION_FORBIDDEN`] in its
/// tree (`trees`, one per crate, every dependency kind), and each of
/// [`PRESENTATION_ON_CONTRACT`] among them depends directly on `dereth-client-contract`.
pub fn presentation_violations(trees: &[(String, Vec<TreeEntry>)]) -> Vec<String> {
    let mut out = Vec::new();
    for (krate, tree) in trees {
        if !APPLICATION_HALVES.contains(&krate.as_str()) {
            out.extend(tree_violations(krate, tree, PRESENTATION_FORBIDDEN));
        }
        let on_contract = tree
            .iter()
            .any(|e| e.depth == 1 && e.name == "dereth-client-contract");
        if PRESENTATION_ON_CONTRACT.contains(&krate.as_str()) && !on_contract {
            out.push(format!(
                "{krate} does not depend directly on dereth-client-contract"
            ));
        }
    }
    dedup(out)
}

/// Rule 9 over the halves' trees (`trees`, one per half, every dependency kind): no direct
/// dependency on any of [`HALVES_FORBIDDEN`], and the scene not on the shell at any depth.
pub fn halves_violations(trees: &[(String, Vec<TreeEntry>)]) -> Vec<String> {
    let mut out = Vec::new();
    for (krate, tree) in trees {
        for e in tree {
            if e.depth == 1 && HALVES_FORBIDDEN.contains(&e.name.as_str()) {
                out.push(format!("{krate} depends directly on `{}`", e.name));
            }
        }
        if krate == "dereth-scene" {
            out.extend(tree_violations(krate, tree, &["dereth-client-shell"]));
        }
    }
    dedup(out)
}

fn crate_sources(ws: &Path, krate: &str) -> Vec<(PathBuf, String)> {
    let root = crate::util::crate_dir(ws, krate);
    let mut files = Vec::new();
    for sub in ["src", "tests", "benches", "examples"] {
        files.extend(crate::util::rust_sources(&root.join(sub)));
    }
    files.sort();
    files
        .into_iter()
        .filter_map(|f| {
            let text = std::fs::read_to_string(&f).ok()?;
            let rel = f.strip_prefix(ws).map(Path::to_path_buf).unwrap_or(f);
            Some((rel, text))
        })
        .collect()
}

/// A crate's production sources only (`src/`), for rules its tests are exempt from.
/// Whether `path` (relative to the workspace) is the retail front end's assembly.
fn is_front_end_assembly(path: &Path) -> bool {
    path.to_string_lossy()
        .replace('\\', "/")
        .ends_with(&format!("dereth/client/crates/shell/{FRONT_END_ASSEMBLY}"))
}

fn crate_src(ws: &Path, krate: &str) -> Vec<(PathBuf, String)> {
    crate_sources(ws, krate)
        .into_iter()
        .filter(|(p, _)| p.components().any(|c| c.as_os_str() == "src"))
        .collect()
}

/// The retired dat-directory variable, spelled in two halves so this file does not name it.
const RETIRED_DAT_VAR: &str = concat!("AC_", "CLIENT_DIR");

/// The retired variable prefix, likewise in two halves: every variable the tests and tools read
/// is `DERETH_*` (or `EMPYREAN_*`).
const RETIRED_VAR_PREFIX: &str = concat!("DERE", "_");

/// Rule 8's first half: the code (comments stripped) of any workspace file outside `core/dat/`
/// that joins a retail dat file name onto a path. Where the dats are, and what the files are
/// called on disk, is `dereth-dat`'s business (`locate_modern_dats`, `ModernDat::in_dir`, and
/// `dereth_dat::testing` for the tests); plain name strings -- a table of file names, a message,
/// a retail string -- are not paths and are not reported.
#[must_use]
pub fn dat_path_violations(files: &[(PathBuf, String)]) -> Vec<String> {
    let join = regex::Regex::new(
        r#"(?:\.join|Path::new|PathBuf::from)\(\s*"client_[A-Za-z0-9_]*\.dat"|format!\(\s*"[^"]*[/\\]client_[A-Za-z0-9_]*\.dat""#,
    )
    .expect("the pattern compiles");
    let mut out = Vec::new();
    for (path, text) in files {
        let norm = path.to_string_lossy().replace('\\', "/");
        if norm.starts_with("core/dat/") || norm.contains("/core/dat/") {
            continue;
        }
        let code: String = text
            .lines()
            .map(strip_comment)
            .collect::<Vec<_>>()
            .join("\n");
        for m in join.find_iter(&code) {
            let line = code[..m.start()].matches('\n').count() + 1;
            out.push(format!(
                "{}:{line}: joins a retail dat file name onto a path (use dereth_dat::ModernDat or dereth_dat::testing)",
                path.display()
            ));
        }
    }
    dedup(out)
}

/// Rule 8's second half: any tracked workspace file naming the retired dat-directory
/// variable or a variable with the retired prefix. Comments count: a doc that tells a reader to
/// set a variable nothing reads is as wrong as code that reads it.
#[must_use]
pub fn env_name_violations(files: &[(PathBuf, String)]) -> Vec<String> {
    let names = regex::Regex::new(&format!(
        r"{RETIRED_DAT_VAR}|\b{RETIRED_VAR_PREFIX}[A-Z0-9]"
    ))
    .expect("the pattern compiles");
    let mut out = Vec::new();
    for (path, text) in files {
        for (i, line) in text.lines().enumerate() {
            if let Some(m) = names.find(line) {
                out.push(format!(
                    "{}:{}: names the retired variable `{}...`",
                    path.display(),
                    i + 1,
                    &line[m.start()..m.end()]
                ));
            }
        }
    }
    out
}

/// The files git tracks under `specs` (paths relative to `repo`), as `git ls-files -z` lists them.
///
/// # Errors
/// git could not be run, or `repo` is not a work tree.
fn git_ls_files(repo: &Path, specs: &[&Path]) -> Result<Vec<u8>, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["ls-files", "-z", "--"])
        .args(specs)
        .output()
        .map_err(|e| format!("git ls-files: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "git ls-files in {}: {}",
            repo.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(out.stdout)
}

/// The text of every file a `git ls-files -z` listing names, relative to `repo`: a listed file
/// that is not there (deleted in the work tree), larger than 8 MiB or not UTF-8 is skipped, and a
/// file the listing does not name is never read, so an untracked local file is not the tree's.
fn listed_texts(repo: &Path, listing: &[u8]) -> Vec<(PathBuf, String)> {
    let mut out = Vec::new();
    for name in listing.split(|b| *b == 0).filter(|n| !n.is_empty()) {
        let rel = PathBuf::from(String::from_utf8_lossy(name).into_owned());
        let p = repo.join(&rel);
        if !std::fs::metadata(&p).is_ok_and(|m| m.is_file() && m.len() <= 8 << 20) {
            continue;
        }
        if let Ok(text) = std::fs::read_to_string(&p) {
            out.push((rel, text));
        }
    }
    out
}

/// Every tracked text file under `dirs` (relative to `repo`).
///
/// # Errors
/// As [`git_ls_files`].
fn tracked_texts(repo: &Path, dirs: &[&Path]) -> Result<Vec<(PathBuf, String)>, String> {
    Ok(listed_texts(repo, &git_ls_files(repo, dirs)?))
}

/// Rule 11: the optional high-fidelity presentation.
pub const HIFI: &str = "dereth-render-hifi";

/// The workspace crates the presentation's tree may hold, at any depth, with every feature on.
pub const HIFI_TREE_DERE: &[&str] = &[
    "dereth-render-hifi",
    "dereth-render",
    "dereth-render-cpu",
    "dereth-primitives",
    "dereth-client-contract",
];

/// The crates from outside the workspace its manifest may name as a dependency of its own.
pub const HIFI_DIRECT_EXTERNAL: &[&str] = &["wgpu", "glam", "bytemuck", "naga", "sha2"];

/// What its manifest may name for its tests alone, besides: the landscape crate, to check a hash
/// against the landscape's own.
pub const HIFI_DEV_EXTRA: &[&str] = &["dereth-terrain"];

/// Platform crates the presentation's tree never holds.
pub const HIFI_TREE_FORBIDDEN: &[&str] = &["winit", "cpal"];

/// What the presentation's code (comments stripped, its tests and build script included) never
/// names: the simulation, the world, the network, the scene and the game types a picture has no
/// business with.
pub const HIFI_NAMES_NOT: &[&str] = &[
    "dereth_physics",
    "dereth_animation",
    "dereth_world_data",
    "dereth_protocol",
    "dereth_transport",
    "dereth_rules",
    "dereth_scene",
    "WorldState",
    "PhysicsWorld",
    "MotionDriver",
    "PickScene",
    "UiRequest",
];

/// Prefixes the presentation's code never names an identifier with: every client crate past the
/// device (`dereth_client_runtime`, `dereth_client_model`, `dereth_client_net`, ...) and every
/// server crate.
pub const HIFI_PREFIXES_NOT: &[&str] = &["dereth_client_", "empyrean_"];

/// What the presentation's production code (`src/`) never names; its tests may, to check a hash
/// against the landscape's own.
pub const HIFI_SRC_NAMES_NOT: &[&str] = &["dereth_terrain"];

/// The presentation's only dependent.
pub const HIFI_DEPENDENTS: &[&str] = &["dereth-scene"];

/// The one file that names the presentation's crate: the scene's bridge to it.
pub const HIFI_BRIDGE: &str = "dereth/client/crates/scene/src/world_scene/hifi_bridge.rs";

/// What the bridge never names: anything that writes the scene or the simulation or advances
/// their state, and every way of writing through a shared reference.
pub const BRIDGE_NAMES_NOT: &[&str] = &[
    "borrow_mut",
    "get_mut",
    "iter_mut",
    "values_mut",
    "driver_mut",
    "terrain_height_at",
    "sweep_sphere",
    "generate_scenery",
    "apply_lighting",
    "apply_fog",
    "relight_blocks",
    "tick_schedule",
    "calc_water_depth",
    "ran2",
    "Cell",
    "RefCell",
    "UnsafeCell",
    "OnceCell",
    "Mutex",
    "RwLock",
    "as_ptr",
    "replace_with",
    "unsafe",
    "transmute",
];

/// Prefixes the bridge never names an identifier with.
pub const BRIDGE_PREFIXES_NOT: &[&str] = &["take_", "set_", "Atomic"];

/// Calls the bridge never makes (matched with the whitespace taken out): the standard library's
/// writes through a shared reference, and its swaps and takes. The bridge's own memory is handed
/// to it as `&mut`, so it needs none of them.
pub const BRIDGE_CALLS_NOT: &[&str] = &[
    ".take(",
    ".replace(",
    ".swap(",
    ".set(",
    ".update(",
    ".lock(",
    ".write(",
    ".store(",
    ".fetch_",
    ".compare_exchange",
    "mem::take(",
    "mem::replace(",
    "mem::swap(",
    "ptr::",
];

/// The receivers the bridge reads the frame from: the scene's drawing half and the world.
pub const BRIDGE_RECEIVERS: &[&str] = &["draw", "ws"];

/// The methods the bridge may call on [`BRIDGE_RECEIVERS`] directly, each reviewed to read only.
/// A call to any other is a violation until it is reviewed and listed.
pub const BRIDGE_READS: &[&str] = &[
    "view_params",
    "block_shift",
    "world_fog_state",
    "sky_region",
    "weather_enabled",
    "light_pools",
    "viewer_cell",
];

/// This checker's own source.
const SEAMS_SELF: &str = "tools/xtask/src/seams.rs";

/// The device's seam to the presentation.
pub const HIFI_SEAM: &str = "dereth/client/crates/render/src/wgpu/sidecar.rs";

/// The device's own state the seam writes only on the lines of [`SEAM_WRITES_ALLOWED`].
pub const SEAM_FIELDS_NOT_WRITTEN: &[&str] = &[
    "commands",
    "pipelines",
    "pipeline_index",
    "textures",
    "texture_book",
    "depth",
    "draw_calls",
    "frame_stamp",
    "uniform_arena",
    "vertex_arena",
];

/// The methods the seam may call on one of [`SEAM_FIELDS_NOT_WRITTEN`]: each only reads. Any
/// other method called on one of them counts as a write.
pub const SEAM_FIELD_READS: &[&str] = &[
    "borrow",
    "len",
    "is_empty",
    "iter",
    "get",
    "as_ref",
    "as_slice",
    "contains",
    "contains_key",
    "keys",
    "values",
    "first",
    "last",
    "clone",
    "to_vec",
    "chunks",
    "windows",
];

/// The seam's writes to the device's state, each the exact line and how many times it may stand:
/// a presented frame is counted, as the ordinary end of a frame counts it; the constant arena is
/// lent to the presentation's context, which appends blocks after the recorded ones and nothing
/// else, and is cut back to the recorded length once the frame is submitted.
///
/// Not covered: a write made inside one of the device's own methods the seam calls (the depth
/// target and sampler caches it fills on first use, the arena uploads). Those are the plain
/// frame's own calls, made the same way; this rule sees the seam's text only.
pub const SEAM_WRITES_ALLOWED: &[(&str, usize)] = &[
    ("self.frame_stamp += 1;", 1),
    ("uniform_arena: &mut self.uniform_arena,", 2),
    ("self.uniform_arena.resize(end, 0);", 1),
    (
        "self.uniform_arena[start..start + bytes.len()].copy_from_slice(bytes);",
        1,
    ),
    ("self.uniform_arena.truncate(recorded_uniform_len);", 1),
];

/// The clients that never carry the presentation, each package built on its own, as a release
/// builds it: the browser client and the headless one. The desktop client carries it by design.
pub const SHIPPED_CLIENTS: &[&str] = &["dereth-web", "dereth-headless"];

/// What a shipped client's feature tree never holds: the presentation, or any crate's `hifi`
/// feature.
pub const SHIPPED_NOT: &[&str] = &[
    "dereth-render-hifi",
    "dereth-render feature \"hifi\"",
    "dereth-scene feature \"hifi\"",
    "dereth-client-shell feature \"hifi\"",
    "dereth-client feature \"hifi\"",
    "dereth-horizon feature \"hifi\"",
    "dereth-client-runtime feature \"hifi\"",
    "dereth-client-contract feature \"hifi\"",
];

/// `text` with every comment (line, block and doc) blanked out and every newline kept, so line
/// numbers still match. String and character literals are kept whole, so a `//` inside a string
/// does not hide the code after it.
pub fn blank_comments(text: &str) -> String {
    let b: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let blank = |c: char| if c == '\n' { '\n' } else { ' ' };
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        let next = b.get(i + 1).copied();
        if c == '/' && next == Some('/') {
            while i < b.len() && b[i] != '\n' {
                out.push(' ');
                i += 1;
            }
            continue;
        }
        if c == '/' && next == Some('*') {
            let mut depth = 0usize;
            while i < b.len() {
                if b[i] == '/' && b.get(i + 1) == Some(&'*') {
                    depth += 1;
                    out.push_str("  ");
                    i += 2;
                } else if b[i] == '*' && b.get(i + 1) == Some(&'/') {
                    depth -= 1;
                    out.push_str("  ");
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    out.push(blank(b[i]));
                    i += 1;
                }
            }
            continue;
        }
        let ident_before = i > 0 && (b[i - 1].is_ascii_alphanumeric() || b[i - 1] == '_');
        // A raw string: `r"..."`, `r#"..."#`, `br"..."`.
        if c == 'r' && !ident_before || (c == 'b' && next == Some('r') && !ident_before) {
            let start = i;
            let mut j = i + if c == 'b' { 2 } else { 1 };
            let mut hashes = 0;
            while b.get(j) == Some(&'#') {
                hashes += 1;
                j += 1;
            }
            if b.get(j) == Some(&'"') {
                j += 1;
                loop {
                    match b.get(j) {
                        None => break,
                        Some('"') if (1..=hashes).all(|h| b.get(j + h) == Some(&'#')) => {
                            j += 1 + hashes;
                            break;
                        }
                        Some(_) => j += 1,
                    }
                }
                out.extend(&b[start..j.min(b.len())]);
                i = j;
                continue;
            }
        }
        if c == '"' {
            out.push(c);
            i += 1;
            while i < b.len() {
                out.push(b[i]);
                if b[i] == '\\' {
                    if let Some(&e) = b.get(i + 1) {
                        out.push(e);
                    }
                    i += 2;
                    continue;
                }
                i += 1;
                if b[i - 1] == '"' {
                    break;
                }
            }
            continue;
        }
        // A character literal (`'"'`, `'\''`); a lifetime is left alone.
        if c == '\'' {
            let len = match (next, b.get(i + 2)) {
                (Some('\\'), _) => b[i + 2..].iter().position(|&x| x == '\'').map(|p| p + 3),
                (Some(_), Some('\'')) => Some(3),
                _ => None,
            };
            if let Some(len) = len {
                let end = (i + len).min(b.len());
                out.extend(&b[i..end]);
                i = end;
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

/// Whether `code` names an identifier starting with `prefix`.
fn names_prefix(code: &str, prefix: &str) -> bool {
    let is_ident = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let mut from = 0;
    while let Some(at) = code[from..].find(prefix) {
        let start = from + at;
        let before = code[..start].chars().next_back();
        if !before.is_some_and(is_ident) {
            return true;
        }
        from = start + prefix.len();
    }
    false
}

/// The identifier at the start of `s`.
fn leading_ident(s: &str) -> &str {
    let end = s
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .unwrap_or(s.len());
    &s[..end]
}

/// Whether a workspace crate's name is one of `members`, or looks like a workspace crate's, so a
/// listing that failed to name one still cannot let it through.
fn is_workspace_crate(name: &str, members: &[String]) -> bool {
    members.iter().any(|m| m == name)
        || name.starts_with("dereth-")
        || name.starts_with("empyrean-")
}

/// Rule 11 over the presentation's tree (every feature, every target, normal and build edges):
/// no workspace crate outside [`HIFI_TREE_DERE`] (`members` names the workspace's crates), and
/// none of [`HIFI_TREE_FORBIDDEN`].
pub fn hifi_tree_violations(tree: &[TreeEntry], members: &[String]) -> Vec<String> {
    let mut out = tree_violations(HIFI, tree, HIFI_TREE_FORBIDDEN);
    for e in tree {
        if is_workspace_crate(&e.name, members) && !HIFI_TREE_DERE.contains(&e.name.as_str()) {
            out.push(format!(
                "{HIFI}'s tree contains `{}` (depth {}); allowed: {}",
                e.name,
                e.depth,
                HIFI_TREE_DERE.join(", ")
            ));
        }
    }
    dedup(out)
}

/// One dependency a manifest declares: its package name, and whether it is for tests alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestDep {
    pub name: String,
    pub dev: bool,
}

/// Rule 11 over what the presentation's manifest declares, whatever the features: only
/// [`HIFI_TREE_DERE`] and [`HIFI_DIRECT_EXTERNAL`], and for its tests [`HIFI_DEV_EXTRA`] too.
pub fn hifi_manifest_violations(deps: &[ManifestDep]) -> Vec<String> {
    let out = deps
        .iter()
        .filter(|d| {
            let n = d.name.as_str();
            !(HIFI_TREE_DERE.contains(&n)
                || HIFI_DIRECT_EXTERNAL.contains(&n)
                || (d.dev && HIFI_DEV_EXTRA.contains(&n)))
        })
        .map(|d| {
            format!(
                "{HIFI} declares a{} dependency on `{}`; allowed: {}, {}{}",
                if d.dev { " test" } else { "" },
                d.name,
                HIFI_TREE_DERE.join(", "),
                HIFI_DIRECT_EXTERNAL.join(", "),
                if d.dev {
                    format!(", {}", HIFI_DEV_EXTRA.join(", "))
                } else {
                    String::new()
                }
            )
        })
        .collect();
    dedup(out)
}

/// Rule 11 over the presentation's code (`files` relative to the workspace): none of
/// [`HIFI_NAMES_NOT`] or [`HIFI_PREFIXES_NOT`] anywhere, and none of [`HIFI_SRC_NAMES_NOT`] in
/// `src/` or the build script. Comments are blanked first and strings kept, so neither hides a
/// name.
pub fn hifi_code_violations(files: &[(PathBuf, String)]) -> Vec<String> {
    let mut out = Vec::new();
    for (file, text) in files {
        let in_src = file.components().any(|c| c.as_os_str() == "src")
            || file.file_name().is_some_and(|n| n == "build.rs");
        let code_text = blank_comments(text);
        for (i, (code, line)) in code_text.lines().zip(text.lines()).enumerate() {
            let names = HIFI_NAMES_NOT
                .iter()
                .chain(HIFI_SRC_NAMES_NOT.iter().filter(|_| in_src));
            for n in names {
                if names_ident(code, n) {
                    out.push(format!(
                        "{}:{}: names `{n}`: {}",
                        file.display(),
                        i + 1,
                        line.trim()
                    ));
                }
            }
            for p in HIFI_PREFIXES_NOT {
                if names_prefix(code, p) {
                    out.push(format!(
                        "{}:{}: names `{p}...`: {}",
                        file.display(),
                        i + 1,
                        line.trim()
                    ));
                }
            }
        }
    }
    out
}

/// Rule 11 over the presentation's direct dependents: exactly [`HIFI_DEPENDENTS`].
pub fn hifi_dependent_violations(dependents: &[TreeEntry]) -> Vec<String> {
    let out = dependents
        .iter()
        .filter(|e| e.depth == 1 && !HIFI_DEPENDENTS.contains(&e.name.as_str()))
        .map(|e| {
            format!(
                "`{}` depends on {HIFI}; only {} may",
                e.name,
                HIFI_DEPENDENTS.join(", ")
            )
        })
        .collect();
    dedup(out)
}

/// The methods called directly on `receiver` in `squeezed` (code with the whitespace taken out):
/// each `receiver.name(` where `receiver` is a whole identifier.
fn direct_calls<'a>(squeezed: &'a str, receiver: &str) -> Vec<&'a str> {
    let is_ident = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let needle = format!("{receiver}.");
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(at) = squeezed[from..].find(&needle) {
        let start = from + at;
        let after = start + needle.len();
        from = after;
        if squeezed[..start].chars().next_back().is_some_and(is_ident) {
            continue;
        }
        let name = leading_ident(&squeezed[after..]);
        if !name.is_empty() && squeezed[after + name.len()..].starts_with('(') {
            out.push(name);
        }
    }
    out
}

/// Rule 11 over the workspace's Rust files (`files` relative to the workspace, the presentation's
/// own excluded): only [`HIFI_BRIDGE`] names the presentation's crate; the bridge names none of
/// [`BRIDGE_NAMES_NOT`] or [`BRIDGE_PREFIXES_NOT`], makes none of [`BRIDGE_CALLS_NOT`], and calls
/// nothing on the scene or the world but [`BRIDGE_READS`].
pub fn hifi_bridge_violations(files: &[(PathBuf, String)]) -> Vec<String> {
    let unix = |p: &Path| p.to_string_lossy().replace('\\', "/");
    let mut out = Vec::new();
    for (file, text) in files {
        let name = unix(file);
        // The presentation itself, and this checker, whose calibration plants every violation.
        if name.starts_with("dereth/client/crates/render-hifi/") || name == SEAMS_SELF {
            continue;
        }
        let bridge = name == HIFI_BRIDGE;
        let code_text = blank_comments(text);
        for (i, (code, line)) in code_text.lines().zip(text.lines()).enumerate() {
            let mut flag = |what: String| {
                out.push(format!(
                    "{}:{}: {what}: {}",
                    file.display(),
                    i + 1,
                    line.trim()
                ));
            };
            if !bridge {
                if names_ident(code, "dereth_render_hifi") {
                    flag(format!("names `dereth_render_hifi` outside {HIFI_BRIDGE}"));
                }
                continue;
            }
            for n in BRIDGE_NAMES_NOT {
                if names_ident(code, n) {
                    flag(format!("the bridge names `{n}`"));
                }
            }
            for p in BRIDGE_PREFIXES_NOT {
                if names_prefix(code, p) {
                    flag(format!("the bridge names `{p}...`"));
                }
            }
            let squeezed: String = code.chars().filter(|c| !c.is_whitespace()).collect();
            for c in BRIDGE_CALLS_NOT {
                if squeezed.contains(c) {
                    flag(format!("the bridge calls `{c}`"));
                }
            }
            for receiver in BRIDGE_RECEIVERS {
                for m in direct_calls(&squeezed, receiver) {
                    if !BRIDGE_READS.contains(&m) {
                        flag(format!(
                            "the bridge calls `{receiver}.{m}`, which is not one of the reviewed \
                             reads"
                        ));
                    }
                }
            }
        }
    }
    out
}

/// Whether the code after a protected field (`rest`, whitespace taken out) writes it: an
/// assignment, a compound assignment, a method not in [`SEAM_FIELD_READS`], or either after an
/// index.
fn writes_after_field(rest: &str) -> bool {
    let rest = if rest.starts_with('[') {
        let mut depth = 0usize;
        let mut end = rest.len();
        for (k, c) in rest.char_indices() {
            match c {
                '[' => depth += 1,
                ']' => {
                    depth -= 1;
                    if depth == 0 {
                        end = k + 1;
                        break;
                    }
                }
                _ => {}
            }
        }
        &rest[end..]
    } else {
        rest
    };
    if rest.starts_with('=') && !rest.starts_with("==") {
        return true;
    }
    if ["+=", "-=", "*=", "/=", "|=", "&=", "^=", "<<=", ">>="]
        .iter()
        .any(|op| rest.starts_with(op))
    {
        return true;
    }
    if let Some(after_dot) = rest.strip_prefix('.') {
        let m = leading_ident(after_dot);
        if !m.is_empty() && after_dot[m.len()..].starts_with('(') {
            return !SEAM_FIELD_READS.contains(&m);
        }
    }
    false
}

/// Rule 11 over the device's seam (`text`, [`HIFI_SEAM`]'s contents): no write to any of
/// [`SEAM_FIELDS_NOT_WRITTEN`] (an assignment, a method that is not a read, or `&mut` on one),
/// but on the lines of [`SEAM_WRITES_ALLOWED`], each at most as often as it is allowed.
pub fn hifi_seam_violations(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut used = vec![0usize; SEAM_WRITES_ALLOWED.len()];
    let code_text = blank_comments(text);
    for (i, (code, line)) in code_text.lines().zip(text.lines()).enumerate() {
        if let Some(k) = SEAM_WRITES_ALLOWED
            .iter()
            .position(|(allowed, _)| code.trim() == *allowed)
        {
            used[k] += 1;
            if used[k] <= SEAM_WRITES_ALLOWED[k].1 {
                continue;
            }
        }
        let squeezed: String = code.chars().filter(|c| !c.is_whitespace()).collect();
        for f in SEAM_FIELDS_NOT_WRITTEN {
            let field = format!("self.{f}");
            let mut from = 0;
            while let Some(at) = squeezed[from..].find(&field) {
                let start = from + at;
                let end = start + field.len();
                from = end;
                let rest = &squeezed[end..];
                let next_ident = rest
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_');
                if next_ident {
                    continue;
                }
                if writes_after_field(rest) || squeezed[..start].ends_with("&mut") {
                    out.push(format!(
                        "{HIFI_SEAM}:{}: writes the device's `{f}`: {}",
                        i + 1,
                        line.trim()
                    ));
                }
            }
        }
    }
    out
}

/// Rule 11 over one shipped client's feature tree (`lines`, `cargo tree -e features` for
/// `package` alone with no prefix): none of [`SHIPPED_NOT`].
pub fn shipped_client_violations(package: &str, lines: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in lines.lines() {
        let line = line.trim();
        for n in SHIPPED_NOT {
            let hit = if n.contains(' ') {
                line.starts_with(n)
            } else {
                line.split_whitespace().next() == Some(n)
            };
            if hit {
                out.push(format!(
                    "`{package}`, built on its own, holds the high-fidelity presentation: {line}"
                ));
            }
        }
    }
    dedup(out)
}

/// The workspace's crate names, and the dependencies `krate`'s manifest declares, from
/// `cargo metadata` (no resolution, so every optional dependency and every target is listed).
fn workspace_manifest(ws: &Path, krate: &str) -> Result<(Vec<String>, Vec<ManifestDep>), String> {
    let out = Command::new("cargo")
        .args([
            "metadata",
            "--no-deps",
            "--format-version",
            "1",
            "--offline",
        ])
        .current_dir(ws)
        .output()
        .map_err(|e| format!("failed to launch cargo metadata: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let meta: serde_json::Value =
        serde_json::from_slice(&out.stdout).map_err(|e| format!("cargo metadata: {e}"))?;
    let packages = meta["packages"]
        .as_array()
        .ok_or("cargo metadata: no packages")?;
    let members: Vec<String> = packages
        .iter()
        .filter_map(|p| p["name"].as_str().map(str::to_owned))
        .collect();
    let deps = packages
        .iter()
        .find(|p| p["name"].as_str() == Some(krate))
        .ok_or_else(|| format!("cargo metadata: no package {krate}"))?["dependencies"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|d| {
            Some(ManifestDep {
                name: d["name"].as_str()?.to_owned(),
                dev: d["kind"].as_str() == Some("dev"),
            })
        })
        .collect();
    Ok((members, deps))
}

/// `cargo tree -e features` for `package` alone, as a release builds it (its normal and build
/// dependencies, not its tests'), as lines with no prefix.
fn feature_tree(ws: &Path, package: &str) -> Result<String, String> {
    let out = Command::new("cargo")
        .args(["tree", "-e", "features,normal,build", "-p", package])
        .args([
            "--target",
            "all",
            "--prefix",
            "none",
            "-f",
            "{p}",
            "--offline",
        ])
        .current_dir(ws)
        .output()
        .map_err(|e| format!("failed to launch cargo tree: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "cargo tree -e features -p {package} failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// `cargo tree` with `args` before the output format.
fn cargo_tree_args(ws: &Path, args: &[&str]) -> Result<Vec<TreeEntry>, String> {
    let out = Command::new("cargo")
        .arg("tree")
        .args(args)
        .args(["--prefix", "depth", "-f", "{p}", "--offline"])
        .current_dir(ws)
        .output()
        .map_err(|e| format!("failed to launch cargo tree: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "cargo tree {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(parse_tree(&String::from_utf8_lossy(&out.stdout)))
}

/// Every seam check, as rows for a table.
pub fn reports() -> Vec<Report> {
    let ws = workspace_root();
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
            Report::new(name, Outcome::Fail, "could not read the dependency tree")
        }
    };

    let primitives =
        cargo_tree(&ws, "dereth-primitives", "normal").map(|t| primitives_violations(&t));
    let primitives_code = Ok(primitives_code_violations(&crate_src(
        &ws,
        "dereth-primitives",
    )));
    let contract =
        cargo_tree(&ws, "dereth-client-contract", "normal").map(|t| contract_violations(&t));
    let screens_tree = cargo_tree(&ws, "dereth-ui-screens", "normal,build,dev")
        .map(|t| tree_violations("dereth-ui-screens", &t, SCREENS_FORBIDDEN));
    let screens_code = Ok(code_violations(
        &crate_sources(&ws, "dereth-ui-screens"),
        &[
            "dereth_client_model",
            "dereth_client_runtime",
            "dereth_client",
        ],
    ));
    let crates = client_crates(&ws);
    let core_tree =
        cargo_tree(&ws, "dereth-client-runtime", "normal").map(|t| core_violations(&t, &crates));
    let core_code = Ok(code_violations(
        &crate_sources(&ws, "dereth-client-runtime"),
        CORE_NAMES_NOT,
    ));
    let presentation = if crates.len() < PRESENTATION_ON_CONTRACT.len() {
        Err(format!(
            "found {} crate(s) under dereth/client/crates/; expected at least {}",
            crates.len(),
            PRESENTATION_ON_CONTRACT.len()
        ))
    } else {
        crates
            .iter()
            .map(|k| cargo_tree(&ws, k, "normal,build,dev").map(|t| (k.clone(), t)))
            .collect::<Result<Vec<_>, _>>()
            .map(|trees| presentation_violations(&trees))
    };
    let headless = cargo_tree(&ws, "dereth-headless", "normal,build,dev")
        .map(|t| headless_violations(&t, &crates));
    let sdk = cargo_tree(&ws, "dereth-client-sdk", "normal,build,dev")
        .map(|t| sdk_violations(&t, &crates));
    let action_seam_files: Vec<(PathBuf, String)> = ACTION_SEAM_CRATES
        .iter()
        .flat_map(|k| crate_sources(&ws, k))
        .collect();
    // A crate whose sources were not found would pass vacuously; each must contribute files.
    let action_seam_code = match ACTION_SEAM_CRATES
        .iter()
        .find(|k| crate_sources(&ws, k).is_empty())
    {
        Some(k) => Err(format!("no sources found for {k}")),
        None => Ok(code_violations(&action_seam_files, DEVICE_NAMES_NOT)),
    };
    // Every library crate logs through the facade and installs nothing: core/*, and the client's
    // own crates under dereth/client/crates/.
    let libraries: Vec<String> = crates_under(&ws, "core")
        .into_iter()
        .chain(crates_under(&ws, "dereth/client/crates"))
        .collect();
    let library_log = if libraries.len() < 16 {
        Err(format!(
            "found {} library crate(s) under core/ and dereth/client/crates/; expected at least 16",
            libraries.len()
        ))
    } else {
        libraries
            .iter()
            .map(|k| cargo_tree(&ws, k, "normal").map(|t| (k.clone(), t)))
            .collect::<Result<Vec<_>, _>>()
            .map(|trees| library_log_violations(&trees))
    };
    // The host reaches the gameplay screen through `Screen::on_game` and the screen crate's call
    // vocabulary, never by naming (and downcasting to) the concrete screen. Tests may.
    let host_screen = Ok(code_violations(
        &["dereth-client", "dereth-client-shell"]
            .iter()
            .flat_map(|k| crate_src(&ws, k))
            .collect::<Vec<_>>(),
        &HOST_NAMES_NO_SCREEN,
    ));
    // Rule 9: the application's halves reach no platform.
    let halves_tree = APPLICATION_HALVES
        .iter()
        .map(|k| cargo_tree(&ws, k, "normal,build,dev").map(|t| ((*k).to_owned(), t)))
        .collect::<Result<Vec<_>, _>>()
        .map(|trees| halves_violations(&trees));
    let halves_code = match APPLICATION_HALVES
        .iter()
        .find(|k| crate_sources(&ws, k).is_empty())
    {
        Some(k) => Err(format!("no sources found for {k}")),
        None => Ok(path_violations(
            &APPLICATION_HALVES
                .iter()
                .flat_map(|k| crate_sources(&ws, k))
                .collect::<Vec<_>>(),
            HALVES_NAMES_NOT,
        )),
    };
    // Rule 10: every front end reaches the game through its context.
    let front_end = FRONT_ENDS
        .iter()
        .map(|krate| {
            let files: Vec<(PathBuf, String)> = crate_src(&ws, krate)
                .into_iter()
                .filter(|(p, _)| !is_front_end_assembly(p))
                .collect();
            if files.is_empty() {
                Err(format!("no sources found for {krate}"))
            } else {
                Ok(code_violations(&files, FRONT_END_NAMES_NOT))
            }
        })
        .try_fold(Vec::new(), |mut all, one| {
            all.extend(one?);
            Ok::<_, String>(all)
        });
    // Rule 8: every Rust file of the workspace, and every tracked text file in it.
    let workspace_rs: Vec<(PathBuf, String)> = crate::util::rust_sources(&ws)
        .into_iter()
        .filter_map(|p| {
            let text = std::fs::read_to_string(&p).ok()?;
            Some((
                p.strip_prefix(&ws).map(Path::to_path_buf).unwrap_or(p),
                text,
            ))
        })
        .collect();
    let dat_paths = if workspace_rs.len() < 100 {
        Err(format!(
            "found {} Rust file(s) in the workspace; expected hundreds",
            workspace_rs.len()
        ))
    } else {
        Ok(dat_path_violations(&workspace_rs))
    };
    // Tracked files only: what the tree says, not what a working copy happens to hold (a local,
    // untracked configuration file may name anything).
    let env_names = tracked_texts(&ws, &[Path::new(".")]).map(|t| env_name_violations(&t));
    // Rule 11: the high-fidelity presentation changes only pixels.
    let hifi_manifest = workspace_manifest(&ws, HIFI);
    let members = hifi_manifest
        .as_ref()
        .map(|(m, _)| m.clone())
        .unwrap_or_default();
    let hifi_tree = cargo_tree_args(
        &ws,
        &[
            "-e",
            "normal,build",
            "--all-features",
            "--target",
            "all",
            "-p",
            HIFI,
        ],
    )
    .and_then(|t| {
        let (_, deps) = hifi_manifest.as_ref().map_err(Clone::clone)?;
        let mut v = hifi_tree_violations(&t, &members);
        v.extend(hifi_manifest_violations(deps));
        Ok(v)
    });
    let mut hifi_files = crate_sources(&ws, HIFI);
    let build_script = crate::util::crate_dir(&ws, HIFI).join("build.rs");
    if let Ok(text) = std::fs::read_to_string(&build_script) {
        let rel = build_script
            .strip_prefix(&ws)
            .map(Path::to_path_buf)
            .unwrap_or(build_script);
        hifi_files.push((rel, text));
    }
    let hifi_code = if hifi_files.is_empty() {
        Err(format!("no sources found for {HIFI}"))
    } else {
        Ok(hifi_code_violations(&hifi_files))
    };
    let hifi_dependents = cargo_tree_args(
        &ws,
        &[
            "--workspace",
            "--all-features",
            "-e",
            "normal,build,dev",
            "-i",
            HIFI,
            "--depth",
            "1",
        ],
    )
    .map(|t| hifi_dependent_violations(&t));
    let hifi_bridge = if workspace_rs
        .iter()
        .any(|(p, _)| p.to_string_lossy().replace('\\', "/") == HIFI_BRIDGE)
    {
        Ok(hifi_bridge_violations(&workspace_rs))
    } else {
        Err(format!("{HIFI_BRIDGE} is missing"))
    };
    let hifi_seam = std::fs::read_to_string(ws.join(HIFI_SEAM))
        .map(|t| hifi_seam_violations(&t))
        .map_err(|e| format!("{HIFI_SEAM}: {e}"));
    let shipped = SHIPPED_CLIENTS
        .iter()
        .map(|p| feature_tree(&ws, p).map(|t| shipped_client_violations(p, &t)))
        .try_fold(Vec::new(), |mut all, v| {
            all.extend(v?);
            Ok::<_, String>(all)
        });

    vec![
        row(
            "seam: primitives deps",
            primitives,
            format!(
                "direct deps within {}; no dereth-* or platform crate",
                PRIMITIVES_DIRECT.join(", ")
            ),
        ),
        row(
            "seam: primitives code",
            primitives_code,
            format!("no std I/O; unsafe only in {PRIMITIVES_UNSAFE_FILE}, behind host-nls"),
        ),
        row(
            "seam: dereth-client-contract deps",
            contract,
            format!(
                "direct deps within {}; no other dereth-*",
                CONTRACT_DIRECT.join(", ")
            ),
        ),
        row(
            "seam: ui-screens deps",
            screens_tree,
            "no dereth-client-model / dereth-client-runtime / dereth-client in any table"
                .to_owned(),
        ),
        row(
            "seam: ui-screens code",
            screens_code,
            "names no dereth_client_model / dereth_client_runtime / dereth_client".to_owned(),
        ),
        row(
            "seam: client-runtime deps",
            core_tree,
            format!(
                "nothing under dereth/client/crates/; none of {}",
                CORE_FORBIDDEN.join(", ")
            ),
        ),
        row(
            "seam: client-runtime code",
            core_code,
            format!("names none of {}", CORE_NAMES_NOT.join(" / ")),
        ),
        row(
            "seam: headless deps",
            headless,
            format!(
                "dereth-* deps exactly {}; nothing under dereth/client/",
                HEADLESS_DERE.join(", ")
            ),
        ),
        row(
            "seam: client-sdk deps",
            sdk,
            "nothing under dereth/client/ in any table: no dereth-input".to_owned(),
        ),
        row(
            "seam: action seam code",
            action_seam_code,
            format!(
                "{} name no key, key map, input map or device message",
                ACTION_SEAM_CRATES.join(", ")
            ),
        ),
        row(
            "seam: client crates",
            presentation,
            format!(
                "dereth/client/crates/* reach no dereth-client-runtime; {} depend on the contract",
                PRESENTATION_ON_CONTRACT.join(", ")
            ),
        ),
        row(
            "seam: library logging",
            library_log,
            format!(
                "core/* and dereth/client/crates/* reach none of {} (a binary installs it)",
                LIBRARY_NO_SUBSCRIBER.join(", ")
            ),
        ),
        row(
            "seam: host -> screens",
            host_screen,
            format!(
                "dereth-client and dereth-client-shell src name none of {}",
                HOST_NAMES_NO_SCREEN.join(", ")
            ),
        ),
        row(
            "seam: application halves deps",
            halves_tree,
            format!(
                "{} depend directly on none of {}; the scene not on the shell",
                APPLICATION_HALVES.join(", "),
                HALVES_FORBIDDEN.join(", ")
            ),
        ),
        row(
            "seam: application halves code",
            halves_code,
            format!(
                "{} name none of {}",
                APPLICATION_HALVES.join(", "),
                HALVES_NAMES_NOT.join(" / ")
            ),
        ),
        row(
            "seam: front end -> context",
            front_end,
            format!(
                "{} src but {FRONT_END_ASSEMBLY} names none of {}",
                FRONT_ENDS.join(", "),
                FRONT_END_NAMES_NOT.join(", ")
            ),
        ),
        row(
            "seam: dat paths",
            dat_paths,
            "only dereth-dat joins a retail dat file name onto a path".to_owned(),
        ),
        row(
            "seam: retired variables",
            env_names,
            "the workspace names no retired environment variable".to_owned(),
        ),
        row(
            "seam: high-fidelity deps",
            hifi_tree,
            format!(
                "{HIFI} reaches no workspace crate but {}, none of {}, and declares only those \
                 and {}",
                HIFI_TREE_DERE.join(", "),
                HIFI_TREE_FORBIDDEN.join(", "),
                HIFI_DIRECT_EXTERNAL.join(", ")
            ),
        ),
        row(
            "seam: high-fidelity code",
            hifi_code,
            "names no simulation, world, network, server or scene crate or type".to_owned(),
        ),
        row(
            "seam: high-fidelity dependents",
            hifi_dependents,
            format!("only {} depends on {HIFI}", HIFI_DEPENDENTS.join(", ")),
        ),
        row(
            "seam: high-fidelity bridge",
            hifi_bridge,
            format!(
                "only {HIFI_BRIDGE} names it; it writes nothing through a shared reference and \
                 calls only reviewed reads on the scene and the world"
            ),
        ),
        row(
            "seam: high-fidelity seam writes",
            hifi_seam,
            "the device's seam writes the device's state directly only on its allowed lines"
                .to_owned(),
        ),
        row(
            "seam: browser and headless clients without high fidelity",
            shipped,
            format!(
                "{}, each built on its own for every target, hold no high-fidelity presentation",
                SHIPPED_CLIENTS.join(", ")
            ),
        ),
    ]
}

/// `cargo xtask seams`.
pub fn seams() -> i32 {
    print_table("xtask seams", &reports())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ORACLE: none needed -- these calibrate the checker against planted violations, because a
    // seam check that silently stops firing is worse than none.

    fn tree(rows: &[(usize, &str)]) -> Vec<TreeEntry> {
        rows.iter()
            .map(|&(depth, name)| TreeEntry {
                depth,
                name: name.to_owned(),
            })
            .collect()
    }

    #[test]
    fn client_crates_reject_the_runtime_and_require_the_contract() {
        let clean = vec![
            (
                "dereth-ui".to_owned(),
                tree(&[
                    (0, "dereth-ui"),
                    (1, "dereth-client-contract"),
                    (2, "dereth-primitives"),
                ]),
            ),
            (
                "dereth-clipboard".to_owned(),
                tree(&[(0, "dereth-clipboard")]),
            ),
        ];
        assert!(presentation_violations(&clean).is_empty());

        let runtime = vec![(
            "dereth-render".to_owned(),
            tree(&[
                (0, "dereth-render"),
                (1, "dereth-client-contract"),
                (1, "dereth-x"),
                (2, "dereth-client-runtime"),
            ]),
        )];
        assert_eq!(presentation_violations(&runtime).len(), 1);

        let no_contract = vec![(
            "dereth-ui-screens".to_owned(),
            tree(&[(0, "dereth-ui-screens"), (1, "dereth-primitives")]),
        )];
        assert_eq!(presentation_violations(&no_contract).len(), 1);
    }

    #[test]
    fn the_application_halves_sit_on_the_runtime_and_reach_no_platform() {
        let on_runtime = vec![(
            "dereth-scene".to_owned(),
            tree(&[
                (0, "dereth-scene"),
                (1, "dereth-client-runtime"),
                (1, "dereth-render"),
                (2, "windows"),
            ]),
        )];
        assert!(presentation_violations(&on_runtime).is_empty());
        assert!(halves_violations(&on_runtime).is_empty());

        let platform = vec![(
            "dereth-client-shell".to_owned(),
            tree(&[
                (0, "dereth-client-shell"),
                (1, "winit"),
                (1, "dereth-clipboard"),
            ]),
        )];
        assert_eq!(halves_violations(&platform).len(), 2);

        let upward = vec![(
            "dereth-scene".to_owned(),
            tree(&[
                (0, "dereth-scene"),
                (1, "dereth-x"),
                (2, "dereth-client-shell"),
            ]),
        )];
        assert_eq!(halves_violations(&upward).len(), 1);

        let code = vec![(
            PathBuf::from("src/a.rs"),
            "#[cfg(windows)]\nlet n = layout.windows.len();\nuse winit::event;\nlet h = windows::core::HSTRING::new();\n".to_owned(),
        )];
        assert_eq!(path_violations(&code, HALVES_NAMES_NOT).len(), 2);
    }

    #[test]
    fn a_front_end_names_neither_the_application_nor_its_internals() {
        let code = vec![(
            PathBuf::from("src/front_end.rs"),
            "fn step(cx: &mut Cx<'_, H>) {}\n\
             fn bad(core: &mut CoreApp<H>) {}\n\
             let i: &mut crate::interaction::Interaction = x;\n\
             use crate::interaction::TargetMode;\n\
             // the App is assembled elsewhere\n\
             let apply = AppState::default();\n"
                .to_owned(),
        )];
        assert_eq!(code_violations(&code, FRONT_END_NAMES_NOT).len(), 2);
        assert!(is_front_end_assembly(Path::new(
            "dereth\\client\\crates\\shell\\src\\app.rs"
        )));
        assert!(!is_front_end_assembly(Path::new(
            "dereth/client/crates/shell/src/front_end.rs"
        )));
    }

    #[test]
    fn headless_depends_on_the_sdk_and_nothing_else_of_ours() {
        let crates = vec!["dereth-ui".to_owned(), "dereth-render".to_owned()];
        let clean = tree(&[
            (0, "dereth-headless"),
            (1, "dereth-client-sdk"),
            (2, "dereth-client-runtime"),
            (3, "dereth-primitives"),
            (1, "thiserror"),
        ]);
        assert!(headless_violations(&clean, &crates).is_empty());
        // A second direct dereth-* dependency, even a shared one the SDK already re-exports.
        let side = tree(&[
            (0, "dereth-headless"),
            (1, "dereth-client-sdk"),
            (1, "dereth-primitives"),
        ]);
        assert_eq!(headless_violations(&side, &crates).len(), 1);
        // The executable, anywhere.
        let product = tree(&[
            (0, "dereth-headless"),
            (1, "dereth-client-sdk"),
            (1, "dereth-client"),
        ]);
        assert_eq!(headless_violations(&product, &crates).len(), 2);
        // A presentation crate reached through something else.
        let deep = tree(&[
            (0, "dereth-headless"),
            (1, "dereth-client-sdk"),
            (1, "serde"),
            (2, "dereth-ui"),
        ]);
        assert_eq!(headless_violations(&deep, &crates).len(), 1);
        // And the SDK itself missing.
        let bare = tree(&[(0, "dereth-headless"), (1, "thiserror")]);
        assert_eq!(headless_violations(&bare, &crates).len(), 1);
    }

    #[test]
    fn the_sdk_reaches_no_device_input_and_no_client_crate() {
        let crates = vec!["dereth-input".to_owned(), "dereth-ui".to_owned()];
        let clean = tree(&[
            (0, "dereth-client-sdk"),
            (1, "dereth-client-runtime"),
            (2, "dereth-client-contract"),
            (1, "dereth-client-model"),
        ]);
        assert!(sdk_violations(&clean, &crates).is_empty());
        // The device input, direct or through something else.
        let direct = tree(&[(0, "dereth-client-sdk"), (1, "dereth-input")]);
        assert_eq!(sdk_violations(&direct, &crates).len(), 1);
        let deep = tree(&[
            (0, "dereth-client-sdk"),
            (1, "dereth-client-runtime"),
            (2, "dereth-input"),
        ]);
        assert_eq!(sdk_violations(&deep, &crates).len(), 1);
        let product = tree(&[(0, "dereth-client-sdk"), (1, "dereth-client")]);
        assert_eq!(sdk_violations(&product, &crates).len(), 1);
    }

    #[test]
    fn the_action_seam_names_no_device_but_may_describe_one() {
        let clean = vec![(
            PathBuf::from("core/client-runtime/src/a.rs"),
            "//! The front end's input layer reads `WM_KEYDOWN`; this crate takes an `Action`.
             let a = Action::begin(ActionId(0x29)); // not an InputEvent
             let m = WindowMessage::new(msg::WM_ACTIVATEAPP, 1, 0);
             let n = input_events_seen; let k = keymap_file_name;
"
            .to_owned(),
        )];
        assert!(
            code_violations(&clean, DEVICE_NAMES_NOT).is_empty(),
            "{:?}",
            code_violations(&clean, DEVICE_NAMES_NOT)
        );
        let planted = vec![(
            PathBuf::from("core/client-sdk/src/lib.rs"),
            "pub use dereth_input as input;
             fn f(e: &InputEvent) {}
             let m = Win32Message::new(msg::WM_KEYDOWN, 0x57, 0, 0);
             if key.scan_code == 0x11 {}
"
            .to_owned(),
        )];
        let hits = code_violations(&planted, DEVICE_NAMES_NOT);
        assert_eq!(hits.len(), 5, "{hits:?}");
    }

    #[test]
    fn a_library_may_emit_events_but_may_not_carry_a_subscriber() {
        let facade = tree(&[
            (0, "dereth-world-data"),
            (1, "tracing"),
            (2, "tracing-core"),
        ]);
        assert!(library_log_violations(&[("dereth-world-data".to_owned(), facade)]).is_empty());
        let planted = tree(&[
            (0, "dereth-audio"),
            (1, "dereth-primitives"),
            (2, "tracing-subscriber"),
        ]);
        let v = library_log_violations(&[("dereth-audio".to_owned(), planted)]);
        assert_eq!(v.len(), 1, "{v:?}");
        assert!(v[0].contains("tracing-subscriber"), "{v:?}");
    }

    #[test]
    fn parses_depth_prefixed_tree_lines() {
        let text =
            "0dereth-client-contract v0.0.0 (C:\\x)\n1dereth-primitives v0.0.0 (C:\\y)\n2thiserror v2.0.20\n\
                    1glam v0.30.10\n\n";
        assert_eq!(
            parse_tree(text),
            tree(&[
                (0, "dereth-client-contract"),
                (1, "dereth-primitives"),
                (2, "thiserror"),
                (1, "glam")
            ])
        );
    }

    #[test]
    fn contract_allows_its_list_and_rejects_a_planted_dependency() {
        let clean = tree(&[
            (0, "dereth-client-contract"),
            (1, "dereth-primitives"),
            (2, "thiserror"),
            (2, "pxfm"),
            (1, "glam"),
            (1, "raw-window-handle"),
        ]);
        assert!(contract_violations(&clean).is_empty());

        let direct = tree(&[(0, "dereth-client-contract"), (1, "serde")]);
        assert_eq!(contract_violations(&direct).len(), 1);

        let deep = tree(&[
            (0, "dereth-client-contract"),
            (1, "dereth-primitives"),
            (2, "dereth-ui"),
        ]);
        assert_eq!(contract_violations(&deep).len(), 1);

        let platform = tree(&[(0, "dereth-client-contract"), (1, "glam"), (2, "winit")]);
        assert_eq!(contract_violations(&platform).len(), 1);
    }

    #[test]
    fn primitives_allow_their_list_and_reject_a_planted_dependency() {
        let clean = tree(&[
            (0, "dereth-primitives"),
            (1, "libm"),
            (1, "pxfm"),
            (1, "thiserror"),
            (2, "thiserror-impl"),
        ]);
        assert!(primitives_violations(&clean).is_empty());
        let direct = tree(&[(0, "dereth-primitives"), (1, "glam")]);
        assert_eq!(primitives_violations(&direct).len(), 1);
        let dere = tree(&[
            (0, "dereth-primitives"),
            (1, "thiserror"),
            (2, "dereth-dat"),
        ]);
        assert_eq!(primitives_violations(&dere).len(), 1);
        let platform = tree(&[(0, "dereth-primitives"), (1, "pxfm"), (2, "windows-sys")]);
        assert_eq!(primitives_violations(&platform).len(), 1);
    }

    #[test]
    fn primitives_code_allows_unsafe_in_the_nls_arm_only_and_no_io_anywhere() {
        let decl = PRIMITIVES_UNSAFE_DECL.to_owned() + "\npub mod cp1252;\n";
        let clean = vec![
            (PathBuf::from("core/primitives/src/text/mod.rs"), decl.clone()),
            (
                PathBuf::from("core/primitives/src/text/windows.rs"),
                "extern \"system\" { fn GetACP() -> u32; }\npub fn acp() -> u32 { unsafe { GetACP() } }\n"
                    .to_owned(),
            ),
            (
                PathBuf::from("core/primitives/src/asset.rs"),
                "//! No std::fs here.\npub enum E { Io { source: std::io::Error } }\n".to_owned(),
            ),
        ];
        assert!(
            primitives_code_violations(&clean).is_empty(),
            "{:?}",
            primitives_code_violations(&clean)
        );

        let io = vec![(
            PathBuf::from("core/primitives/src/shape.rs"),
            "let f = std::fs::read(p);\n".to_owned(),
        )];
        assert_eq!(primitives_code_violations(&io).len(), 1);
        let stray = vec![(
            PathBuf::from("core/primitives/src/num/mod.rs"),
            "#[allow(unsafe_code)]\nfn f() { unsafe { g() } }\n".to_owned(),
        )];
        assert_eq!(primitives_code_violations(&stray).len(), 2);
        let ungated = vec![(
            PathBuf::from("core/primitives/src/text/mod.rs"),
            "#[allow(unsafe_code)]\nmod windows;\n".to_owned(),
        )];
        assert_eq!(primitives_code_violations(&ungated).len(), 1);
    }

    #[test]
    fn a_forbidden_name_anywhere_in_the_tree_is_a_violation() {
        let t = tree(&[
            (0, "dereth-ui-screens"),
            (1, "dereth-rules"),
            (2, "dereth-client-model"),
        ]);
        assert_eq!(
            tree_violations("dereth-ui-screens", &t, SCREENS_FORBIDDEN).len(),
            1
        );
        let t = tree(&[
            (0, "dereth-client-runtime"),
            (1, "dereth-audio"),
            (2, "cpal"),
        ]);
        assert_eq!(
            tree_violations("dereth-client-runtime", &t, CORE_FORBIDDEN).len(),
            1
        );
        let t = tree(&[(0, "dereth-client-runtime"), (1, "windows-sys")]);
        assert!(tree_violations("dereth-client-runtime", &t, CORE_FORBIDDEN).is_empty());
    }

    #[test]
    fn the_runtime_reaches_no_client_crate_and_no_platform() {
        let crates = vec![
            "dereth-world-render".to_owned(),
            "dereth-new-panel".to_owned(),
        ];
        let clean = tree(&[
            (0, "dereth-client-runtime"),
            (1, "dereth-landscape"),
            (1, "dereth-physics"),
            (2, "dereth-primitives"),
        ]);
        assert!(core_violations(&clean, &crates).is_empty());
        let drawing = tree(&[
            (0, "dereth-client-runtime"),
            (1, "dereth-landscape"),
            (2, "dereth-world-render"),
        ]);
        assert_eq!(core_violations(&drawing, &crates).len(), 1);
        // A crate under dereth/client/crates/ that the fixed list does not name is still caught.
        let unnamed = tree(&[(0, "dereth-client-runtime"), (1, "dereth-new-panel")]);
        assert_eq!(core_violations(&unnamed, &crates).len(), 1);
        let product = tree(&[(0, "dereth-client-runtime"), (1, "dereth-client")]);
        assert_eq!(core_violations(&product, &crates).len(), 1);
    }

    #[test]
    fn code_names_count_but_comments_and_longer_identifiers_do_not() {
        let files = vec![(
            PathBuf::from("a.rs"),
            "//! `dereth_client_model::chat` is mentioned in a doc\n\
             let x = 1; // dereth_client_model::World\n\
             use dereth_ui_screens::panels;\n\
             use dereth_client_model::World;\n"
                .to_owned(),
        )];
        let hits = code_violations(&files, &["dereth_client_model", "dereth_ui"]);
        assert_eq!(hits.len(), 1, "{hits:?}");
        assert!(hits[0].starts_with("a.rs:4:"));
        assert_eq!(code_violations(&files, &["dereth_ui_screens"]).len(), 1);
        assert!(code_violations(&files, &["dereth_client"]).is_empty());
    }

    #[test]
    fn only_dereth_dat_joins_a_retail_file_name_onto_a_path() {
        let planted = vec![
            (
                PathBuf::from("dereth/client/tests/dat/a.rs"),
                "let ok = dir\n    .join(\n        \"client_portal.dat\",\n    )\n    .exists();\n"
                    .to_owned(),
            ),
            (
                PathBuf::from("core/client-runtime/src/b.rs"),
                "let p = format!(\"{}/client_cell_1.dat\", dir);\n".to_owned(),
            ),
        ];
        let hits = dat_path_violations(&planted);
        assert_eq!(hits.len(), 2, "{hits:?}");
        assert!(
            hits[0].starts_with("core/client-runtime/src/b.rs:1:"),
            "{hits:?}"
        );
        assert!(
            hits[1].starts_with("dereth/client/tests/dat/a.rs:2:"),
            "{hits:?}"
        );

        let clean = vec![
            (
                PathBuf::from("core/dat/src/locate.rs"),
                "dir.join(\"client_portal.dat\")".to_owned(),
            ),
            (
                PathBuf::from("core/client-runtime/src/ddd.rs"),
                "Self::Portal => \"client_portal.dat\",\n// dir.join(\"client_portal.dat\")\n\
                 let d = display_string(ID, &[\"client_local_English.dat\"]);\n"
                    .to_owned(),
            ),
        ];
        assert!(dat_path_violations(&clean).is_empty());
    }

    /// Calibration: in a real git work tree, a tracked file naming a retired variable is found
    /// and an untracked one beside it naming another is not.
    #[test]
    fn only_tracked_files_are_read_for_retired_variables() {
        let repo = std::env::temp_dir().join(format!("xtask-seams-tracked-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&repo);
        std::fs::create_dir_all(repo.join("docs")).expect("temp repo");
        let git = |args: &[&str]| {
            let ok = Command::new("git")
                .arg("-C")
                .arg(&repo)
                .args(args)
                .status()
                .expect("git runs")
                .success();
            assert!(ok, "git {args:?}");
        };
        git(&["init", "-q"]);
        let tracked = format!("{RETIRED_VAR_PREFIX}CAPTURES\n");
        let untracked = format!("{RETIRED_VAR_PREFIX}GOLDEN\n");
        std::fs::write(repo.join("docs/tracked.toml"), &tracked).expect("write");
        std::fs::write(repo.join("docs/local.toml"), &untracked).expect("write");
        git(&["add", "docs/tracked.toml"]);
        let texts = tracked_texts(&repo, &[Path::new("docs"), Path::new("tools")]);
        std::fs::remove_dir_all(&repo).ok();
        let hits = env_name_violations(&texts.expect("a work tree"));
        assert_eq!(hits.len(), 1, "{hits:?}");
        assert!(hits[0].starts_with("docs/tracked.toml:1:"), "{hits:?}");
        // The same two files read without the listing would both be found.
        let both = vec![
            (PathBuf::from("docs/tracked.toml"), tracked),
            (PathBuf::from("docs/local.toml"), untracked),
        ];
        assert_eq!(env_name_violations(&both).len(), 2);
    }

    #[test]
    fn a_retired_variable_name_is_found_in_code_and_in_prose() {
        let old = RETIRED_DAT_VAR;
        let prefixed = format!("{RETIRED_VAR_PREFIX}CAPTURES");
        let planted = vec![
            (
                PathBuf::from("tools/x.py"),
                format!("os.environ.get(\"{prefixed}\")\n"),
            ),
            (
                PathBuf::from("docs/README.md"),
                format!("line one\npoint `${old}` at the install\n"),
            ),
        ];
        let hits = env_name_violations(&planted);
        assert_eq!(hits.len(), 2, "{hits:?}");
        assert!(hits[1].starts_with("docs/README.md:2:"), "{hits:?}");
        let clean = vec![(
            PathBuf::from("tools/y.py"),
            "DERETH_CAPTURES DERETH_TEST_DAT_DIR dere_fixtures EMPYREAN_TEST_WORLD_PACK\n"
                .to_owned(),
        )];
        assert!(env_name_violations(&clean).is_empty());
    }

    #[test]
    fn the_high_fidelity_presentation_reaches_only_the_device_and_is_reached_only_by_the_scene() {
        let members: Vec<String> = ["dereth-render", "empyrean-world", "xtask"]
            .iter()
            .map(|m| (*m).to_owned())
            .collect();
        let clean = tree(&[
            (0, "dereth-render-hifi"),
            (1, "dereth-render"),
            (2, "dereth-render-cpu"),
            (2, "dereth-client-contract"),
            (3, "dereth-primitives"),
            (1, "wgpu"),
        ]);
        assert!(hifi_tree_violations(&clean, &members).is_empty());
        let reaching = tree(&[
            (0, "dereth-render-hifi"),
            (1, "dereth-render"),
            (2, "dereth-physics"),
            (1, "winit"),
            (1, "empyrean-world"),
            (2, "xtask"),
        ]);
        assert_eq!(hifi_tree_violations(&reaching, &members).len(), 4);
        // A server crate is refused even when the workspace listing could not be read.
        assert_eq!(hifi_tree_violations(&reaching, &[]).len(), 3);

        let only_scene = tree(&[(0, "dereth-render-hifi"), (1, "dereth-scene")]);
        assert!(hifi_dependent_violations(&only_scene).is_empty());
        let another = tree(&[
            (0, "dereth-render-hifi"),
            (1, "dereth-scene"),
            (1, "dereth-client"),
            (2, "dereth-x"),
        ]);
        assert_eq!(hifi_dependent_violations(&another).len(), 1);
    }

    #[test]
    fn the_high_fidelity_manifest_declares_only_the_device_and_its_listed_crates() {
        let dep = |name: &str, dev: bool| ManifestDep {
            name: name.to_owned(),
            dev,
        };
        let clean = vec![
            dep("dereth-render", false),
            dep("dereth-render-cpu", false),
            dep("dereth-primitives", false),
            dep("wgpu", false),
            dep("glam", false),
            dep("naga", true),
            dep("dereth-terrain", true),
        ];
        assert!(hifi_manifest_violations(&clean).is_empty());
        let declaring = vec![
            dep("empyrean-world", false),
            dep("serde", false),
            dep("dereth-terrain", false),
            dep("dereth-physics", true),
        ];
        assert_eq!(
            hifi_manifest_violations(&declaring).len(),
            4,
            "{:#?}",
            hifi_manifest_violations(&declaring)
        );
    }

    #[test]
    fn the_high_fidelity_code_names_no_simulation_and_its_tests_alone_the_landscape() {
        let files = vec![
            (
                PathBuf::from("dereth/client/crates/render-hifi/src/a.rs"),
                "use dereth_render::wgpu::sidecar::Mark;\n\
                 // the scene's WorldState is not named here\n\
                 fn f(w: &WorldState) {}\n\
                 let r = dereth_client_runtime::x();\n\
                 let h = dereth_terrain::hash(1);\n\
                 let s = my_dereth_scene_name;\n\
                 let u = \"a//b\"; dereth_physics::x();\n\
                 /* dereth_physics, in a block comment */\n\
                 let w = empyrean_world::Map::new();\n"
                    .to_owned(),
            ),
            (
                PathBuf::from("dereth/client/crates/render-hifi/tests/cpu/b.rs"),
                "let h = dereth_terrain::hash(1);\nuse dereth_physics::x;\n".to_owned(),
            ),
            (
                PathBuf::from("dereth/client/crates/render-hifi/build.rs"),
                "fn main() { let _ = dereth_terrain::x; dereth_protocol::y(); }\n".to_owned(),
            ),
        ];
        let found = hifi_code_violations(&files);
        assert_eq!(found.len(), 8, "{found:#?}");
    }

    #[test]
    fn comments_are_blanked_and_strings_kept_line_for_line() {
        let text = "a // b\n\"x//y\" c\n/* d\ne */ f\nlet q = '\"'; g // h\n\
                    let r = r#\"s//\"#; i\nlet z = '\\''; j // k\nfn l<'a>() {} // m\n";
        let blanked = blank_comments(text);
        assert_eq!(blanked.lines().count(), text.lines().count());
        let words: Vec<&str> = blanked.split_whitespace().collect();
        for kept in ["a", "\"x//y\"", "c", "f", "g", "i", "j", "l<'a>()"] {
            assert!(words.contains(&kept), "{kept} lost from {blanked:?}");
        }
        for gone in ["b", "d", "e", "h", "k", "m"] {
            assert!(!words.contains(&gone), "{gone} kept in {blanked:?}");
        }
    }

    #[test]
    fn only_the_bridge_names_the_presentation_and_the_bridge_only_reads() {
        let files = vec![
            (
                PathBuf::from(HIFI_BRIDGE),
                "use dereth_render_hifi::HifiRenderer;\n\
                 let s = settings(&prefs);\n\
                 let mut sh = shadow.take();\n\
                 let w = world.borrow_mut();\n\
                 draw.set_weather_enabled(true);\n\
                 let p = physics.terrain_height_at(x);\n\
                 // never borrow_mut here\n\
                 draw.frame_pick_candidates.take();\n\
                 draw.selected_part_drawn.set(true);\n\
                 let c = std::mem::take(&mut x);\n\
                 draw.viewcone_check_object_id.replace(0);\n\
                 let v = draw.view_params(ws, 1, 1);\n\
                 draw.rebuild();\n\
                 static N: AtomicU32 = AtomicU32::new(0);\n\
                 let s = \"//\"; draw.selected_part_drawn.set(true);\n\
                 /* draw.rebuild(); */\n"
                    .to_owned(),
            ),
            (
                PathBuf::from("dereth/client/crates/scene/src/world_scene/views.rs"),
                "hifi_bridge::publish(self, &mut self.hifi.borrow_mut(), ws, gpu);\n\
                 let r = dereth_render_hifi::HifiRenderer::new(s);\n"
                    .to_owned(),
            ),
            (
                PathBuf::from("dereth/client/crates/render-hifi/src/lib.rs"),
                "pub use dereth_render_hifi as me;\n".to_owned(),
            ),
        ];
        let found = hifi_bridge_violations(&files);
        assert_eq!(found.len(), 13, "{found:#?}");
    }

    #[test]
    fn the_device_seam_writes_the_device_state_only_on_its_allowed_lines() {
        let clean = "let n = self.commands.borrow().len();\n\
                     if self.frame_stamp == 3 {}\n\
                     self.frame_stamp += 1;\n\
                     let v = self.depth_view();\n\
                     self.sidecar = Some(i);\n\
                     let r = self.commands[0..1].iter();\n\
                     let n = self.uniform_arena.len();\n\
                     uniform_arena: &mut self.uniform_arena,\n\
                     uniform_arena: &mut self.uniform_arena,\n\
                     self.uniform_arena.truncate(recorded_uniform_len);\n";
        assert!(
            hifi_seam_violations(clean).is_empty(),
            "{:#?}",
            hifi_seam_violations(clean)
        );
        let writes = "self.commands.borrow_mut().push(c);\n\
                      self.draw_calls += 1;\n\
                      self.textures.insert(1, t);\n\
                      let p = &mut self.pipelines;\n\
                      self.depth = None;\n\
                      self.frame_stamp += 1;\n\
                      self.frame_stamp += 1;\n\
                      self.uniform_arena.truncate(0);\n\
                      self.uniform_arena.truncate(recorded_uniform_len);\n\
                      self.uniform_arena.truncate(recorded_uniform_len);\n\
                      self.vertex_arena.clear();\n\
                      self.pipelines[0] = p;\n\
                      self.textures.retain(|_| true);\n\
                      uniform_arena: &mut self.uniform_arena,\n\
                      uniform_arena: &mut self.uniform_arena,\n\
                      uniform_arena: &mut self.uniform_arena,\n";
        let found = hifi_seam_violations(writes);
        assert_eq!(found.len(), 12, "{found:#?}");
    }

    #[test]
    fn a_shipped_client_holding_the_presentation_is_refused() {
        let clean = "dereth-render v0.0.0 (here)\n\
                     dereth-render feature \"mock\"\n\
                     dereth-scene feature \"wgpu\"\n";
        assert!(shipped_client_violations("dereth-client", clean).is_empty());
        let holding = "dereth-render v0.0.0 (here)\n\
                       dereth-render feature \"hifi\"\n\
                       dereth-render-hifi v0.0.0 (there)\n\
                       dereth-scene feature \"hifi\"\n";
        assert_eq!(shipped_client_violations("dereth-client", holding).len(), 3);
    }
}
