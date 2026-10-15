//! The canvas as the client's window, the page as its screen, and the events the page queues for
//! it.
//!
//! The page is the screen the client is placed on: its size in the device's own pixels. Full
//! screen fills it: the canvas is drawn at the page's size, at the player's drawing scale, and
//! follows the page and the scale as they change ([`Page::follow`]). The drawing scale starts at
//! the front page's choice, which the window offers the client as its start
//! ([`WindowHost::drawing_scale`]), and from then on is the option store's, which every
//! interface's Client Options page changes. Out of full screen the canvas is the size the
//! client asks for, and the page scales it to fit: the screens the modern and classic interfaces
//! hold at their fixed 800 by 600 before the world are drawn that way, as is a game the player
//! takes out of full screen. The resolutions offered are the standard ones that fit the page, and
//! the page's own size.
//!
//! The page listens for the canvas's pointer, wheel, key, focus and size events and queues each as
//! a [`HostEvent`] on the [`WindowEvents`] it shares with the front end, which routes them when
//! the runtime asks it to, as a desktop window's drain does. The window itself produces nothing
//! in its drain; it only answers the geometry questions.
//!
//! The key identities are the shared tables over the shared key codes, which are named as the
//! page's `KeyboardEvent.code` strings are, so [`key_code_from_dom`] is a name lookup.

use std::cell::Cell;
use std::rc::Rc;

use dereth_client_contract::options::drawing_scale;
use dereth_client_contract::window::ScreenMetrics;
use dereth_client_runtime::config::FORCED_LOGIN_SIZE;
use dereth_client_runtime::platform::window::{
    headless_screen_metrics, WindowHost, STANDARD_DISPLAY_MODES,
};
use dereth_ui_screens::options::store::DisplayMode;

use dereth_input::keys::Key;

/// The shared event, and the window's queue of them: the client shell's.
pub use {dereth_client_shell::platform::window::WindowEvents, dereth_input::host::HostEvent};

/// The longest side the canvas is drawn at.
pub const LONGEST_SIDE: u32 = 8192;

/// The page's size in the device's own pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageSize {
    pub width: u32,
    pub height: u32,
}

impl PageSize {
    /// A page of `width` by `height` pixels.
    #[must_use]
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }
}

/// `size`, made at least the 800 by 600 the client lays its screens out at, in its own shape, and
/// no side longer than [`LONGEST_SIDE`].
#[must_use]
pub fn drawable(size: (u32, u32)) -> (u32, u32) {
    let (least_w, least_h) = (
        u64::from(FORCED_LOGIN_SIZE.0),
        u64::from(FORCED_LOGIN_SIZE.1),
    );
    let (w, h) = (u64::from(size.0.max(1)), u64::from(size.1.max(1)));
    let (w, h) = if w >= least_w && h >= least_h {
        (w, h)
    } else if w * least_h >= h * least_w {
        // As wide as 4:3 or wider: the height is the side to raise to its least.
        ((w * least_h).div_ceil(h).max(least_w), least_h)
    } else {
        (least_w, (h * least_w).div_ceil(w).max(least_h))
    };
    let side = |v: u64| u32::try_from(v.min(u64::from(LONGEST_SIDE))).unwrap_or(LONGEST_SIDE);
    (side(w), side(h))
}

/// The page as the client's screen, and the canvas's size on it: shared by the window and the
/// page that sizes it.
#[derive(Debug)]
pub struct Page {
    /// The page's size, as the page last said.
    size: Cell<PageSize>,
    /// The canvas's size: the page's while it fills it, else the size the client asked for.
    canvas: Cell<(u32, u32)>,
    /// Whether the canvas fills the page: the client's full screen.
    fills: Cell<bool>,
    /// The share of the page's pixels the client starts drawing while the canvas fills it, in
    /// percent: the front page's drawing scale.
    start_scale: u32,
}

impl Page {
    /// A page of `size`, with the canvas `canvas` on it and not filling it, whose drawing scale
    /// starts at `start_scale` percent.
    #[must_use]
    pub fn new(size: PageSize, canvas: (u32, u32), start_scale: u32) -> Rc<Self> {
        let page = Rc::new(Self {
            size: Cell::new(PageSize::new(1, 1)),
            canvas: Cell::new(canvas),
            fills: Cell::new(false),
            start_scale,
        });
        page.set(size);
        page
    }

    /// The page is now `size`. A page with no pixels (one being hidden) changes nothing.
    pub fn set(&self, size: PageSize) {
        if size.width == 0 || size.height == 0 {
            return;
        }
        self.size.set(size);
    }

    /// The screen the client is placed on: the page's own pixels, at least 800 by 600.
    #[must_use]
    pub fn screen(&self) -> (u32, u32) {
        let s = self.size.get();
        drawable((s.width, s.height))
    }

