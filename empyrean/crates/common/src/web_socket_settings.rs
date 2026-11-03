//! Not ACE: the WebSocket endpoint's settings (`empyrean.toml` → `[server.websocket]`), which both
//! game servers read alike and so live in their shared crate.
//!
//! DIVERGE: an extra section; ACE has no WebSocket endpoint.

pub use dereth_server_websocket::settings::{WebSocketSettings, WebSocketSettingsError};
