//! The web front page's launcher: the desktop launcher's own logic in a small WebAssembly module of
//! its own, which the page loads first and which holds neither game.
//!
//! The page loads it when it opens, and starts the module of
//! whichever game a world plays only when that world is played, so a visit to the launcher fetches
//! neither game.
//!
//! **Depends on** `dereth-launch` (the launcher's logic), `dereth-web-store` (the client's own
//! files and a world's overlay in browser storage), `dereth-client-net` and
//! `dereth-client-runtime` (how the client names a world's overlay, and reads its blocklist),
//! `dereth-dat`, `dereth-primitives` and `serde_json`. The WebAssembly build adds `wasm-bindgen`
//! and `js-sys`. **Used by** the worker under `dereth/web/www/`, which loads it.
//!
//! **Must never** hold either game: the game's own crates stay out of its tree, which is what keeps
//! it small, and `cargo xtask seams` holds that.
//!
//! The browser half, `browser`, compiles only for `wasm32`. Everything else here is
//! platform-neutral and tested on the host.

pub mod front;
pub mod plugin_sources;
pub mod server_url;

#[cfg(target_arch = "wasm32")]
pub mod browser;
