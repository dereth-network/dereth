//! The canvas as the client's window, and the events the page queues for it.
//!
//! The page listens for the canvas's pointer, wheel, key, focus and size events and queues each as
//! a [`HostEvent`] on the [`WindowEvents`] it shares with the front end, which routes them when
//! the runtime asks it to, as a desktop window's drain does. The window itself produces nothing
//! in its drain; it only answers the geometry questions, with the canvas's size.
//!
//! The key identities are the shared tables over the shared key codes, which are named as the
//! page's `KeyboardEvent.code` strings are, so [`key_code_from_dom`] is a name lookup.

use std::cell::Cell;

use dereth_client_contract::window::ScreenMetrics;
use dereth_client_runtime::platform::window::{
    headless_screen_metrics, WindowHost, STANDARD_DISPLAY_MODES,
};
use dereth_ui_screens::options::store::DisplayMode;

use dereth_input::keys::Key;

/// The shared event, and the window's queue of them: the client shell's.
pub use {dereth_client_shell::platform::window::WindowEvents, dereth_input::host::HostEvent};

/// The canvas's size, shared by the window and the page that resizes the canvas.
pub type CanvasSize = std::rc::Rc<Cell<(u32, u32)>>;

/// The canvas, as the client's window.
#[derive(Debug)]
pub struct WebWindow {
    size: CanvasSize,
}

impl WebWindow {
    /// A window over the canvas whose size is `size`.
    #[must_use]
    pub const fn new(size: CanvasSize) -> Self {
        Self { size }
    }
}

impl WindowHost for WebWindow {
    fn has_window(&self) -> bool {
        true
    }

    fn client_size(&self) -> (u32, u32) {
        self.size.get()
    }

    /// The canvas is the whole screen the client can be placed on.
    fn screen_metrics(&self, _frame_metrics: (i32, i32, i32)) -> ScreenMetrics {
        let (w, h) = self.size.get();
        ScreenMetrics {
            cx_screen: i32::try_from(w).unwrap_or(i32::MAX),
            cy_screen: i32::try_from(h).unwrap_or(i32::MAX),
            ..headless_screen_metrics()
        }
    }

    /// The standard modes that fit the canvas, and the canvas's own size.
    fn display_modes(&self) -> Vec<DisplayMode> {
        let (w, h) = self.size.get();
        let mut modes: Vec<DisplayMode> = STANDARD_DISPLAY_MODES
            .iter()
            .filter(|(mw, mh)| *mw <= w.max(800) && *mh <= h.max(600))
            .map(|&(width, height)| DisplayMode {
                width,
                height,
                refresh_rate: 0,
                bits_per_pixel: 32,
            })
            .collect();
        if !modes.iter().any(|m| (m.width, m.height) == (w, h)) {
            modes.push(DisplayMode {
                width: w,
                height: h,
                refresh_rate: 0,
                bits_per_pixel: 32,
            });
        }
        modes
    }

    /// The client asks for a size; the canvas takes it.
    fn request_inner_size(&self, width: u32, height: u32) -> (u32, u32) {
        self.size.set((width, height));
        (width, height)
    }
}

/// The client's key identity for a shared key code: the virtual key and scan code the input
/// pipeline's messages carry, or `None` for a key the tables do not map.
#[must_use]
pub fn key_from_key_code(code: dereth_input::keys::KeyCode) -> Option<Key> {
    Some(Key::new(
        dereth_input::pump::vk_from_key_code(code)?,
        dereth_input::pump::scan_code_from_key_code(code)?,
    ))
}

/// The key code the page's `KeyboardEvent.code` names.
#[must_use]
pub fn key_code_from_dom(code: &str) -> Option<dereth_input::keys::KeyCode> {
    crate::keycodes::from_dom(code)
}
