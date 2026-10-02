//! The client with no UI, no device and no window: the application state machine, its frame loop
//! and everything the frame needs that is not drawing or an OS call.
//!
//! **Depends on** the shared data and engine crates (`dereth-primitives`, `dereth-dat`,
//! `dereth-assets`, `dereth-world-data`, `dereth-landscape`, `dereth-terrain`, `dereth-physics`,
//! `dereth-animation`),
//! the wire (`dereth-transport`, `dereth-protocol`, `dereth-client-net`), the object model and
//! contract (`dereth-client-model`, `dereth-client-contract`) and the sound engine
//! (`dereth-audio`). **Used by** the SDK (`dereth-client-sdk`), the client (`dereth-client`) and
//! its test kit (`dereth-testkit`).
//!
//! **Must never** reach presentation, a device or a platform: its production tree holds no crate
//! under `dereth/client/crates/` and none of `winit`, `windows`, `cpal` or `ash`, and its code
//! names no drawing or UI crate (`cargo xtask seams`, `seam: client-runtime deps`, `seam:
//! client-runtime code`). Nor does it name a key, a key map, an input map or a device message
//! (`seam: action seam code`): what the player does arrives as actions. A module that cannot
//! compile without one of those belongs in `dereth-client`. It never prints and installs no log
//! subscriber: its progress and problems are `tracing` events, and the frame loop's spans, for
//! whichever subscriber the binary running it has installed (`seam: library logging`).
//!
//! What draws and what answers the player plugs in: a platform ([`app::Platform`]: the window, the
//! clock, the frame pacer), a presentation ([`present::Presentation`]: the device) and a front end
//! ([`shell::Shell`]: the UI and whatever else draws over the world). The runtime decides the
//! frame's order and calls each piece at its step; a test, a replay harness or a headless client
//! runs it with the null version of all three. [`actions`] is the input seam: an action id with a
//! phase, injected by a script or handed on by a front end's device input, run through the
//! movement, camera, combat and emote handlers here.

/// The core string table: its numbered strings, and the display names the client composes from
/// them.
pub mod corestrings;

/// The client's ordered teardown list, as plain state.
pub mod shutdown;

/// A rate limit for the console's state lines.
pub mod report_gate;

/// An `AnimAssets` over the retail portal dat: where the animation runtime meets the assets.
/// Lives in `dereth-world-data`; re-exported at its old path.
pub use dereth_world_data::anim_assets;

/// The six built-in display templates a speech line is composed with.
pub mod chat;

/// `ClientNetwork`: the UDP socket handed to `dereth_client_net::client_session`, and nothing else.
pub mod net;

/// What the player is shown when the first connection fails, and the exit that follows it.
pub mod connect_failure;

/// The device-free half of the client's platform seam: the window, the clock, the text
/// conversion, the error box, the audio endpoint and the other host answers.
pub mod platform;

/// The frame's step list: what `App::frame` did, in order.
pub mod frame;
/// The frame's event log. `App`'s ~30 test-only counters are one-line reads over it, and a
/// replay harness's golden event log is built on it.
pub mod frame_events;
pub mod perf;

/// Opening the four retail dats: the seam every other module reads assets through.
pub mod assets;

/// The setup / gfxobj model cache and the placement-frame ids.
pub mod models;

/// Which records two eras' portal files share by id are the same object in both.
pub mod object_identity;

/// The dropped-opcode ledger.
pub mod dropped;

/// The live diagnostic trace targets (`dereth::trace::*`) and their filter checks. The two
/// functions that read a live UI tree stay in `dereth_client::trace`.
pub mod trace;

/// `send_request`: the one place a `dereth_client_model::Request` becomes bytes on a session.
pub mod requests;

/// The landblock and region identities the landscape path is indexed by, and `WorldError`.
/// Everything but `block_shift` lives in `dereth-world-data`.
pub mod landblock;

/// `StartsTrue`, the named `bool` whose zero value is `true`.
pub mod flags;

// The object / character / physics chain.

