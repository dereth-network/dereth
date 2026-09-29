//! What the shell needs from the platform under it, and the parts of that every platform shares.
//!
//! The traits are [`host`]'s: [`host::Host`] is what a desktop window or a browser page supplies —
//! the window and its clock, the time zone, the URL launch, the sound output, the clipboard and
//! the cursor images — and the shell is generic over it. The runtime's own seams (the window host,
//! the clock and pacer, the dialog, the files) are [`dereth_client_runtime::platform`]'s, and a
//! host's window, clock and dialog arrive in the runtime's `Platform` it opens.
//!
//! What is here besides is shared by every host: the window's queue of events and the runtime
//! lifecycle each one is ([`window`]), and the key and button identities the events carry
//! ([`keys`]).

pub mod host;

/// The window's events: the shared device-and-lifecycle event, and the queue a window fills and
/// the shell routes.
pub mod window {
    pub use dereth_client_runtime::platform::window::{
        headless_screen_metrics, NullWindow, PumpedEvents, WindowHost, STANDARD_DISPLAY_MODES,
    };

    pub use dereth_input::host::HostEvent;

    /// The runtime's lifecycle event this is, or `None` for a device event.
    #[must_use]
    pub const fn lifecycle(
        event: &HostEvent,
    ) -> Option<dereth_client_runtime::platform::window::HostEvent> {
        use dereth_client_runtime::platform::window::HostEvent as L;
        Some(match *event {
            HostEvent::CloseRequested => L::CloseRequested,
            HostEvent::Destroyed => L::Destroyed,
            HostEvent::Focused(gained) => L::Focused(gained),
            HostEvent::Resized { width, height } => L::Resized { width, height },
            HostEvent::ScaleFactorChanged => L::ScaleFactorChanged,
            _ => return None,
        })
    }

    /// The window's queue of events, shared between the window that fills it during its drain and
    /// the front end that routes them.
    pub type WindowEvents = std::rc::Rc<std::cell::RefCell<Vec<HostEvent>>>;
}

/// The shared physical key and mouse-button identities.
pub mod keys {
    pub use dereth_input::keys::{Key, MouseButton};
}
