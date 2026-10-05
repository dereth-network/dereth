//! The three names the reply keys address.
//!
//! Cut out of `dereth_ui_screens::chat::window`, which keeps `GamePlayScreen`'s chat window and its
//! key handling. `ReplyTargets` is the one value in it the *world* half fills:
//! `dereth_client_shell::hud` answers the last-teller, last-monarch-speaker and last-patron-speaker
//! name queries out of `dereth_client_model`'s chat state, at two sites, and the window only
//! reads it. Three `Option<String>`s.

/// The last teller, monarch, and patron names exposed by the communication system.
/// An empty answer is `None` and the key does nothing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReplyTargets {
    /// The last player to send a tell — action `0x10000022`.
    pub last_teller: Option<String>,
    /// The last name heard on the monarch channel — action `0x10000020`.
    pub monarch: Option<String>,
    /// The last name heard on the patron channel — action `0x10000021`.
    pub patron: Option<String>,
}
