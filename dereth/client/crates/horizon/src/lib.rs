//! The Horizon interface: the client's third interface, beside the modern and the classic one, drawn
//! from its own art.
//!
//! The game underneath is the client's: the shared runtime's frame loop, network session, object
//! model and physics, the shared scene that draws the world, and the shared device input with its
//! key maps. What this crate adds is the interface: the title screen, the world select, the lobby
//! and character creation, the HUD and its windows ([`ui`]), drawn as textured quads through the
//! shared overlay ([`draw`]) from the interface's own art ([`art`], [`pieces`]), which the host
//! hands over: built in on the desktop, fetched from beside the module in a browser.
//!
//! **Depends on** the shared crates: `dereth-primitives`, `dereth-client-contract`,
//! `dereth-client-runtime`, `dereth-client-model`, `dereth-dat`, `dereth-assets`, `dereth-chargen`,
//! `dereth-rules`, `dereth-animation`, `dereth-presentation`, `dereth-client-net`, `dereth-input` and `dereth-plugin-api`
//! (the plugins' declared windows); the client's `dereth-ui-kit` (the widget kit it is drawn with),
//! `dereth-plugin-ui` (the plugins' windows, drawn with the same kit), `dereth-scene` (the game's
//! icons), `dereth-ui` (the game's text) and `dereth-ui-screens` (the game's words and rules that
//! the retail screens keep free of their widgets). **Used by** the client shell (`dereth-client-shell`), which shows it when the
//! player chooses it.
//!
//! **Must never** reach a window, a device or the platform: the shell hands it events and draws
//! its overlay.
//!
//! The pieces: [`runtime`] is the front end the shell drives ([`runtime::HorizonFrontEnd`]); [`ui`] is
//! the interface; [`state`] reads the game for it; [`dialogs`] shows the game's questions;
//! [`draw`] draws it; [`art`] reads the art; [`pieces`] are the interface's own art's files;
//! [`font`] holds the tables its text is drawn from; [`resample`] makes a picture at the size it is
//! drawn; [`scale`] says which interface scales a window offers and the one it is drawn at.

pub mod art;
pub mod dialogs;
pub mod draw;
pub mod font;
pub mod looks;
pub mod options;
pub mod pad;
pub mod pieces;
pub use dereth_ui_kit::resample;
pub mod retail_ui;
pub mod ring;
pub mod runtime;
pub mod scale;
pub mod state;
pub mod strings;
pub mod ui;
