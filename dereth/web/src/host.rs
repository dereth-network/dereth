//! The browser as the client shell's host: the canvas as the window, the page's events as the host
//! events, the browser's clock and time zone, the page's clipboard, a URL opened in a new tab, and
//! the page's audio worklet as the sound output. The cursor is the page's to draw
//! ([`crate::play::Play::cursor_change`]), so the shell's cursor images are never installed here.

// The sound output: a pull the worker posts to the page.
pub mod audio;
// The clipboard, mirrored between the client and the page.
pub mod clipboard;
// The clocks, the pacer and the local time zone.
pub mod clock;
// The modal error box, which a page shows as a log line.
pub mod dialog;
// The canvas as the window, and the events the page queues on it.
pub mod window;

use dereth_client_runtime::app::{Platform, StartupError};
use dereth_client_runtime::config::Config;
use dereth_client_shell::cursor::{CursorImages, PortableCursors};
use dereth_client_shell::platform::host::Host;
use dereth_client_shell::platform::window::WindowEvents;

/// The browser page, as the client shell's host.
#[derive(Debug, Clone, Copy, Default)]
pub struct WebHost;

impl Host for WebHost {
    const BUILD_ID: &'static str = concat!("dereth-web ", env!("CARGO_PKG_VERSION"));

    type Clipboard = clipboard::PageClipboard;

    /// The canvas at the configured size, the runtime's clock, a pacer that leaves the pacing to
    /// the page's animation frames, and the log as the error box.
    fn open_platform(cfg: &Config, _events: WindowEvents) -> Result<Platform, StartupError> {
        Ok(Platform {
            window: Box::new(window::WebWindow::new(std::rc::Rc::new(
                std::cell::Cell::new((cfg.width, cfg.height)),
            ))),
            clock: Box::new(clock::SystemClock::new()),
            pacer: Box::new(clock::FramePaced),
            dialog: Box::new(dialog::SystemDialog),
        })
    }

    fn local_utc_offset_secs(unix_secs: i64) -> i32 {
        clock::local_utc_offset_secs(unix_secs)
    }

    fn launch_uri(url: &str) -> i32 {
        match open_url(url) {
            Ok(()) => 33,
            Err(_) => 0,
        }
    }

    fn install_default_output() {
        audio::install_default_output();
    }

    fn clipboard() -> Self::Clipboard {
        clipboard::PageClipboard
    }

    fn cursor_images(_window: Option<isize>) -> Box<dyn CursorImages> {
        Box::new(PortableCursors)
    }
}

/// Ask the page to open `url` in a new tab.
///
/// # Errors
/// Never; the signature is the desktop launch's.
pub fn open_url(url: &str) -> Result<(), String> {
    OPENED.with(|o| o.borrow_mut().push(url.to_string()));
    Ok(())
}

thread_local! {
    static OPENED: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// The URLs the client has asked to open since the last call, for the page to open.
#[must_use]
pub fn take_opened_urls() -> Vec<String> {
    OPENED.with(|o| std::mem::take(&mut *o.borrow_mut()))
}
