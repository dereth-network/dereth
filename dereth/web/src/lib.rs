//! The Dereth client in a web browser: the client application in a Web Worker, reading the
//! player's data files on the player's own machine and reaching the server over a WebSocket,
//! directly or through a relay on the player's machine.
//!
//! The application is the client shell's whole application, drawing into the page's canvas on the
//! graphics device's `wgpu` backend (WebGPU, or WebGL 2 where the browser has no WebGPU). The
//! WebSocket is a server's own endpoint, or `dereth-web-relay` on the player's machine for a
//! server that speaks only UDP.
//!
//! **Depends on** the application (`dereth-client-shell`, over `dereth-scene`), the graphics
//! device on its `wgpu` backend (`dereth-render`), and what its host names by crate:
//! `dereth-client-runtime`, `dereth-client-contract`, `dereth-client-net`, `dereth-client-sdk`
//! (the data store), `dereth-dat`, `dereth-primitives`, `dereth-ui-screens` and `dereth-input`;
//! `dereth-transport` for the WebSocket frame; and `dereth-classic-dat` and `dereth-classic-fonts`
//! for the classic interface's text, drawn from the fonts the client carries. The WebAssembly build adds `wasm-bindgen`,
//! `js-sys`, `web-sys` and a `tracing-subscriber` that writes to the browser console. **Used by**
//! the page and worker under `www/`, which load it as a WebAssembly module.
//!
//! **Must never** read the player's data files from anywhere but the browser on the player's own
//! machine (the dev runner that can stand in for browser storage binds `127.0.0.1` only), reach a
//! network except over the one WebSocket to the server the player chose, or send the game's
//! unencrypted protocol over plain `ws://` to anything but this machine ([`server_url`]).
//!
//! The browser half, `browser`, compiles only for `wasm32`: the data files arrive through a
//! synchronous read the worker answers (`dats::BrowserFile`), datagrams cross the WebSocket with a
//! two-byte port in front ([`frame`]), and [`play`] runs the client shell's whole application on
//! the browser's host ([`host`]), one frame per animation frame. Everything else here is
//! platform-neutral and tested on the host.

pub mod dats;
pub mod frame;
pub mod host;
pub mod keycodes;
pub mod play;
pub mod server_url;
pub mod settings;

#[cfg(target_arch = "wasm32")]
pub mod browser;