    /// The size the canvas is drawn at while it fills the page: the page's pixels at the drawing
    /// scale the option store holds, at least 800 by 600.
    #[must_use]
    pub fn fill_size(&self) -> (u32, u32) {
        let s = self.size.get();
        let scale = u64::from(drawing_scale::percent());
        let share = |v: u32| u32::try_from((u64::from(v) * scale + 50) / 100).unwrap_or(u32::MAX);
        drawable((share(s.width), share(s.height)))
    }

    /// The canvas's size.
    #[must_use]
    pub fn canvas(&self) -> (u32, u32) {
        self.canvas.get()
    }

    /// Whether the canvas fills the page.
    #[must_use]
    pub fn fills(&self) -> bool {
        self.fills.get()
    }

    /// While the canvas fills the page and the page or the drawing scale has changed since, the
    /// size it is now drawn at, for the client to be told its window was resized to it; `None`
    /// otherwise.
    pub fn follow(&self) -> Option<(u32, u32)> {
        let fill = self.fill_size();
        (self.fills.get() && self.canvas.get() != fill).then(|| {
            self.canvas.set(fill);
            fill
        })
    }
}

/// The canvas, as the client's window.
#[derive(Debug)]
pub struct WebWindow {
    page: Rc<Page>,
}

impl WebWindow {
    /// A window over the canvas on `page`.
    #[must_use]
    pub const fn new(page: Rc<Page>) -> Self {
        Self { page }
    }
}

impl WindowHost for WebWindow {
    fn has_window(&self) -> bool {
        true
    }

    fn client_size(&self) -> (u32, u32) {
        self.page.canvas()
    }

    /// The page is the whole screen the client can be placed on.
    fn screen_metrics(&self, _frame_metrics: (i32, i32, i32)) -> ScreenMetrics {
        let (w, h) = self.page.screen();
        ScreenMetrics {
            cx_screen: i32::try_from(w).unwrap_or(i32::MAX),
            cy_screen: i32::try_from(h).unwrap_or(i32::MAX),
            ..headless_screen_metrics()
        }
    }

    /// The standard modes that fit the page, and the page's own size.
    fn display_modes(&self) -> Vec<DisplayMode> {
        let (w, h) = self.page.screen();
        let mode = |(width, height)| DisplayMode {
            width,
            height,
            refresh_rate: 0,
            bits_per_pixel: 32,
        };
        let mut modes: Vec<DisplayMode> = STANDARD_DISPLAY_MODES
            .iter()
            .filter(|(mw, mh)| *mw <= w && *mh <= h)
            .map(|&size| mode(size))
            .collect();
        if !modes.iter().any(|m| (m.width, m.height) == (w, h)) {
            modes.push(mode((w, h)));
        }
        modes
    }

    /// Full screen fills the page.
    fn offers_full_screen(&self) -> bool {
        true
    }

    /// Full screen draws a share of the page's pixels, starting at the front page's choice.
    fn drawing_scale(&self) -> Option<u32> {
        Some(self.page.start_scale)
    }

    /// Full screen draws the canvas at the page's size; out of it, the canvas keeps its size until
    /// the client asks for another.
    fn set_borderless_fullscreen(&self, full_screen: bool) {
        self.page.fills.set(full_screen);
        if full_screen {
            self.page.canvas.set(self.page.fill_size());
        }
    }

