//! The host: what a desktop window or a browser page gives the shell, as one type the shell is
//! generic over.
//!
//! Every item is an associated function or type, so a host is a choice made at compile time and
//! costs nothing at run time: the desktop client names its host once (`dereth_client::Desktop`),
//! the browser client names its own, and a headless run or a test names [`NullHost`].

use dereth_client_runtime::app::{Platform, StartupError};
use dereth_client_runtime::config::Config;

use super::window::WindowEvents;
use crate::clipboard::HostClipboard;
use crate::cursor::{CursorImages, PortableCursors};

/// What the shell needs from the platform under it.
pub trait Host: 'static {
    /// The program's name and version, as `@version` prints them after "Client version" (for
    /// example `dereth-client 0.1.3`).
    const BUILD_ID: &'static str;

    /// The host clipboard the shell mirrors each frame.
    type Clipboard: HostClipboard;

    /// The windowed platform: the window and its event loop, the clock, the pacer and the error
    /// box. `events` is the queue the window fills and the shell routes.
    ///
    /// # Errors
    /// [`StartupError::Device`] when the window cannot be made.
    fn open_platform(cfg: &Config, events: WindowEvents) -> Result<Platform, StartupError>;

    /// The shift from UTC the host's `localtime` would apply to `unix_secs`, in seconds, for
    /// every local date the client draws.
    fn local_utc_offset_secs(unix_secs: i64) -> i32;

    /// Hand `url` to the host to open: `33` when it was handed on, as the desktop launch answers,
    /// `0` when it was not.
    fn launch_uri(url: &str) -> i32;

    /// Install the host's sound output as the mixer's default endpoint. Idempotent.
    fn install_default_output();

    /// The clipboard, for one front end's life.
    fn clipboard() -> Self::Clipboard;

    /// The cursor images, put on `window` (the native handle, `None` without one).
    fn cursor_images(window: Option<isize>) -> Box<dyn CursorImages>;

    /// The system fonts the classic interface's text is drawn with; `None` on a host that has
    /// none, where the classic interface is not offered.
    fn classic_fonts() -> Option<std::sync::Arc<dyn dereth_classic_dat::fonts::FontSource>> {
        None
    }
}

/// The host with nothing under it: no window, UTC, nothing launched, no sound, an empty
/// clipboard and cursors that are resolved and never installed. What a headless run and the
/// shell's own tests stand on.
#[derive(Debug, Clone, Copy, Default)]
pub struct NullHost;

impl Host for NullHost {
    const BUILD_ID: &'static str = concat!("dereth-client ", env!("CARGO_PKG_VERSION"));

    type Clipboard = crate::clipboard::NoClipboard;

    fn open_platform(_cfg: &Config, _events: WindowEvents) -> Result<Platform, StartupError> {
        Err(StartupError::Device {
            cause: "this host has no window".to_string(),
        })
    }

    fn local_utc_offset_secs(_unix_secs: i64) -> i32 {
        0
    }

    fn launch_uri(_url: &str) -> i32 {
        0
    }

    fn install_default_output() {}

    fn clipboard() -> Self::Clipboard {
        crate::clipboard::NoClipboard
    }

    fn cursor_images(_window: Option<isize>) -> Box<dyn CursorImages> {
        Box::new(PortableCursors)
    }
}
