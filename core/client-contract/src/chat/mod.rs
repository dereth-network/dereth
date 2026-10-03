//! The chat seam.
//!
//! `dereth-ui-screens` draws the chat windows; `dereth-client` owns what goes in them — the final-string
//! notice that carries a chat line into the UI is one of the three process globals the client owns, and the failure table turns
//! an inbound `WeenieError` into a line. Everything both sides have to agree about is here, at the
//! same module path as in `dereth_ui_screens::chat`, which re-exports each item.

/// The client's whole arm table and its two entry points.
pub mod failure;
/// `ChatInterface`'s window ids, one delivered line, and the two opacity attributes.
pub mod interface;
/// `MainChat`'s auto-target question and answer.
pub mod mainchat;
/// The three names the reply keys address.
pub mod window;

/// Chat presentation colours.
pub mod colors;

/// Shared entry intents and readback.
pub mod entry;
