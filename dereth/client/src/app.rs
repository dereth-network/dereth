//! The application on the desktop: the client shell's [`dereth_client_shell::app::App`] on the
//! [`Desktop`](crate::Desktop) host.
//!
//! Shared application types are named through the runtime.

use dereth_client_runtime::app::{Platform, StartupError};

use dereth_client_runtime::config::Config;

/// The application, on the desktop.
pub type App = dereth_client_shell::app::App<crate::Desktop>;

/// The front end, on the desktop.
pub type ClientShell = dereth_client_shell::app::ClientShell<crate::Desktop>;

/// The runtime's application with the desktop's front end in it.
pub type CoreApp = dereth_client_shell::app::CoreApp<crate::Desktop>;

/// The windowed platform: the window and its event loop, the system clock and pacer, and the OS
/// error box.
///
/// # Errors
/// [`StartupError::Device`] when the event loop or the window cannot be created.
pub fn open_platform(
    cfg: &Config,
    events: dereth_client_shell::platform::window::WindowEvents,
) -> Result<Platform, StartupError> {
    dereth_desktop::launch::open_platform::<crate::Dereth>(cfg, events)
}
