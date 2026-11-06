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
//! `dereth-web-store` for the client's own files and a world's overlay in browser storage;
//! `dereth-transport` for the WebSocket frame; `dereth-classic-dat` and `dereth-classic-fonts`
//! for the classic interface's text, drawn from the fonts the client carries; and `dereth-horizon`
//! for the Horizon interface's art, which the page fetches from beside the module. The WebAssembly build adds `wasm-bindgen`,
//! `js-sys`, `web-sys` and a `tracing-subscriber` that writes to the browser console. **Used by**
//! the worker under `www/`, which loads it as a WebAssembly module when a world of Asheron's Call
//! is played.
//!
//! **Must never** read the player's data files from anywhere but the browser on the player's own
//! machine (the dev runner that can stand in for browser storage binds `127.0.0.1` only), reach a
//! network except over the one WebSocket to the server the player chose, or send the game's
//! unencrypted protocol over plain `ws://` to anything but this machine (the launcher holds that
//! rule, and refuses such a world before this module is loaded).
//!
//! The launcher is a module of its own, which this one knows nothing of. The browser half, `browser`,
//! compiles only for `wasm32`: the data files arrive through a
//! synchronous read the worker answers (`dats::BrowserFile`), a world's overlay is kept in the
//! browser's own storage through the same kind of calls (`overlay`), datagrams cross the
//! WebSocket with a two-byte port in front ([`frame`]), and [`play`] runs the client shell's whole
//! application on the browser's host ([`host`]), one frame per animation frame, with the plugins
//! the player added to the page ([`plugins`]). Everything else here is platform-neutral and
//! tested on the host.

pub mod dats;
pub mod frame;
pub mod host;
pub mod keycodes;
pub mod play;
pub mod plugins;
pub use dereth_web_store::settings;

#[cfg(target_arch = "wasm32")]
pub mod browser;
#[cfg(target_arch = "wasm32")]
pub use dereth_web_store::overlay;
#[cfg(target_arch = "wasm32")]
pub mod plugin_wasm;

/// The server as the client addresses it: the frame's ports are relative (the first datagram's is
/// the logon port), so every server is addressed at this one, whatever it listens on.
pub const NOMINAL_SERVER: &str = "127.0.0.1:9000";