/// A `dereth_physics::LandSource` over the retail cell dat. Lives in `dereth-world-data`;
/// re-exported at its old path.
pub use dereth_world_data::land_source;
/// The DDD data-cache patch path: receive a patch, validate it, put it in the dat.
pub mod ddd;
/// The interior cells of a landblock, wired onto both of their consumers. Lives in
/// `dereth-world-data`; re-exported at its old path.
pub use dereth_world_data::env_cells;
/// The free camera and the character camera. The four functions that take a
/// `crate::present::Scene` stay in `dereth_client::camera`, with the two tests whose oracle is
/// `dereth_render::camera::view_from_frame`.
pub mod camera;
/// The embodied character.
pub mod character;
/// Combat input's jump callbacks: the charge and its release.
pub mod jump;
/// The objects the server puts in the world, made solid.
pub mod object_physics;
/// Player-object range checks, client half.
pub mod object_range;
/// The objects the server puts in the world.
pub mod objects;
/// `WorldObjects`'s pick, and the geometry it sweeps. `impl PickScene for WorldScene` stays in
/// `dereth_client::pick_scene`, because `WorldScene` is `dereth-client`'s.
pub mod pick;
/// The pick's geometry: the pick ray, the sphere and polygon tests, and the sweep over the parts
/// a frame draws.
pub mod pick_geometry;
/// Per-object selection geometry, client half.
pub mod selection_geometry;

/// The teleport / portal animation, wired to the smart box. The model it drives is
/// `dereth_client_contract::teleport`.
pub mod teleport;

// The client's own preference state. The store is `dereth_client_contract::options` and the
// backend *selection* is `dereth_client_contract::RendererChoice`, so neither file needs the UI or
// the device.

/// The command line and the preferences file. `Config` is what `main` builds and
/// `App` is constructed from; `Preferences` is the `UserPreferences.ini` it reads.
pub mod config;
/// The fourteen `Render.*` preference names and their choice decodes. The three `apply_*`
/// functions that hold a `Gpu` stay in `dereth_client::render_prefs`.
pub mod render_prefs;

/// `SoundTrigger` alone. The rest of `dereth_client::audio` holds the `cpal` stream, so it stays
/// in `dereth-client`.
pub mod audio;

// The simulation, kept apart from the render scene: the per-object physics/animation step and the
// movement interpretation. The three modules are in name order.

/// The animation-hook drain and its counters.
pub mod anim_hooks;

/// The wire's movement buffer as the motion runtime calls it.
pub mod movement;

/// The per-object physics / animation step, and the simulation half of a scene object.
pub mod object_step;

/// The game model: the HUD model and its `GameView` provider, the interaction layer (selection, combat, use, the
/// `UiRequest` router), the shop/trade/allegiance projections, and the cursor decision's item-use
/// predicates. `dereth_client` re-exports each at its old path.
pub mod allegiance_view;
pub mod cursor;
/// The questions the game asks the player, whatever UI shows them.
pub mod dialogs;
pub mod hud;
pub mod interaction;
pub mod trade_view;
pub mod vendor_view;

/// The landscape residency window and normal-render viewpoint, exposed through the `WorldStreamer` the
/// scene owns and drives.
pub mod world_stream;

/// The Dereth calendar clock.
pub mod game_clock;
/// The simulation and residency half of the scene.
pub mod world_state;

/// A presentation with no device that still builds and drives the world.
pub mod sim_present;
/// Building the world with no device: the starting state, each landblock's simulation content and
/// its physics registration.
pub mod world_build;
/// The server's objects entering, changing and leaving the world, with no device.
pub mod world_objects;
/// One frame of the world's simulation with no device.
pub mod world_step;

/// The player system's process-owned landscape presets.
/// `dereth_client::world::EnvironmentOverrideState` resolves to it through a `pub use`.
pub mod environment;

/// The presentation seam: the device the frame draws through, the headless presentation, and the
/// drawn world as the frame reads and writes it.
pub mod present;

/// What the drawn world is built from.
pub mod scene;

/// The window procedure's state and the lifecycle messages that drive it.
pub mod pump;

/// The front end that plugs into the frame: the UI, the cursor, the clipboard, and whatever draws
/// over the world.
pub mod shell;

/// What a front end is handed at each step it takes part in.
pub mod ui_context;

/// The answers a headless client with no server gives itself, in the server's place.
pub mod server_stub;

/// The action seam: the actions the frame's handlers take, and the queue that carries them from
/// one stage of the frame to the next.
pub mod actions;

/// The application state machine and the frame loop.
pub mod app;