    /// The client asks for a size; the canvas takes it.
    fn request_inner_size(&self, width: u32, height: u32) -> (u32, u32) {
        self.page.canvas.set((width, height));
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

#[cfg(test)]
mod tests {
    use super::*;

    use dereth_client_contract::options::store;
    use dereth_client_contract::PrefValue;

    fn window(width: u32, height: u32) -> (Rc<Page>, WebWindow) {
        let page = Page::new(PageSize::new(width, height), (800, 600), 100);
        (Rc::clone(&page), WebWindow::new(page))
    }

    /// The option store with the drawing scale registered at `start` percent, as the client
    /// registers it when it starts in a browser.
    fn drawing_scale_at(start: u32) {
        store::init();
        drawing_scale::register(start);
    }

    fn modes(window: &WebWindow) -> Vec<(u32, u32)> {
        window
            .display_modes()
            .iter()
            .map(|m| (m.width, m.height))
            .collect()
    }

    /// The page is the client's screen: the resolutions offered are every standard one that fits
    /// the page and the page's own size, whatever size the canvas is drawn at now.
    ///
    /// Behaviour: window.full-screen.in-a-browser-it-fills-the-page-at-its-own-pixels
    #[test]
    fn the_resolutions_offered_are_the_ones_that_fit_the_page_and_the_page_s_own() {
        let (_, w) = window(1920, 1080);
        assert_eq!(w.client_size(), (800, 600));
        let offered = modes(&w);
        for fits in [
            (800, 600),
            (1024, 768),
            (1280, 720),
            (1600, 900),
            (1920, 1080),
        ] {
            assert!(offered.contains(&fits), "{fits:?} in {offered:?}");
        }
        assert!(!offered.contains(&(1920, 1200)) && !offered.contains(&(2560, 1440)));
        assert_eq!(offered.iter().filter(|m| **m == (1920, 1080)).count(), 1);
        let screen = w.screen_metrics((0, 0, 0));
        assert_eq!((screen.cx_screen, screen.cy_screen), (1920, 1080));

        let (_, odd) = window(2880, 1754);
        assert!(modes(&odd).contains(&(2880, 1754)) && modes(&odd).contains(&(2560, 1440)));
    }

    /// Full screen draws the canvas at the page's own pixels, at the player's drawing scale, and
    /// follows the page and the scale as they change, once for each change.
    ///
    /// Behaviour: window.full-screen.in-a-browser-it-fills-the-page-at-its-own-pixels
    #[test]
    fn full_screen_fills_the_page_at_the_drawing_scale_and_follows_it() {
        drawing_scale_at(100);
        let (page, w) = window(2560, 1440);
        assert!(w.offers_full_screen());
        w.set_borderless_fullscreen(true);
        assert!(page.fills());
        assert_eq!(w.client_size(), (2560, 1440));
        assert_eq!(page.follow(), None, "already at the page's size");

        assert!(store::set_value(
            drawing_scale::DRAWING_SCALE,
            PrefValue::Int(50)
        ));
        assert_eq!(page.follow(), Some((1280, 720)));
        assert_eq!(w.client_size(), (1280, 720));
        assert_eq!(page.follow(), None, "once for each change");

        assert!(store::set_value(
            drawing_scale::DRAWING_SCALE,
            PrefValue::Int(100)
        ));
        page.set(PageSize::new(1700, 1000));
        assert_eq!(page.follow(), Some((1700, 1000)));
        page.set(PageSize::new(0, 0));
        assert_eq!(page.follow(), None, "a page with no pixels changes nothing");
        assert_eq!(w.client_size(), (1700, 1000));
        store::clear();
    }

    /// The window offers the client the drawing scale at the front page's choice, which the
    /// canvas is drawn at once the client has taken it; a change the Client Options page makes
    /// is followed at the next frame.
    ///
    /// Behaviour: window.full-screen.in-a-browser-the-drawing-scale-is-a-client-option
    #[test]
    fn the_drawing_scale_starts_at_the_front_page_s_choice_and_follows_the_options_page() {
        let page = Page::new(PageSize::new(2000, 1200), (800, 600), 75);
        let w = WebWindow::new(Rc::clone(&page));
        assert_eq!(w.drawing_scale(), Some(75));
        drawing_scale_at(75);
        w.set_borderless_fullscreen(true);
        assert_eq!(w.client_size(), (1500, 900));
        assert!(store::set_value(
            drawing_scale::DRAWING_SCALE,
            PrefValue::Int(85)
        ));
        assert_eq!(page.follow(), Some((1700, 1020)));
        assert_eq!(w.client_size(), (1700, 1020));
        store::clear();
    }

    /// Out of full screen the canvas is the size the client asks for, which the page scales to
    /// fit, and a change of the page leaves it at that size.
    ///
    /// Behaviour: window.full-screen.in-a-browser-it-fills-the-page-at-its-own-pixels
    #[test]
    fn out_of_full_screen_the_canvas_is_the_size_asked_for_and_does_not_follow_the_page() {
        let (page, w) = window(1920, 1080);
        w.set_borderless_fullscreen(true);
        w.set_borderless_fullscreen(false);
        assert!(!page.fills());
        assert_eq!(w.request_inner_size(800, 600), (800, 600));
        assert_eq!(w.client_size(), (800, 600));
        page.set(PageSize::new(1280, 1024));
        assert_eq!(page.follow(), None);
        assert_eq!(w.client_size(), (800, 600));
        w.set_borderless_fullscreen(true);
        assert_eq!(w.client_size(), (1280, 1024));
    }

    /// A page smaller than 800 by 600, or a drawing scale that would make the canvas smaller, is
    /// drawn at least that large in the page's own shape; no side is longer than the longest a
    /// canvas is drawn at, and the scale is never below half.
    ///
    /// Behaviour: window.full-screen.in-a-browser-it-fills-the-page-at-its-own-pixels
    #[test]
    fn a_small_page_is_drawn_at_least_800_by_600_in_its_own_shape() {
        assert_eq!(drawable((700, 500)), (840, 600));
        assert_eq!(drawable((600, 900)), (800, 1200));
        assert_eq!(drawable((1000, 300)), (2000, 600));
        assert_eq!(drawable((800, 600)), (800, 600));
        assert_eq!(drawable((0, 0)), (800, 800));
        assert_eq!(drawable((9000, 9000)), (LONGEST_SIDE, LONGEST_SIDE));

        drawing_scale_at(50);
        let page = Page::new(PageSize::new(1366, 768), (800, 600), 50);
        assert_eq!(page.fill_size(), (1068, 600));
        assert_eq!(page.screen(), (1366, 768));
        page.set(PageSize::new(4000, 2400));
        assert!(store::set_value(
            drawing_scale::DRAWING_SCALE,
            PrefValue::Int(5)
        ));
        assert_eq!(page.fill_size(), (2000, 1200), "half of the page at least");
        store::clear();
    }
}
