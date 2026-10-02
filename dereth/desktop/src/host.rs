//! The desktop host: what the client shell is given on a desktop -- the `winit` window and the
//! system clock, the operating system's time zone and URL launch, the `cpal` sound output, the
//! system clipboard and the system cursors -- for the product `P`.

use std::marker::PhantomData;

use dereth_client_runtime::app::{Platform, StartupError};
use dereth_client_runtime::config::Config;
use dereth_client_shell::cursor::CursorImages;
use dereth_client_shell::platform::host::Host;

use crate::Product;

/// The desktop, as the client shell's host, for product `P`.
#[derive(Debug, Clone, Copy, Default)]
pub struct Desktop<P: Product>(PhantomData<P>);

impl<P: Product> Host for Desktop<P> {
    const BUILD_ID: &'static str = P::BUILD_ID;

    type Clipboard = crate::clipboard::Win32Clipboard;

    fn open_platform(
        cfg: &Config,
        events: crate::platform::window::WindowEvents,
    ) -> Result<Platform, StartupError> {
        crate::launch::open_platform::<P>(cfg, events)
    }

    fn local_utc_offset_secs(unix_secs: i64) -> i32 {
        crate::platform::local_utc_offset_secs(unix_secs)
    }

    fn launch_uri(url: &str) -> i32 {
        crate::launch::launch_uri(url)
    }

    fn install_default_output() {
        crate::audio::install_default_output();
    }

    fn clipboard() -> Self::Clipboard {
        crate::clipboard::Win32Clipboard
    }

    fn cursor_images(window: Option<isize>) -> Box<dyn CursorImages> {
        P::cursor_images(window)
    }
}
