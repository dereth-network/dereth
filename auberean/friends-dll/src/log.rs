//! The library's log: a file the launcher names in `AUBEREAN_FRIENDS_LOG`, or none.
//!
//! The library is the outermost Rust in the retail client's process, so it installs its own log
//! subscriber, once, when the client first asks for its object. It writes connections, requests'
//! kinds and results' codes, never the ticket or a tell's text.

use std::fs::OpenOptions;
use std::sync::{Mutex, Once};

/// The variable naming the log file.
pub const LOG_VAR: &str = "AUBEREAN_FRIENDS_LOG";

/// Start logging to the file [`LOG_VAR`] names, if it names one; once.
pub fn start() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let Some(path) = std::env::var_os(LOG_VAR).filter(|p| !p.is_empty()) else {
            return;
        };
        let Ok(file) = OpenOptions::new().create(true).append(true).open(path) else {
            return;
        };
        let _ = tracing_subscriber::fmt()
            .with_writer(Mutex::new(file))
            .with_ansi(false)
            .with_max_level(tracing::Level::DEBUG)
            .try_init();
        tracing::info!(software = crate::host::SOFTWARE, "loaded");
    });
}
