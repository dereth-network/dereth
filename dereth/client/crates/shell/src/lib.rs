//! The client's front end over the runtime: the retail UI, the HUD's panels, the cursor, the
//! clipboard bridge, the device input and the renderer that draws the UI over the scene, generic
//! over the platform under it.
//!
//! **Depends on** the runtime and what it is built from (`dereth-client-runtime`,
//! `dereth-client-model`, `dereth-client-contract`, `dereth-client-net`, `dereth-protocol`,
//! `dereth-primitives`, `dereth-dat`, `dereth-assets`, `dereth-physics`, `dereth-animation`,
//! `dereth-audio`), the drawn world (`dereth-scene`, with `dereth-render` and `dereth-world-render`),
//! and the UI crates (`dereth-ui`, `dereth-ui-screens`, `dereth-input`). **Used by** the desktop
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
//! confirmations. It re-exports the runtime's and the scene's modules its own code and its users
//! name at their old paths.

pub use dereth_client_runtime::allegiance_view;
pub use dereth_client_runtime::anim_assets;
pub use dereth_client_runtime::anim_hooks;
pub use dereth_client_runtime::trade_view;
pub use dereth_client_runtime::vendor_view;
pub mod app;
pub mod front_end;
pub use dereth_client_runtime::assets;
/// The sound model, the runtime's; the host supplies the output device.
pub use dereth_client_runtime::audio;
pub use dereth_client_runtime::character;
pub use dereth_client_runtime::chat;
/// The camera: the scene's, at its old path.
pub use dereth_scene::camera;
// The host-clipboard seam and the bridge that mirrors it into the UI each frame.
pub mod clipboard;
/// The command line and preferences file, re-exported from [`dereth_client_runtime::config`].
pub use dereth_client_runtime::config;
pub use dereth_client_runtime::connect_failure;
pub use dereth_client_runtime::corestrings;
pub mod cursor;
pub use dereth_client_runtime::ddd;
pub use dereth_client_runtime::dropped;
pub use dereth_client_runtime::env_cells;
pub use dereth_client_runtime::frame;
pub use dereth_client_runtime::frame_events;
/// The renderer: the scene's, and the UI drawn over it.
pub mod gpu;
pub mod hud;
/// The HUD's panel driver: every `Hud` entry point that takes a live `&mut dereth_ui::UiSystem`,
/// kept apart from `hud.rs` so the model half names no UI system at all.
pub mod hud_drive;
/// The device input: the input manager with the client's registrations and the keymap store, and
/// the hand-off of what it produces to the runtime as actions.
pub mod input;
pub use dereth_client_runtime::interaction;
pub use dereth_client_runtime::jump;
pub use dereth_client_runtime::land_source;
pub use dereth_client_runtime::models;
pub use dereth_client_runtime::movement;
pub use dereth_client_runtime::net;
pub use dereth_client_runtime::object_identity;
pub use dereth_client_runtime::object_physics;
pub use dereth_client_runtime::object_range;
pub use dereth_client_runtime::object_step;
pub use dereth_client_runtime::objects;
/// Building compressed textures' mip chains off the main thread: the scene's.
pub use dereth_scene::mip_worker;
/// Presentation: the scene's particles.
#[cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]
pub use dereth_scene::particles;
/// The persistence files, opened. The types and text formats are
/// `dereth_client_contract::persist`; the path branch and the reads and writes, through the
/// runtime's file seam, are here.
pub mod persist;
pub use dereth_client_runtime::pick;
// What the shell needs from the platform under it (`platform::host::Host`), and the window
// events and key identities every platform shares.
pub mod platform;
/// The presentation: the runtime's device seam and what the UI adds to it.
pub mod present;
/// The paper-doll preview space: the scene's.
pub use dereth_scene::preview;
/// The message pump: host events in, Win32 messages out, the window procedure in the middle.
pub mod pump;
/// The `Render.*` preference owners, over the scene's.
pub mod render_prefs;
pub use dereth_client_runtime::selection_geometry;
/// The answers a headless client with no server gives itself.
pub use dereth_client_runtime::server_stub;
pub use dereth_client_runtime::shutdown;
/// The sky: the scene's.
pub use dereth_scene::sky;
mod target_confirmation;
pub use dereth_client_runtime::teleport;
/// The dat texture lookup: the scene's.
pub use dereth_scene::textures;
pub mod trace;
pub mod ui;
pub mod ui_draw;
pub use dereth_client_runtime::world_stream;
/// `world_scene` at its short path.
pub use dereth_scene::world;
/// The drawn world: the scene's.
pub use dereth_scene::world_scene;

pub use config::Config;
