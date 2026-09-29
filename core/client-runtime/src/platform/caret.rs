//! The desktop's caret blink interval, as the host reads it.
//!
//! The UI's text tick asks for the player's caret blink setting, which is an operating-system
//! query. The host installs its reader here, the way it installs the local-time shift in
//! [`crate::platform::clock`].

/// The host's reader, installed once.
static CARET_BLINK: std::sync::OnceLock<fn() -> f64> = std::sync::OnceLock::new();

/// Install the host's caret blink reader. The first installation stands; later ones are the same
/// function and are ignored.
pub fn install_caret_blink(f: fn() -> f64) {
    let _ = CARET_BLINK.set(f);
}

/// The caret blink interval in seconds: the host's answer, or before it has installed its reader
/// the 530 ms the portable build has always used.
#[must_use]
pub fn caret_blink_time_seconds() -> f64 {
    CARET_BLINK.get().map_or_else(
        || dereth_client_contract::window_proc::caret_blink_time_seconds_from_millis(530),
        |f| f(),
    )
}
