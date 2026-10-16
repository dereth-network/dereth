//! The server the page connects to: a WebSocket URL, and the rule for which ones may be used.
//!
//! The client connects to a server's WebSocket endpoint directly (`wss://`), or to
//! `dereth-web-relay` on this machine (`ws://127.0.0.1:<port>/`) for a server that speaks only UDP;
//! the frame is the same either way. The game protocol is not encrypted (its key stream only keys
//! the checksums, and the login carries the account's password), so a plain `ws://` URL is refused
//! for anything but the loopback address.

/// The client's own numbering of the server's two ports: the frame's ports are relative (the first
/// datagram's is the logon port), so every server is addressed at this one.
pub const NOMINAL_SERVER: &str = "127.0.0.1:9000";

/// Why `url` cannot be the server, or `None` when it can: `wss://` to any host, `ws://` only to the
/// loopback address. The launcher's own rule ([`dereth_launch::direct`]), which every address a
/// world is reached at is held to, whoever names it.
#[must_use]
pub fn problem(url: &str) -> Option<String> {
    dereth_launch::direct::problem(url)
}
