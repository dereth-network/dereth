//! The read-only view onto the game, the request channel back out and the action vocabulary, with
//! no presentation and no platform in it.
//!
//! **Depends on** `dereth-primitives` (plus `glam` and `raw-window-handle`). **Used by** everything
//! on either side of the seam between the game and what draws it: the object model and the runtime
//! (`dereth-client-model`, `dereth-client-runtime`), the SDK, the drawing and UI crates
//! (`dereth-render`, `dereth-render-cpu`, `dereth-ui`, `dereth-ui-screens`), the device input
//! (`dereth-input`), the client and its test kit.
//!
//! **Must never** reach another `dereth-*` crate or a presentation or platform crate: its direct
//! production dependencies are exactly `dereth-primitives`, `glam` and `raw-window-handle`, and
//! nothing in its tree is a UI, a renderer, `winit` or `windows` (`cargo xtask seams`, `seam:
//! dereth-client-contract deps`; the allowed list is the seam check's own, so adding a dependency
//! is a reviewed change there). It does no file I/O either: the persistence formats are here, the
//! reading and writing is the client's.
//!
//! [`GameView`], [`UiRequest`] and the `…View` value structs ([`view`], glob-re-exported here) are
//! the contract between whoever holds the world and whoever draws it; both sides depend on this
//! crate instead of on each other. [`ids`] holds [`ElementId`] and [`UiMode`]; [`snapshot`] the
//! owned [`GameSnapshot`], for a view that cannot borrow; [`persist`] the UI's persistence types
//! and text formats; and [`actions`] the retail action ids, their phase and their names, which a
//! front end's device input produces and the runtime consumes.

#![forbid(unsafe_code)]

/// The action vocabulary: the retail action ids, their phase and their names. The runtime takes
/// actions; a front end's device pipeline makes them.
pub mod actions;
/// The projection parameters `ViewParams` carries. `dereth_render_cpu::camera` re-exports them.
pub mod camera;
/// The chat seam shared with `dereth_ui_screens::chat`: the failure table,
/// `ChatMessage`, the window ids and opacity attributes, the reply targets and the auto-target
/// question. `dereth_client::hud` fills every one of them.
pub mod chat;
pub mod combat_mode;
/// The skill the combat window's melee arm gates its meter on.
pub mod combat_notice;
/// The gameplay reason a confirmation dialog is on screen, shared by the object model and the
/// dialog engine.
pub mod confirmation;
pub mod ctime;
/// What a disconnect says, whatever UI shows it.
pub mod disconnect;
pub mod era;
/// `PlayerModule`'s window-placement blob, shared with `dereth_ui_screens::hud::floaty`.
/// `dereth_client::hud` owns the blob.
pub mod floaty;
/// The gameplay screen's 3D viewport element id.
pub mod gameplay;
pub mod ids;
/// `InputPump` and `NullInputPump`, which `dereth_ui` re-exports. `dereth-ui` calls the trait and
/// `dereth_client::input` implements it, so it belongs under both rather than in one of them.
pub mod input;
pub mod journal;
pub mod linkstatus;
/// The shared UI easing table, which `dereth_ui::media` re-exports.
pub mod media;
/// The inbound notice queue, shared with `dereth_ui_screens::notices`. `MagicNotice` is
/// [`view`]'s; the inbox is a plain value
/// with no per-thread state.
pub mod notices;
/// The option *data*: the preference value store, the choice tables and the two const tables it
/// registers from. `dereth_ui_screens::options` re-exports each item.
pub mod options;
/// The overlay any UI draws over the world, as a presentation draws it.
pub mod overlay;
/// The panel seam shared with `dereth_ui_screens::panels`: the property
/// keys, tables, notices and formulas the world half of the client has to agree with a panel
/// about. The panels themselves stay in the UI.
pub mod panels;
pub mod persist;
/// The power bar's mode, shared by the combat state and the power-bar widgets.
pub mod powerbar;
/// The pre-game view the host hands the pre-game screens each frame, and the player-session
/// operations they hand back.
pub mod pregame;
/// The radar's two bit vocabularies and its range, shared with
/// `dereth_ui_screens::mapradar::radar`. The world half and the radar must name the same mask; this
/// is where they meet.
pub mod radar;
/// `RendererChoice`, the backend a player asked for, split off from
/// `dereth_render::device::Backend`, which keeps the capability half and converts from this one.
pub mod renderer;
/// The outbound request queue, shared with `dereth_ui_screens::requests`. `UiRequest` is
/// [`view`]'s; the outbox is a plain value
/// with no per-thread state.
pub mod requests;
pub mod research;
/// `STARTUP_FILTERING`, which `dereth_render_cpu::sampler` re-exports.
pub mod sampler;
pub mod snapshot;
pub mod spellbook;
pub mod statmgmt;
/// Where the selected object stands on screen.
pub mod target;
/// The world-view teleport / portal animation model, which
/// `dereth_ui_screens::screens::teleport` re-exports.
pub mod teleport;
pub mod view;
/// `WindowHandles`, `Rect` and `ScreenMetrics`: the plain data a host window contributes.
/// `dereth_render::{device, window_proc}` re-export each.
pub mod window;
/// The window message table, the frame sleep and the placement arithmetic.
pub mod window_proc;

pub use ctime::UtcOffsetSecs;
pub use ids::{ElementId, UiMode};
pub use input::{InputPump, NullInputPump};
pub use journal::{create_journal_path, JournalIdentity, STEM as JOURNAL_STEM};
pub use persist::{
    CharacterIdentity, CharacterSet, PersistError, SavedWindow, ScreenLayout, UiPersistentData,
    UserPreferences,
};
pub use renderer::RendererChoice;
pub use snapshot::GameSnapshot;
pub use statmgmt::XpHeader;
pub use view::*;
