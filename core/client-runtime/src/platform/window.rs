//! The window seam: what `App` needs from a window, the events it reads, and the window a
//! `--headless` run does not have.
//!
//! The types the seam names -- `WindowHandles`, `Rect`, `ScreenMetrics` and `DisplayMode` -- are
//! plain data in `dereth-client-contract`, so `WindowHost` names no renderer and no
//! presentation crate, and neither does `NullWindow`, which is nothing but this trait's
//! defaults with an extent.
//!
//! `winit` is named in `dereth_desktop::platform::window` and `dereth_desktop::pump` and nowhere else.
//! The events the pump hands back are `HostEvent`s: plain data, no windowing type in any field.
//! [`crate::pump`] maps them onto the messages the window procedure would have seen.
//!
//! `dereth_desktop::platform::window` re-exports everything here, and
//! keeps `WinitWindow`, `open_window`, the monitor enumeration and the `winit` event mapping.

use dereth_client_contract::options::store::DisplayMode;
use dereth_client_contract::window::{Rect, ScreenMetrics};

/// One window lifecycle event, as the client reads it.
///
/// The variants are exactly the ones the window procedure's table and `App` consume; a
/// host event this list does not name is the equivalent of falling through to `DefWindowProcA`
/// and is dropped by the host rather than carried. Device input -- keys, buttons, the pointer --
/// is not a lifecycle event: a front end turns it into actions of its own accord.
#[derive(Debug, Clone, PartialEq)]
pub enum HostEvent {
    /// `WM_CLOSE`'s producer — the close button, or `DefWindowProc`'s answer to Alt+F4.
    CloseRequested,
    /// `WM_DESTROY`.
    Destroyed,
    /// `WM_ACTIVATEAPP` / `WM_ACTIVATE` / `WM_SETFOCUS`, which the host collapses into one.
    Focused(bool),
    /// `WM_SIZE`. A zero extent is `SIZE_MINIMIZED`.
    Resized { width: u32, height: u32 },
    /// `WM_DPICHANGED`. The host has already negotiated the physical client size; this is the
    /// notification that the caption metrics may have moved with it.
    ScaleFactorChanged,
}

/// What one drain produced.
#[derive(Debug, Default)]
pub struct PumpedEvents {
    /// Every event of the drain, in arrival order.
    pub events: Vec<HostEvent>,
    /// The window system reported `WM_QUIT` and its window is gone.
    pub exited: bool,
}

/// What `App` needs from a window.
///
/// Every method is either a read of the window's geometry, a `SetWindowPos`-shaped write, or the
/// event drain. No signature names a windowing library, and none names a graphics API beyond the
/// raw handle pair a device is created from.
///
/// The defaults are `NullWindow`'s: a headless run has no window, so every write is a no-op and
/// every read that needs one answers `None`. [`Self::has_window`] is the native-handle nonzero
/// test.
pub trait WindowHost {
    /// Whether the native window handle is nonzero.
    fn has_window(&self) -> bool {
        false
    }

    /// The native window's identity, which the host's cursor images find the window by; `None`
    /// without a window.
    fn raw_handle(&self) -> Option<isize> {
        None
    }

    /// The two raw handles `Gpu::new` turns into a surface.
    fn render_handles(&self) -> Option<dereth_client_contract::window::WindowHandles> {
        None
    }

    /// The client rectangle, in physical pixels.
    fn client_size(&self) -> (u32, u32);

    /// The **outer** window rectangle, or `None` when there is no
    /// window to ask, in which case `App` keeps its own maintained one.
    fn window_rect(&self) -> Option<Rect> {
        None
    }

    /// `GetSystemMetrics(SM_CXDLGFRAME / SM_CYCAPTION / SM_CYDLGFRAME)`, read off the window
    /// because this crate is `#![forbid(unsafe_code)]`. `None` when there is no window.
    fn frame_metrics(&self) -> Option<(i32, i32, i32)> {
        None
    }

    /// `GetSystemMetrics(SM_CXSCREEN / SM_CYSCREEN)` plus the monitor's origin and work area.
    fn screen_metrics(&self, frame_metrics: (i32, i32, i32)) -> ScreenMetrics;

    /// The adapter mode list used to initialize display preferences.
    fn display_modes(&self) -> Vec<DisplayMode> {
        Vec::new()
    }

    /// The window title bar.
    fn set_title(&self, _title: &str) {}

    /// `SetWindowLongA(hwnd, GWL_STYLE, ...)`'s observable half.
    ///
    /// **`GetWindowLong(GWL_STYLE)` on the running window will not read `0x92000000`, and that
    /// is the window system's doing, not a defect.** `winit`'s Windows backend implements an
    /// undecorated window by *keeping* `WS_CAPTION` in the style and overriding the non-client
    /// area in `WM_NCCALCSIZE` (`window_state.rs`: "Frameless style implemented by manually
    /// overriding the non-client area in `WM_NCCALCSIZE`"), so the style bits stay at winit's own
    /// `0x16CA0000` while the frame really is gone. `App::change_presentation` computes retail's
    /// intent; this realises it through the only API this crate has. Measured interactively:
    /// Alt+Enter took the window from `outer 816x639 / client 800x600` to
    /// `outer 3840x2160 / client 3840x2160` — client == outer, i.e. no frame at all.
    fn set_decorations(&self, _decorated: bool) {}

    /// `SetWindowPos(hwnd, HWND_TOPMOST | HWND_NOTOPMOST, ...)`.
    fn set_topmost(&self, _topmost: bool) {}

