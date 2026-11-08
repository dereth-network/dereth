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
//! 11. **The plugin host's crates** keep to their layers. `dereth-plugin-api` (the interface
//!     as types) depends directly on [`PLUGIN_API_DIRECT`] only; `dereth-plugin-package` (packages
//!     as files) on [`PLUGIN_PACKAGE_DIRECT`] only; `dereth-plugin-wasm` (compiled plugins) on
//!     [`PLUGIN_WASM_DIRECT`] only; `dereth-plugin-lua` (the engine)
//!     on [`PLUGIN_LUA_DIRECT`] only, with no other `dereth-*` crate in its tree, and holds its
//!     `unsafe` to the Rust half of the VM's C runtime in a browser, compiled for wasm32 only
//!     ([`plugin_lua_code_violations`]); and
//!     `dereth-plugin-host` reaches the game as the headless client does, with no crate under
//!     `dereth/client/` anywhere in its tree and none of their names in its code.
//! 8. **One place finds the retail dats.** No workspace file outside `core/dat/` joins a retail dat
//!    file name onto a path ([`dat_path_violations`]): the client, the server and every test find
//!    the directory with `dereth_dat::locate_modern_dats` (the tests through `dereth_dat::testing`)
//!    and name a file in it with `dereth_dat::ModernDat`. And no tracked file of the workspace
//!    names a retired environment variable ([`env_name_violations`]).
//! 12. **The web page runs the launcher's page and keeps no copy of it.** No file git tracks under
//!     [`WEB_PAGE`] has the name of a file of [`LAUNCHER_UI`] (the web page's own `index.html`
//!     aside, and [`LAUNCHER_SHELL_IN_WEB`] included) or the bytes of one, under any name
//!     ([`launcher_copy_violations`]): the build brings those files in, and two copies drift.
//! 13. **Each game's web module holds its game alone, and the launcher's neither.** The page fetches
//!     a module only when it is wanted, and a static host serves a file of 25 MiB at most, so the
//!     modules' trees stay apart ([`WEB_MODULES`]).
//! 14. **The remaster changes only pixels**: its backend layer reaches no workspace crate, the
//!     remaster none but the device's own, neither names the simulation, the world, the network
//!     or the scene, the scene alone depends on the remaster and only its bridge and asset decoder
//!     name it, the Vulkan device's seam writes none of the device's state but its own, and the
//!     browser and headless clients hold none of it ([`remaster`]).
//!
//! The dependency half reads `cargo tree`, so a rule broken two crates down is still caught. The
//! code half is a text scan with comments stripped: the crates' docs are allowed to talk about the
//! other side of a seam, their code is not allowed to reach across it.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::util::{print_table, workspace_root, Outcome, Report};

mod auberean;
pub mod remaster;

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

/// `dereth-plugin-api`'s allowed direct production dependencies: the interface as types needs the
/// game's identifiers and nothing else.
pub const PLUGIN_API_DIRECT: &[&str] = &["dereth-primitives"];

/// `dereth-plugin-package`'s allowed direct production dependencies: the interface and the formats
/// a package is made of. The launcher builds it, so it stays this small.
pub const PLUGIN_PACKAGE_DIRECT: &[&str] =
    &["dereth-plugin-api", "toml", "zip", "sha2", "ed25519-dalek"];

/// `dereth-plugin-wasm`'s allowed direct production dependencies: the interface, reading and
/// rewriting modules, the host's loan to the imports, the budget clock, and the interpreter.
pub const PLUGIN_WASM_DIRECT: &[&str] = &[
    "dereth-plugin-api",
    "wasmparser",
    "wasm-encoder",
    "scoped-tls-hkt",
    "web-time",
    "wasmi",
];

