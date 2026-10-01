//! The desktop host: what the client shell is given by this executable — the `winit` window and
//! the system clock, the operating system's time zone and URL launch, the `cpal` sound output, the
//! system clipboard and the system cursors.

use dereth_client_shell::cursor::CursorImages;
use dereth_client_shell::platform::host::Host;

use crate::app::{Platform, StartupError};
use crate::Config;

/// The desktop, as the client shell's host.
#[derive(Debug, Clone, Copy, Default)]
pub struct Desktop;

impl Host for Desktop {
    const BUILD_ID: &'static str = concat!("dereth-client ", env!("CARGO_PKG_VERSION"));

    type Clipboard = crate::clipboard::Win32Clipboard;

    fn open_platform(
        cfg: &Config,
        events: crate::platform::window::WindowEvents,
    ) -> Result<Platform, StartupError> {
        crate::app::open_platform(cfg, events)
    }

    fn local_utc_offset_secs(unix_secs: i64) -> i32 {
        crate::platform::local_utc_offset_secs(unix_secs)
    }

    fn launch_uri(url: &str) -> i32 {
        crate::app::launch_uri(url)
    }

    fn install_default_output() {
        crate::audio::install_default_output();
    }

    fn clipboard() -> Self::Clipboard {
        crate::clipboard::Win32Clipboard
    }

    fn cursor_images(window: Option<isize>) -> Box<dyn CursorImages> {
        crate::cursor::desktop_cursor_images(window)
    }
}
