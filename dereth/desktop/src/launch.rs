//! The two things only the desktop does at start-up: open the window (with the system clock, the
//! pacer and the error box), and hand a URL to the desktop to open.

use dereth_client_runtime::app::{Platform, StartupError};
use dereth_client_runtime::config::Config;
use dereth_client_runtime::platform::clock::SystemClock;

use crate::Product;

/// The host's launch, which the [`Desktop`](crate::Desktop) host installs as the core's
/// [`dereth_client_runtime::platform::shell::launch_uri`].
#[cfg(windows)]
pub fn launch_uri(url: &str) -> i32 {
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
pub fn launch_uri(url: &str) -> i32 {
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
pub fn open_platform<P: Product>(
    cfg: &Config,
    events: dereth_client_shell::platform::window::WindowEvents,
) -> Result<Platform, StartupError> {
    P::before_window();
    let look = crate::platform::window::WindowLook {
        title: P::TITLE,
        icon_png: P::ICON_PNG,
        app_id: P::APP_ID,
        resizable: P::RESIZABLE,
    };
    let window = crate::platform::window::open_window(cfg, &look, events)
        .map_err(|cause| StartupError::Device { cause })?;
    // Whether frames slow down in the background is the player's option, which the frame pace
    // reads every frame.
    Ok(Platform {
        window: Box::new(window),
        clock: Box::new(SystemClock::new()),
        pacer: Box::new(SystemClock::new()),
        dialog: Box::new(crate::platform::dialog::SystemDialog),
    })
}
