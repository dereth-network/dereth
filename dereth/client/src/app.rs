//! The application on the desktop: the client shell's [`dereth_client_shell::app::App`] on the
//! [`Desktop`](crate::Desktop) host, with the two things only the desktop does at start-up — open
//! a window, and hand a URL to the desktop to open.
//!
//! Every other item of the shell's `app` module is re-exported here at its old path.

pub use dereth_client_shell::app::*;

use crate::Config;

/// The application, on the desktop.
pub type App = dereth_client_shell::app::App<crate::Desktop>;

/// The front end, on the desktop.
pub type ClientShell = dereth_client_shell::app::ClientShell<crate::Desktop>;

/// The runtime's application with the desktop's front end in it.
pub type CoreApp = dereth_client_shell::app::CoreApp<crate::Desktop>;

/// The host's launch, which the [`Desktop`](crate::Desktop) host installs as the core's
/// [`dereth_client_runtime::platform::shell::launch_uri`].
#[cfg(windows)]
pub(crate) fn launch_uri(url: &str) -> i32 {
    let Ok(uri) = windows::Foundation::Uri::CreateUri(&windows::core::HSTRING::from(url)) else {
        return 0;
    };
    match windows::System::Launcher::LaunchUriAsync(&uri) {
        Ok(_) => 33,
        Err(_) => 0,
    }
}

/// `xdg-open` / `open`, through the `open` crate: the same "hand it to the desktop" call.
#[cfg(not(windows))]
pub(crate) fn launch_uri(url: &str) -> i32 {
    match open::that(url) {
        Ok(()) => 33,
        Err(_) => 0,
    }
}

/// The windowed platform: the window and its event loop, the system clock and pacer, and the OS
/// error box.
///
/// Step 12 of: `InitUI -> (windowed, title, 800, 600, visible, "")`.
///
/// # Errors
/// [`StartupError::Device`] when the event loop or the window cannot be created.
pub fn open_platform(
    cfg: &Config,
    events: crate::platform::window::WindowEvents,
) -> Result<Platform, StartupError> {
    let window = crate::platform::window::open_window(cfg, events)
        .map_err(|cause| StartupError::Device { cause })?;
    Ok(Platform {
        window: Box::new(window),
        clock: Box::new(SystemClock::new()),
        pacer: Box::new(SystemClock::new()),
        dialog: Box::new(crate::platform::dialog::SystemDialog),
    })
}