    /// Borderless full screen on the monitor the window is on, as the **window system's own**
    /// request rather than as a style change plus a move plus a resize.
    ///
    /// Retail reaches full screen by handing D3D9 a `Windowed = FALSE` presentation and letting
    /// the runtime restyle the window. There is no such runtime here, and realising the same
    /// intent with `set_decorations` + `set_outer_position` + `request_inner_size` against
    /// `dereth_render::window_proc::placement`'s rectangle is faithful arithmetic that only
    /// works on Windows. **A window cannot position itself on Wayland at all** -- there is
    /// no such protocol request, so the move is silently dropped and the window stays where the
    /// compositor put it -- and on macOS the rectangle misses the menu bar and the Dock, which
    /// are not part of the screen a window may simply cover. On both, the frame goes away and
    /// the window does not fill the screen.
    ///
    /// Every backend does implement a real fullscreen request, so that is what this is. The
    /// caller still computes retail's rectangle -- it is what the offscreen and headless paths
    /// resize to, and what the placement helper's arithmetic is tested against -- and
    /// uses this instead of the three window calls when there is a window to ask.
    ///
    /// The default is a no-op, which is the headless answer: there is no window to make full
    /// screen and [`Self::client_size`] is already whatever the caller resized the target to.
    fn set_borderless_fullscreen(&self, _full_screen: bool) {}

    /// Whether [`Self::set_borderless_fullscreen`] really makes the window full screen, so an
    /// interface may offer the switch. The default is the headless answer: no.
    fn offers_full_screen(&self) -> bool {
        false
    }

    /// Ask for a client rectangle and answer with the one the window system granted.
    fn request_inner_size(&self, width: u32, height: u32) -> (u32, u32) {
        (width, height)
    }

    /// `SetWindowPos`'s move half.
    fn set_outer_position(&self, _x: i32, _y: i32) {}

    /// drain the queue completely.
    ///
    /// `requested` and `full_screen` are inputs because the DPI negotiation happens *inside* the
    /// window system's live callback: winit's `WM_DPICHANGED` default preserves the
    /// **logical** size, and the override that keeps the selected physical pixels has to be made
    /// while the size writer is alive.
    fn pump_events(&mut self, _requested: (u32, u32), _full_screen: bool) -> PumpedEvents {
        PumpedEvents::default()
    }
}

/// The window a `--headless` run does not have.
///
/// It reports the extent it was constructed with, produces no events and applies no write: every
/// headless answer `App` needs is one of this trait's defaults.
#[derive(Debug)]
pub struct NullWindow {
    size: (u32, u32),
}

impl NullWindow {
    #[must_use]
    pub const fn new(width: u32, height: u32) -> Self {
        Self {
            size: (width, height),
        }
    }
}

impl WindowHost for NullWindow {
    fn client_size(&self) -> (u32, u32) {
        self.size
    }

    /// The virtual desktop a headless run is placed on. See [`headless_screen_metrics`].
    fn screen_metrics(&self, _frame_metrics: (i32, i32, i32)) -> ScreenMetrics {
        headless_screen_metrics()
    }
}

/// The virtual desktop a **headless** run is placed on.
///
/// A headless run has no window handle and no monitor, so `GetSystemMetrics(SM_CXSCREEN)` and
/// `SM_CYSCREEN` have no answer at all — `dereth_desktop::platform::window::monitor_metrics` is
/// never even called on that path,
/// because `App::new`'s headless arm skips the whole window block. This is the screen the
/// placement arithmetic is given instead, so that
/// `dereth_render::window_proc::change_presentation_placement` — and the test that guards its
/// "do not re-centre" behaviour — can run without a window.
///
/// The frame metrics are `(0, 0, 0)`, which is what `App::new`'s headless arm already carries in
/// `App::frame_metrics`: there is no caption and no dialog frame, so the outer rectangle *is*
/// the client rectangle, exactly as it is for a borderless full-screen window.
pub fn headless_screen_metrics() -> dereth_client_contract::window::ScreenMetrics {
    dereth_client_contract::window::ScreenMetrics {
        cx_screen: 1920,
        cy_screen: 1080,
        cx_dlg_frame: 0,
        cy_caption: 0,
        cy_dlg_frame: 0,
        work_area: None,
        origin: (0, 0),
    }
}

/// **A declared divergence: trivial sizing of windowed display modes.**
///
/// Retail's list is the adapter's *exclusive full-screen* modes, because that is the only kind of
/// resolution change a D3D9 client can make. This build's full screen is **borderless windowed**
/// and its "resolution" is the client rectangle, so any size the desktop can hold is a legal
/// choice.
///
/// So when the adapter enumeration answers nothing — a headless run, or a backend that reports
/// no modes — this standard table stands in rather than leaving the drop-down empty. It is the
/// same shape retail's own list has (every entry is at least
/// [`store::MIN_MODE_WIDTH`](dereth_client_contract::options::store::MIN_MODE_WIDTH) x
/// `MIN_MODE_HEIGHT`) and it goes through the identical
/// `initialize_display_preferences` filter, dedupe, sort and 1024x768 append, so nothing
/// downstream can tell which source it came from.
///
/// The original list also injects a mode the adapter never reported: 1024x768 is appended
/// unconditionally, whatever the driver said.
pub const STANDARD_DISPLAY_MODES: [(u32, u32); 16] = [
    (800, 600),
    (1024, 768),
    (1152, 864),
    (1280, 720),
    (1280, 800),
    (1280, 960),
    (1280, 1024),
    (1360, 768),
    (1440, 900),
    (1600, 900),
    (1600, 1200),
    (1680, 1050),
    (1920, 1080),
    (1920, 1200),
    (2560, 1440),
    (3840, 2160),
];
