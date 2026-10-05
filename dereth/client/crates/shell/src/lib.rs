//! The client's front end over the runtime: the retail UI, the HUD's panels, the cursor, the
//! clipboard bridge, the device input and the renderer that draws the UI over the scene, generic
//! over the platform under it.
//!
//! **Depends on** the runtime and what it is built from (`dereth-client-runtime`,
//! `dereth-client-model`, `dereth-client-contract`, `dereth-client-net`, `dereth-protocol`, `dereth-rules`,
//! `dereth-primitives`, `dereth-chargen`, `dereth-dat`, `dereth-assets`, `dereth-physics`, `dereth-animation`,
//! `dereth-audio`, `dereth-world-data`), the drawn world (`dereth-scene`, with `dereth-render` and `dereth-world-render`),
//! the UI crates (`dereth-ui`, `dereth-ui-screens`, `dereth-input`), and the classic interface it
//! runs when that is chosen (`dereth-classic-ui`, with `dereth-classic-dat` for its portal and its
//! fonts). **Used by** the desktop
//! client (`dereth-client`) and the browser client (`dereth-web`).
//!
//! **Must never** reach a platform itself: no window system, no operating system call and no sound
//! device (`cargo xtask seams`, `seam: application halves`). Everything platform-specific arrives
//! through [`platform::host::Host`], which the application is generic over: the window and its
//! clock, the time zone, the URL launch, the sound output, the clipboard and the cursor images.
//! Its code reaches the concrete screens only through the UI framework's hooks, never by name
//! (`seam: host -> screens`).
//!
//! The frame loop is the runtime's. What this crate owns is what plugs into it: the front end and
//! the application with it ([`app::ClientShell`], [`app::App`]), the retail UI ([`ui`],
//! [`ui_draw`], [`hud`], [`hud_drive`]), the renderer that draws the UI over the scene ([`gpu`],
//! [`present`]), the cursor ([`cursor`]), the clipboard bridge ([`clipboard`]), the device input
//! ([`input`], [`pump`]), the layout and keymap files ([`persist`]) and the targeted-use
//! confirmations. Runtime and scene types are named through their owning crates.

pub mod app;
pub mod front_end;

// The host-clipboard seam and the bridge that mirrors it into the UI each frame.
pub mod clipboard;

pub mod cursor;

/// The renderer: the scene's, and the UI drawn over it.
pub mod gpu;
pub mod hud;
/// The HUD's panel driver: every `Hud` entry point that takes a live `&mut dereth_ui::UiSystem`,
/// kept apart from `hud.rs` so the model half names no UI system at all.
pub mod hud_drive;
/// The device input: the input manager with the client's registrations and the keymap store, and
/// the hand-off of what it produces to the runtime as actions.
pub mod input;

/// The persistence files, opened. The types and text formats are
/// `dereth_client_contract::persist`; the path branch and the reads and writes, through the
/// runtime's file seam, are here.
pub mod persist;

// What the shell needs from the platform under it (`platform::host::Host`), and the window
// events and key identities every platform shares.
/// The classic interface in the retail interface's place, when it is chosen.
mod classic_face;
pub mod platform;
/// The presentation: the runtime's device seam and what the UI adds to it.
pub mod present;

/// The message pump: host events in, Win32 messages out, the window procedure in the middle.
pub mod pump;
/// The `Render.*` preference owners, over the scene's.
pub mod render_prefs;

mod target_confirmation;

pub mod trace;
pub mod ui;
pub mod ui_draw;
