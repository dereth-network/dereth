//! Auberean in a web browser: the client application in a Web Worker, reading the player's data
//! files on the player's own machine and reaching the server over one WebSocket. The page's
//! launcher starts it for a world of Asheron's Call 2's; this module holds only the game.
//!
//! **Depends on** the application (`auberean-shell`, with the game's own interface
//! `auberean-interface` and `auberean-ui`), its world and its device (`auberean-scene`, `auberean-draw`,
//! `auberean-gpu`, `auberean-present`), the client (`auberean-client-sdk`), `dereth-audio` for the
//! sound device's seam, `dereth-transport` for the WebSocket frame and `dereth-primitives` for the
//! file the client's own files are kept in. The WebAssembly build adds `wasm-bindgen`, `js-sys`,
//! `web-sys`, `wgpu` with WebGL 2, and a `tracing-subscriber` that writes to the browser console.
//! **Used by** the worker under `dereth/web/www/`, which loads it as a WebAssembly module.
//!
//! **Must never** read the player's data files from anywhere but the browser on the player's own
//! machine, or reach a network except over the one WebSocket to the server the player chose. The
//! launcher holds the rule for which servers may be used.
//!
//! The browser half, `browser`, compiles only for `wasm32`. Everything else here is
//! platform-neutral and tested on the host.

pub mod host;
pub mod keys;
pub mod size;
pub mod store;

#[cfg(target_arch = "wasm32")]
pub mod browser;
