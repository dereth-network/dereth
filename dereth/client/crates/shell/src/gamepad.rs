//! The host's pad: the shell reads it once a frame and hands it to the interface shown, which
//! turns it into actions and the orbit camera's sticks. A host with no pad (the browser, a
//! headless run, a test) reads nothing.

pub use dereth_input::pad::PadState;

/// The host's pad, behind a seam so the shell names no pad library.
pub trait HostGamepad {
    /// The pad as it is now; `None` with none connected.
    fn poll(&mut self) -> Option<PadState>;
}

/// No pad at all.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoGamepad;

impl HostGamepad for NoGamepad {
    fn poll(&mut self) -> Option<PadState> {
        None
    }
}
