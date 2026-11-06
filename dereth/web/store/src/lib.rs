//! What the web client keeps in browser storage for the game and the launcher alike: the client's
//! own files and a world's overlay.
//!
//! The client's own files are [`settings`], and a world's overlay, in the WebAssembly build,
//! `overlay`. The launcher's module reads and sets them on the front page; the game's module plays with them.
//!
//! **Depends on** `dereth-client-runtime` and `dereth-client-contract` (the runtime's file store,
//! the preferences and the overlay blocklist as the client reads them), `dereth-dat` (the overlay's
//! containers) and `dereth-primitives` (the store's file). The WebAssembly build adds
//! `wasm-bindgen`. **Used by** `dereth-web` and `dereth-web-front`.
//!
//! **Must never** hold anything of the game beyond those files: a module that uses this crate must
//! stay able to leave the game out.

pub mod settings;

#[cfg(target_arch = "wasm32")]
pub mod overlay;
