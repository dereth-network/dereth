//! The browser as the client shell's host: the canvas as the window, the page's events as the host
//! events, the browser's clock and time zone, the page's clipboard, a URL opened in a new tab, the
//! page's audio worklet as the sound output, the fonts the client carries for the classic
//! interface's text, and the Horizon interface's art as the page fetched it from beside the module
//! ([`hand_over_horizon_art`]). The cursor is the page's to draw
//! ([`crate::play::Play::cursor_change`]), so the shell's cursor images are never installed here;
//! the pointer held for a camera drag is the page's to lock ([`pointer`](mod@pointer)).

// The sound output: a pull the worker posts to the page.
pub mod audio;
// The clipboard, mirrored between the client and the page.
pub mod clipboard;
// The clocks, the pacer and the local time zone.
pub mod clock;
// The modal error box, which a page shows as a log line.
pub mod dialog;
// The pointer held for a camera drag, which the page locks to the canvas.
pub mod pointer;
// The canvas as the window, and the events the page queues on it.
pub mod window;

use std::cell::RefCell;
use std::sync::Arc;

use dereth_client_runtime::app::{Platform, StartupError};
use dereth_client_runtime::config::Config;
use dereth_client_shell::cursor::{CursorImages, PortableCursors};
use dereth_client_shell::platform::host::{HorizonArt, Host};
use dereth_client_shell::platform::window::WindowEvents;
use dereth_horizon::pieces::Pieces;

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

    /// The page's lock on the pointer: see [`pointer`](mod@pointer).
    fn pointer(_window: Option<isize>) -> Box<dyn dereth_client_shell::pointer::HostPointer> {
        Box::new(pointer::PagePointer)
    }

    /// The fonts the client carries, drawn as the Windows font system draws them, so the classic
    /// interface's text is laid out as on the Windows desktop in every browser.
    fn classic_fonts() -> Option<std::sync::Arc<dyn dereth_classic_dat::fonts::FontSource>> {
        Some(std::sync::Arc::new(dereth_classic_fonts::ShippedFonts))
    }

    /// The art the page handed over ([`hand_over_horizon_art`]). The module carries none: until
    /// the page has handed it over it is loading, and when the page could not fetch it the
    /// Horizon interface is refused with why.
    fn horizon_art() -> HorizonArt {
        HORIZON_ART.with(|art| match &*art.borrow() {
            Some(Ok(pieces)) => HorizonArt::Ready(Arc::clone(pieces)),
            Some(Err(why)) => {
                HorizonArt::Unavailable(format!("The Horizon interface's art did not load: {why}"))
            }
            None => HorizonArt::Loading,
        })
    }
}

thread_local! {
    /// The Horizon interface's art as the page handed it over, or why it could not; `None` until
    /// the page has said.
    static HORIZON_ART: RefCell<Option<Result<Arc<Pieces>, String>>> = const { RefCell::new(None) };
}

/// The page's word on the Horizon interface's art: the files it fetched from beside the module, or
/// why it could not fetch them. Files that fail [`Pieces::check`] are not taken. From then on the
/// Horizon interface is offered, or refused with the reason.
///
/// # Errors
/// Why the art is not taken: the page's reason, or what is wrong with the files.
pub fn hand_over_horizon_art(fetched: Result<Pieces, String>) -> Result<(), String> {
    let art = fetched.and_then(|pieces| pieces.check().map(|()| Arc::new(pieces)));
    let taken = art.as_ref().map(|_| ()).map_err(Clone::clone);
    HORIZON_ART.with(|slot| *slot.borrow_mut() = Some(art));
    taken
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

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental Horizon interface: the art the page hands the host).
    use super::*;

    /// The browser's host has the Horizon interface's art only once the page has handed over
    /// files that pass the check: before that it is loading, and after a failed fetch or for a
    /// page sent in a file's place it says why it has none.
    #[test]
    fn the_horizon_art_is_there_only_once_the_page_has_handed_over_files_that_pass_the_check() {
        assert!(
            matches!(WebHost::horizon_art(), HorizonArt::Loading),
            "nothing handed over yet"
        );
        let why = "pkg/horizon/fonts.json: 404 Not Found";
        assert_eq!(
            hand_over_horizon_art(Err(why.to_owned())),
            Err(why.to_owned())
        );
        let HorizonArt::Unavailable(refused) = WebHost::horizon_art() else {
            panic!("a failed fetch is no art");
        };
        assert!(
            refused.contains("did not load") && refused.contains(why),
            "{refused}"
        );
        let mut wrong = Pieces::built_in().expect("the files, to hand over");
        wrong.insert("manifest.json", b"<!doctype html>".to_vec());
        let refused = hand_over_horizon_art(Ok(wrong)).unwrap_err();
        assert!(refused.contains("manifest.json is not a JSON"), "{refused}");
        assert!(matches!(WebHost::horizon_art(), HorizonArt::Unavailable(_)));
        assert_eq!(
            hand_over_horizon_art(Ok(Pieces::built_in().unwrap())),
            Ok(())
        );
        let HorizonArt::Ready(art) = WebHost::horizon_art() else {
            panic!("handed over");
        };
        assert!(art.get("manifest.json").is_some());
    }
}
