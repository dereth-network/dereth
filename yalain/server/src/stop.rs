//! Stopping on the system's word. Ctrl-C (SIGINT), and SIGTERM and SIGHUP on Linux and macOS, or
//! Ctrl-C and Ctrl-Break on Windows, raise a flag the world's loop reads each pass; the loop then
//! stops as it does at a shutdown's end, every character still in the world kept first. A service
//! manager's stop is SIGTERM, so a world with no console stops as cleanly as one with.
//!
//! Closing a Windows console window ends the process as soon as the flag is raised, before the
//! loop can keep anyone.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// The flag the stop signals raise.
#[derive(Debug, Clone, Default)]
pub struct Interrupt(Arc<AtomicBool>);

impl Interrupt {
    /// The flag, raised by the system's stop signals from now on. A system that will not take the
    /// handler leaves the flag down, and the log says so.
    #[must_use]
    pub fn install() -> Self {
        let flag = Self::default();
        if let Err(e) = ctrlc::set_handler(flag.handler()) {
            tracing::warn!(
                "the stop signals are not caught ({e}): stopping the process ends it at once"
            );
        }
        flag
    }

    /// What a stop signal runs: it raises the flag, and nothing more.
    pub fn handler(&self) -> impl FnMut() + Send + 'static {
        let raised = Arc::clone(&self.0);
        move || raised.store(true, Ordering::SeqCst)
    }

    /// Whether a stop signal came.
    #[must_use]
    pub fn raised(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the executable's plumbing between the system's stop signals and its loop)
    use super::*;

    #[test]
    fn a_stop_signal_raises_the_flag_the_loop_reads() {
        let flag = Interrupt::default();
        assert!(!flag.raised(), "down until a signal comes");
        let mut handler = flag.handler();
        handler();
        assert!(flag.raised(), "raised by the signal's handler");
        handler();
        assert!(flag.clone().raised(), "a second signal leaves it raised");
    }
}