/// The one file of `dereth-plugin-lua` allowed `unsafe`: the Rust half of the Luau VM's C runtime in
/// a browser, and the declaration that compiles it for wasm32 only.
const PLUGIN_LUA_UNSAFE_FILE: &str = "src/wasm_rt.rs";
const PLUGIN_LUA_UNSAFE_DECL: &str =
    "#[cfg(target_arch = \"wasm32\")]\n#[allow(unsafe_code)]\npub mod wasm_rt;";

/// `dereth-plugin-lua`'s allowed direct production dependencies: the interface, the Luau VM and
/// the budget clock.
pub const PLUGIN_LUA_DIRECT: &[&str] = &["dereth-plugin-api", "mlua", "web-time"];

/// Platform crates no plugin crate reaches. (The Luau VM's bindings use the C library's types, so
/// `libc` itself is not one of them.)
const PLUGIN_FORBIDDEN: &[&str] = &["winit", "windows", "cpal", "ash"];

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

/// The plugin engine's `unsafe` rule over its production code: `unsafe` only in
/// [`PLUGIN_LUA_UNSAFE_FILE`], declared for wasm32 only, and denied everywhere else by the crate.
pub fn plugin_lua_code_violations(files: &[(PathBuf, String)]) -> Vec<String> {
    let mut out = Vec::new();
    let unix = |p: &Path| p.to_string_lossy().replace('\\', "/");
    for (file, text) in files {
        let name = unix(file);
        if name.ends_with(PLUGIN_LUA_UNSAFE_FILE) {
            continue;
        }
        let is_lib = name.ends_with("src/lib.rs");
        for (i, line) in text.lines().enumerate() {
            let code = strip_comment(line).trim();
            let allowed =
                is_lib && (code == "#![deny(unsafe_code)]" || code == "#[allow(unsafe_code)]");
            if !allowed && (names_ident(code, "unsafe") || names_ident(code, "unsafe_code")) {
                out.push(format!(
                    "{}:{}: unsafe code outside {PLUGIN_LUA_UNSAFE_FILE}: {}",
                    file.display(),
                    i + 1,
                    line.trim()
                ));
            }
        }
        if is_lib {
            let text = text.replace("\r\n", "\n");
            if !text.contains(PLUGIN_LUA_UNSAFE_DECL) || !text.contains("#![deny(unsafe_code)]") {
                out.push(format!(
                    "{}: must deny unsafe code and declare `wasm_rt` exactly as `{}`",
                    file.display(),
                    PLUGIN_LUA_UNSAFE_DECL.replace('\n', " ")
                ));
            }
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

/// A plugin crate's rule over its production tree: its direct dependencies are within `direct`, and
/// the only `dereth-*` crates anywhere in it are `dereth-primitives`, `dereth-plugin-api` and itself.
pub fn plugin_layer_violations(krate: &str, tree: &[TreeEntry], direct: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    for e in tree {
        if e.depth == 1 && !direct.contains(&e.name.as_str()) {
            out.push(format!(
                "{krate} depends directly on `{}`; allowed: {}",
                e.name,
                direct.join(", ")
            ));
        }
        if e.depth > 0
            && e.name.starts_with("dereth-")
            && !["dereth-primitives", "dereth-plugin-api", krate].contains(&e.name.as_str())
        {
            out.push(format!(
                "{krate}'s tree contains `{}` (depth {})",
                e.name, e.depth
            ));
        }
    }
    out.extend(tree_violations(krate, tree, PLUGIN_FORBIDDEN));
    dedup(out)
}

/// The plugin host's rule over its production tree: neither `dereth-client` nor any crate under
/// `dereth/client/crates/` (`client_crates`) appears at any depth.
pub fn plugin_host_violations(tree: &[TreeEntry], client_crates: &[String]) -> Vec<String> {
    let mut forbidden: Vec<&str> = vec!["dereth-client"];
    forbidden.extend(client_crates.iter().map(String::as_str));
    tree_violations("dereth-plugin-host", tree, &forbidden)
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

/// Rule 12: the launcher's page, relative to the workspace.
pub const LAUNCHER_UI: &str = "launcher/ui";

/// Rule 12: the web page's folder, relative to the workspace.
pub const WEB_PAGE: &str = "dereth/web/www";

/// Rule 12: the name the launcher's shell has in the web page's folder, where the build brings it.
pub const LAUNCHER_SHELL_IN_WEB: &str = "launcher.html";

/// Rule 12 over the files git tracks in the web page's folder (`www`) and in the launcher's page
/// (`ui`), each with its bytes: any file of `www` named as one of `ui` is, or that holds the same
/// bytes as one. Both are `index.html` by design, so that name alone does not count.
#[must_use]
pub fn launcher_copy_violations(
    www: &[(PathBuf, Vec<u8>)],
    ui: &[(PathBuf, Vec<u8>)],
) -> Vec<String> {
    let name = |p: &Path| {
        p.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    };
    let mut out = Vec::new();
    for (path, bytes) in www {
        let file = name(path);
        if file == LAUNCHER_SHELL_IN_WEB
            || (file != "index.html" && ui.iter().any(|(p, _)| name(p) == file))
        {
            out.push(format!(
                "{}: named as a file of the launcher's page; the build brings that file in",
                path.display()
            ));
        } else if let Some((p, _)) = ui.iter().find(|(_, b)| b == bytes) {
            out.push(format!(
                "{}: a copy of {}; the build brings that file in",
                path.display(),
                p.display()
            ));
        }
    }
    dedup(out)
}

/// The bytes of every file git tracks under `dir` (relative to `repo`); a listed file that is not
/// there (deleted in the work tree) is skipped.
fn tracked_bytes(repo: &Path, dir: &Path) -> Result<Vec<(PathBuf, Vec<u8>)>, String> {
    let listing = git_ls_files(repo, &[dir])?;
    Ok(listing
        .split(|b| *b == 0)
        .filter(|n| !n.is_empty())
        .filter_map(|n| {
            let rel = PathBuf::from(String::from_utf8_lossy(n).into_owned());
            let bytes = std::fs::read(repo.join(&rel)).ok()?;
            Some((rel, bytes))
        })
        .collect())
}

/// Rule 12 over the work tree at `repo`.
fn launcher_copies(repo: &Path) -> Result<Vec<String>, String> {
    let ui = tracked_bytes(repo, Path::new(LAUNCHER_UI))?;
    if ui.is_empty() {
        return Err(format!("git tracks no file under {LAUNCHER_UI}"));
    }
    Ok(launcher_copy_violations(
        &tracked_bytes(repo, Path::new(WEB_PAGE))?,
        &ui,
    ))
}

/// Rule 13: the web client's modules, and what each one's tree never holds, by a crate's name or
/// its prefix (a name ending in `-`).
pub const WEB_MODULES: &[(&str, &[&str])] = &[
    (
        "dereth-web-front",
        &[
            "dereth-web",
            "dereth-client-shell",
            "dereth-scene",
            "dereth-render",
            "dereth-horizon",
            "dereth-plugin-lua",
            "auberean-web",
            "auberean-shell",
            "auberean-scene",
            "auberean-gpu",
            "auberean-client-sdk",
        ],
    ),
    (
        "dereth-web",
        &["auberean-", "dereth-web-front", "dereth-launch"],
    ),
    (
        "auberean-web",
        &[
            "dereth-web",
            "dereth-client-shell",
            "dereth-client-runtime",
            "dereth-scene",
            "dereth-render",
            "dereth-launch",
        ],
    ),
];

/// Rule 13 over a module's `package` tree (`cargo tree --prefix none -f {p}`, a crate to a line).
#[must_use]
pub fn web_module_violations(package: &str, not: &[&str], tree: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in tree.lines() {
        let Some(name) = line.split_whitespace().next() else {
            continue;
        };
        if name == package {
            continue;
        }
        if not
            .iter()
            .any(|n| name == *n || (n.ends_with('-') && name.starts_with(n)))
        {
            out.push(format!("`{package}` holds `{name}`, which is another's"));
        }
    }
    dedup(out)
}

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

/// One dependency a manifest declares: its package name, and whether it is for tests alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestDep {
    pub name: String,
    pub dev: bool,
}

/// The client's crates a Lyceum crate may depend on besides the core's and its own: the drawing,
/// the desktop host it runs in, the console it borrows from a terminal and the device input, the
/// client application's shell (whose renderer is the runtime's presentation on a device) and its
/// screens' world map.
pub const LYCEUM_MAY_USE: &[&str] = &[
    "dereth-scene",
    "dereth-render",
    "dereth-world-render",
    "dereth-desktop",
    "dereth-console",
    "dereth-input",
    "dereth-client-shell",
    "dereth-ui-screens",
];

/// One workspace package: its name, its folder from the workspace's root, and what its manifest
/// declares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspacePackage {
    pub name: String,
    pub dir: String,
    pub deps: Vec<ManifestDep>,
}

fn under(dir: &str, root: &str) -> bool {
    dir == root || dir.starts_with(&format!("{root}/"))
}

/// Rule: Lyceum is a product of its own. A crate under `lyceum/` declares no workspace crate but
/// the core's (under `core/`), Lyceum's own and [`LYCEUM_MAY_USE`]: never one of Empyrean's, which
/// it runs as a program, nor the client's application beyond those; and no crate outside
/// `lyceum/` declares one of Lyceum's. Tests' dependencies count too.
pub fn lyceum_violations(packages: &[WorkspacePackage]) -> Vec<String> {
    let dir_of = |name: &str| {
        packages
            .iter()
            .find(|p| p.name == name)
            .map(|p| p.dir.as_str())
    };
    let mut out = Vec::new();
    for p in packages {
        let ours = under(&p.dir, "lyceum");
        for d in &p.deps {
            let Some(dir) = dir_of(&d.name) else {
                continue;
            };
            let theirs = under(dir, "lyceum");
            let test = if d.dev { " (for its tests)" } else { "" };
            if ours && !theirs && !under(dir, "core") && !LYCEUM_MAY_USE.contains(&d.name.as_str())
            {
                out.push(format!(
                    "{} depends on `{}`{test}: Lyceum may use the core's crates and {} only{}",
                    p.name,
                    d.name,
                    LYCEUM_MAY_USE.join(", "),
                    if under(dir, "empyrean") {
                        "; it runs Empyrean as a program and never links it"
                    } else {
                        ""
                    }
                ));
            }
            if !ours && theirs {
                out.push(format!(
                    "{} depends on Lyceum's `{}`{test}: nothing outside lyceum/ may",
                    p.name, d.name
                ));
            }
        }
    }
    dedup(out)
}

/// The relay carries the 1999 dialect so that nothing else has to.
const RELAY: &str = "empyrean/relay";

/// Rule: the relay stands apart. No crate under `core/` or `dereth/` declares it, tests included,
/// so the client's tree never carries the 1999 dialect.
pub fn relay_violations(packages: &[WorkspacePackage]) -> Vec<String> {
    let dir_of = |name: &str| {
        packages
            .iter()
            .find(|p| p.name == name)
            .map(|p| p.dir.as_str())
    };
    let mut out = Vec::new();
    for p in packages {
        if !under(&p.dir, "core") && !under(&p.dir, "dereth") {
            continue;
        }
        for d in &p.deps {
            if dir_of(&d.name).is_some_and(|dir| under(dir, RELAY)) {
                let test = if d.dev { " (for its tests)" } else { "" };
                out.push(format!(
                    "{} depends on the relay's `{}`{test}: core/ and dereth/ never carry its dialect",
                    p.name, d.name
                ));
            }
        }
    }
    dedup(out)
}

/// Every workspace package with its folder and its manifest's dependencies, from `cargo
/// metadata` (no resolution: every optional dependency and every target is listed).
fn workspace_packages(ws: &Path) -> Result<Vec<WorkspacePackage>, String> {
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
    Ok(packages
        .iter()
        .filter_map(|p| {
            let manifest = PathBuf::from(p["manifest_path"].as_str()?);
            let dir = manifest
                .parent()?
                .strip_prefix(ws)
                .ok()?
                .to_string_lossy()
                .replace('\\', "/");
            Some(WorkspacePackage {
                name: p["name"].as_str()?.to_owned(),
                dir,
                deps: p["dependencies"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|d| {
                        Some(ManifestDep {
                            name: d["name"].as_str()?.to_owned(),
                            dev: d["kind"].as_str() == Some("dev"),
                        })
                    })
                    .collect(),
            })
        })
        .collect())
}

/// The workspace's crate names, from `cargo metadata` (no resolution).
fn workspace_members(ws: &Path) -> Result<Vec<String>, String> {
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
    Ok(packages
        .iter()
        .filter_map(|p| p["name"].as_str().map(str::to_owned))
        .collect())
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
    let plugin_lua_code = Ok(plugin_lua_code_violations(&crate_src(
        &ws,
        "dereth-plugin-lua",
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
    let plugin_api = cargo_tree(&ws, "dereth-plugin-api", "normal")
        .map(|t| plugin_layer_violations("dereth-plugin-api", &t, PLUGIN_API_DIRECT));
    let plugin_package = cargo_tree(&ws, "dereth-plugin-package", "normal")
        .map(|t| plugin_layer_violations("dereth-plugin-package", &t, PLUGIN_PACKAGE_DIRECT));
    let plugin_wasm = cargo_tree(&ws, "dereth-plugin-wasm", "normal")
        .map(|t| plugin_layer_violations("dereth-plugin-wasm", &t, PLUGIN_WASM_DIRECT));
    let plugin_lua = cargo_tree(&ws, "dereth-plugin-lua", "normal")
        .map(|t| plugin_layer_violations("dereth-plugin-lua", &t, PLUGIN_LUA_DIRECT));
    let plugin_host = cargo_tree(&ws, "dereth-plugin-host", "normal").map(|t| {
        let mut v = plugin_host_violations(&t, &crates);
        v.extend(code_violations(
            &crate_sources(&ws, "dereth-plugin-host"),
            CORE_NAMES_NOT,
        ));
        v
    });
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
    // Rule 12: the web page runs the launcher's page and keeps no copy of it.
    let launcher_page = launcher_copies(&ws);
    let lyceum = workspace_packages(&ws).map(|p| lyceum_violations(&p));
    let web_modules = WEB_MODULES
        .iter()
        .map(|(p, not)| feature_tree(&ws, p).map(|t| web_module_violations(p, not, &t)))
        .try_fold(Vec::new(), |mut all, v| {
            all.extend(v?);
            Ok::<_, String>(all)
        });
    let relay = workspace_packages(&ws).map(|p| relay_violations(&p));
    // Rule 14: the remaster changes only pixels.
    let remaster_rows = remaster::reports(&ws, &workspace_rs, &row);
    vec![
        row(
            "seam: web modules",
            web_modules,
            "the launcher's module holds neither game, and each game's holds its own alone"
                .to_owned(),
        ),
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
            "seam: the web page keeps no copy of the launcher's",
            launcher_page,
            format!("no tracked file of {WEB_PAGE} named as or the same as one of {LAUNCHER_UI}"),
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
            "seam: plugin-api deps",
            plugin_api,
            format!(
                "direct deps within {}; no other dereth-* or platform crate",
                PLUGIN_API_DIRECT.join(", ")
            ),
        ),
        row(
            "seam: plugin-package deps",
            plugin_package,
            format!(
                "direct deps within {}; no other dereth-* or platform crate",
                PLUGIN_PACKAGE_DIRECT.join(", ")
            ),
        ),
        row(
            "seam: plugin-wasm deps",
            plugin_wasm,
            format!(
                "direct deps within {}; no other dereth-* or platform crate",
                PLUGIN_WASM_DIRECT.join(", ")
            ),
        ),
        row(
            "seam: plugin-lua deps",
            plugin_lua,
            format!(
                "direct deps within {}; no other dereth-* or platform crate",
                PLUGIN_LUA_DIRECT.join(", ")
            ),
        ),
        row(
            "seam: plugin-lua unsafe",
            plugin_lua_code,
            format!("unsafe only in {PLUGIN_LUA_UNSAFE_FILE}, compiled for wasm32 only"),
        ),
        row(
            "seam: plugin-host names no client crate",
            plugin_host,
            "nothing under dereth/client/ in its tree or its code".to_owned(),
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
            "seam: Lyceum",
            lyceum,
            format!(
                "lyceum/ uses the core's crates and {} only, never Empyrean's, and nothing \
                 outside it uses lyceum/",
                LYCEUM_MAY_USE.join(", ")
            ),
        ),
        row(
            "seam: the relay",
            relay,
            "nothing in core/ or dereth/ depends on empyrean/relay".to_owned(),
        ),
    ]
    .into_iter()
    .chain(auberean::reports(&ws))
    .chain(remaster_rows)
    .collect()
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
    fn lyceum_uses_the_core_and_the_clients_drawing_only_and_nothing_else_uses_it() {
        let dep = |name: &str, dev: bool| ManifestDep {
            name: name.to_owned(),
            dev,
        };
        let pkg = |name: &str, dir: &str, deps: Vec<ManifestDep>| WorkspacePackage {
            name: name.to_owned(),
            dir: dir.to_owned(),
            deps,
        };
        let base = || {
            vec![
                pkg("dereth-dat", "core/dat", vec![]),
                pkg("dereth-scene", "dereth/client/crates/scene", vec![]),
                pkg("dereth-client", "dereth/client", vec![]),
                pkg("empyrean-world", "empyrean/crates/world", vec![]),
                pkg(
                    "lyceum-foundry",
                    "lyceum/crates/foundry",
                    vec![dep("dereth-dat", false)],
                ),
                pkg(
                    "lyceum-editor",
                    "lyceum/editor",
                    vec![
                        dep("lyceum-foundry", false),
                        dep("dereth-scene", false),
                        dep("egui", false),
                    ],
                ),
            ]
        };
        assert!(lyceum_violations(&base()).is_empty());
        // The client application, and Empyrean even for tests.
        let mut w = base();
        w[5].deps.push(dep("dereth-client", false));
        w[4].deps.push(dep("empyrean-world", true));
        let v = lyceum_violations(&w);
        assert_eq!(v.len(), 2, "{v:#?}");
        assert!(v[0].contains("lyceum-editor depends on `dereth-client`"));
        assert!(v[1].contains("lyceum-foundry depends on `empyrean-world` (for its tests)"));
        assert!(v[1].contains("never links it"));
        // Something outside depending on Lyceum.
        let mut w = base();
        w[2].deps.push(dep("lyceum-foundry", false));
        let v = lyceum_violations(&w);
        assert_eq!(v.len(), 1);
        assert!(v[0].contains("dereth-client depends on Lyceum's `lyceum-foundry`"));
    }

    #[test]
    fn nothing_in_core_or_the_client_tree_depends_on_the_relay() {
        let pkg = |name: &str, dir: &str, deps: Vec<(&str, bool)>| WorkspacePackage {
            name: name.to_owned(),
            dir: dir.to_owned(),
            deps: deps
                .into_iter()
                .map(|(name, dev)| ManifestDep {
                    name: name.to_owned(),
                    dev,
                })
                .collect(),
        };
        let relay = || {
            pkg(
                "empyrean-relay",
                "empyrean/relay",
                vec![("dereth-protocol", false)],
            )
        };
        let clean = [
            relay(),
            pkg("dereth-protocol", "core/protocol", vec![]),
            pkg(
                "empyrean-testkit",
                "empyrean/testkit",
                vec![("empyrean-relay", true)],
            ),
        ];
        assert!(relay_violations(&clean).is_empty());
        let v = relay_violations(&[
            relay(),
            pkg(
                "dereth-protocol",
                "core/protocol",
                vec![("empyrean-relay", false)],
            ),
            pkg(
                "dereth-headless",
                "dereth/headless",
                vec![("empyrean-relay", true)],
            ),
        ]);
        assert_eq!(v.len(), 2, "{v:#?}");
        assert!(v
            .iter()
            .any(|l| l.contains("dereth-protocol depends on the relay's `empyrean-relay`")));
        assert!(v.iter().any(|l| {
            l.contains("dereth-headless depends on the relay's `empyrean-relay` (for its tests)")
        }));
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
    fn the_plugin_crates_keep_to_their_layers() {
        let api = tree(&[
            (0, "dereth-plugin-api"),
            (1, "dereth-primitives"),
            (2, "thiserror"),
        ]);
        assert!(plugin_layer_violations("dereth-plugin-api", &api, PLUGIN_API_DIRECT).is_empty());
        let planted = tree(&[
            (0, "dereth-plugin-api"),
            (1, "dereth-primitives"),
            (1, "serde"),
        ]);
        assert_eq!(
            plugin_layer_violations("dereth-plugin-api", &planted, PLUGIN_API_DIRECT).len(),
            1
        );
        let lua = tree(&[
            (0, "dereth-plugin-lua"),
            (1, "dereth-plugin-api"),
            (2, "dereth-primitives"),
            (1, "mlua"),
            (1, "web-time"),
        ]);
        assert!(plugin_layer_violations("dereth-plugin-lua", &lua, PLUGIN_LUA_DIRECT).is_empty());
        let reaching = tree(&[
            (0, "dereth-plugin-lua"),
            (1, "dereth-plugin-api"),
            (2, "dereth-client-model"),
        ]);
        assert_eq!(
            plugin_layer_violations("dereth-plugin-lua", &reaching, PLUGIN_LUA_DIRECT).len(),
            1
        );
        let host = tree(&[
            (0, "dereth-plugin-host"),
            (1, "dereth-client-runtime"),
            (2, "dereth-ui"),
        ]);
        assert_eq!(
            plugin_host_violations(&host, &["dereth-ui".to_owned()]).len(),
            1
        );
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
    fn the_plugin_engine_allows_unsafe_in_its_browser_runtime_only() {
        let lib = format!("#![deny(unsafe_code)]\n\n{PLUGIN_LUA_UNSAFE_DECL}\nuse x;\n");
        let clean = vec![
            (PathBuf::from("core/plugin-lua/src/lib.rs"), lib.clone()),
            (
                PathBuf::from("core/plugin-lua/src/wasm_rt.rs"),
                "pub unsafe extern \"C\" fn malloc() {}\n".to_owned(),
            ),
        ];
        assert!(plugin_lua_code_violations(&clean).is_empty());
        let stray = vec![
            (PathBuf::from("core/plugin-lua/src/lib.rs"), lib),
            (
                PathBuf::from("core/plugin-lua/src/tests.rs"),
                "fn f() { unsafe { g() } }\n".to_owned(),
            ),
        ];
        assert_eq!(plugin_lua_code_violations(&stray).len(), 1);
        let ungated = vec![(
            PathBuf::from("core/plugin-lua/src/lib.rs"),
            "#![deny(unsafe_code)]\n#[allow(unsafe_code)]\npub mod wasm_rt;\n".to_owned(),
        )];
        assert_eq!(plugin_lua_code_violations(&ungated).len(), 1);
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

    /// A module that takes in another's crate is found, by its name or its prefix.
    #[test]
    fn a_web_module_that_holds_another_game_is_found() {
        let not = ["dereth-client-shell", "auberean-"];
        assert_eq!(
            web_module_violations(
                "dereth-web",
                &not,
                "dereth-web v0.1.0\ndereth-primitives v0.0.0\nserde v1.0.0"
            ),
            Vec::<String>::new()
        );
        let found = web_module_violations(
            "dereth-web",
            &not,
            "dereth-web v0.1.0\ndereth-client-shell v0.0.0\nauberean-shell v0.0.0 (*)\nauberean-shell v0.0.0",
        );
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(
            found.iter().any(|f| f.contains("dereth-client-shell")),
            "{found:?}"
        );
        assert!(
            found.iter().any(|f| f.contains("auberean-shell")),
            "{found:?}"
        );
    }

    /// A copy of the launcher's page under the web page's folder fails, by its name or by its
    /// bytes; the web page's own files, its own `index.html` among them, do not.
    #[test]
    fn a_copy_of_the_launchers_page_in_the_web_pages_folder_is_found_by_name_or_bytes() {
        let file = |path: &str, bytes: &str| (PathBuf::from(path), bytes.as_bytes().to_vec());
        let ui = vec![
            file("launcher/ui/app.js", "the page's script"),
            file("launcher/ui/index.html", "the launcher's shell"),
            file("launcher/ui/style.css", "the page's look"),
        ];
        let own = vec![
            file("dereth/web/www/index.html", "the web page's own shell"),
            file("dereth/web/www/host.js", "what a browser does its own way"),
        ];
        assert!(launcher_copy_violations(&own, &ui).is_empty());
        for (planted, why) in [
            (file("dereth/web/www/app.js", "edited since"), "named as"),
            (file("dereth/web/www/launcher.html", "edited"), "named as"),
            (
                file("dereth/web/www/launcher.css", "the page's look"),
                "a copy of",
            ),
            (
                file("dereth/web/www/old/shell.html", "the launcher's shell"),
                "a copy of",
            ),
        ] {
            let mut tree = own.clone();
            tree.push(planted);
            let hits = launcher_copy_violations(&tree, &ui);
            assert_eq!(hits.len(), 1, "{hits:?}");
            assert!(hits[0].contains(why), "{hits:?}");
        }
    }

    /// The check reads what git tracks: the copies the build leaves in the web page's folder, which
    /// git ignores, are not the tree's.
    #[test]
    fn the_copies_the_build_leaves_in_the_web_pages_folder_are_not_found() {
        let repo = std::env::temp_dir().join(format!("seams-web-page-{}", std::process::id()));
        std::fs::remove_dir_all(&repo).ok();
        std::fs::create_dir_all(repo.join(WEB_PAGE)).expect("a folder");
        std::fs::create_dir_all(repo.join(LAUNCHER_UI)).expect("a folder");
        let git = |args: &[&str]| {
            let ok = std::process::Command::new("git")
                .arg("-C")
                .arg(&repo)
                .args(args)
                .status()
                .expect("git runs")
                .success();
            assert!(ok, "git {args:?}");
        };
        git(&["init", "-q"]);
        std::fs::write(repo.join(LAUNCHER_UI).join("app.js"), "script").expect("write");
        std::fs::write(repo.join(WEB_PAGE).join("app.js"), "script").expect("write");
        git(&["add", "launcher/ui/app.js"]);
        let built = launcher_copies(&repo);
        git(&["add", "dereth/web/www/app.js"]);
        let tracked = launcher_copies(&repo);
        std::fs::remove_dir_all(&repo).ok();
        assert_eq!(built, Ok(Vec::new()));
        assert_eq!(tracked.expect("a work tree").len(), 1);
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
}
