//! Services: what one plugin offers the others in the same client.
//!
//! A plugin **offers** a service by name (`services.offer("set_meta_state", fn)`), optionally only
//! to the plugins it lists. Another plugin **calls** it
//! (`services.call("network.dereth.tank", "set_meta_state", { state = "Hunt" })`) and gets a promise. The
//! call reaches the provider at the next frame as a `service.call` event; the provider's answer (or
//! its error) settles the caller's promise the frame after. A call to a plugin that is not loaded
//! and active, to a service it does not offer, or one it does not let the caller use, fails at
//! once (`no_plugin`, `no_service`, `not_allowed`); one unanswered for [`REPLY_SECS`] fails as
//! `timeout`.
//!
//! The plugins never share a VM, a value is copied across as data (at most [`MAX_PAYLOAD`] bytes of
//! JSON), and nothing passes the caller's permissions or gesture to the provider: the provider acts
//! under its own, and hears whether the call followed the player's gesture only as a fact.

use dereth_plugin_api::Value;

/// How long a call waits for its answer, in seconds.
pub const REPLY_SECS: f64 = 5.0;
/// The most bytes of JSON a call's arguments or its answer may be.
pub const MAX_PAYLOAD: usize = 64 << 10;
/// The most calls one plugin may make in a second.
pub const CALLS_PER_SECOND: usize = 20;
/// The most services one plugin may offer.
pub const MAX_SERVICES: usize = 64;

/// One call made, before it is delivered.
#[derive(Debug, Clone, PartialEq)]
pub struct ServiceCall {
    pub id: i64,
    pub caller: String,
    pub provider: String,
    pub service: String,
    pub args: Value,
    /// Whether the caller had a live gesture when it called.
    pub gesture: bool,
}

/// One call delivered and not yet answered.
#[derive(Debug, Clone, PartialEq)]
pub struct Pending {
    pub caller: String,
    pub provider: String,
    /// When it fails as `timeout`, on the frame clock.
    pub deadline: f64,
}

/// Whether `name` may name a service: letters, digits, `_`, `-` and `.`, at most 64.
///
/// # Errors
/// What is wrong with it.
pub fn check_name(name: &str) -> Result<(), String> {
    if name.is_empty()
        || name.len() > 64
        || !name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.'))
    {
        return Err(format!(
            "{name:?} is not a service name (letters, digits, _, - and ., at most 64)"
        ));
    }
    Ok(())
}
