//! The window, the display it sits on, and the event pump.
//!
//! `winit` is named here and in [`crate::pump`] and nowhere else in the client's logic modules:
//! window creation, monitor enumeration, the event loop's handlers, presentation-time window
//! placement, the application icon on each of the three platforms, the dat cursors as the window
//! system's own, and the fly-cam's raw key arms all live here, so the application state machine can
//! be compiled, read and driven without a window system. `WindowHost` is what `App` actually needs from a window; the two
//! implementations are the real one (`WinitWindow`) and the headless one (`NullWindow`).
//!
//! The events the pump hands back are `HostEvent`s: plain data, no windowing type in any field.
//! `crate::pump` maps them onto the Win32-style `MSG`s consumed by the input pipeline, which is
//! the adapter — *"`winit` events in, Win32 messages out"* — one layer down.
//!
//! `PumpedEvents`, `WindowHost`, `NullWindow` and `headless_screen_metrics` are
//! [`dereth_client_runtime::platform::window`]'s, re-exported here. The runtime's host event is the
//! window's lifecycle alone; `dereth_input::host::HostEvent` adds the devices. What is here is
//! `winit`: the real window, monitor enumeration, icon and event mapping.
//!
//! **The icon is a different object on each platform**, which is why it has an arm per platform
//! and not one: Windows reads it out of the executable's resource section (`build.rs` links it
//! there and the window loads that same resource), X11 carries it on the window as pixels, and
//! macOS reads it out of the application bundle, so this module has nothing to set there. The
//! three start from one picture, kept in `assets/` in the three forms those platforms read.
//!
//! **The window is made inside its event loop.** The loop is started first, and the window is
//! made when the loop reports itself running, which every desktop backend does on its first
//! drain; [`open_window`](crate::platform::window::open_window) drains until it has. The window and the loop are then shared with the
//! cursor images ([`DesktopWindow`](crate::platform::window::DesktopWindow)), which make cursors on the loop between drains.
//!
//! **The window keeps its events.** `WinitWindow` hands the runtime no events from its drain;
//! it queues every one, lifecycle and device alike and in arrival order, on the `WindowEvents`
//! it shares with the front end, which routes them when the runtime asks it to
//! (`Shell::window_input`). One queue keeps the order: a key pressed before a focus loss is
//! released by that loss, and an Alt held before a close request swallows it.

/// The runtime's window seam, the shared event, its lifecycle half and the window's event queue:
/// the client shell's, which every platform shares.
pub use dereth_client_shell::platform::window::{
    headless_screen_metrics, lifecycle, HostEvent, NullWindow, PumpedEvents, WindowEvents,
    WindowHost, STANDARD_DISPLAY_MODES,
};

use crate::platform::keys::{Key, MouseButton};
use dereth_client_contract::options::store::DisplayMode;
use dereth_render::window_proc::{Rect, ScreenMetrics};

/// `GetSystemMetrics(SM_CXSCREEN / SM_CYSCREEN)` plus the monitor's own
/// origin and usable rectangle, out of `winit` and dereth-render's safe Win32 wrapper — this crate is
/// `#![forbid(unsafe_code)]`.
///
/// The original client reads the primary work area with `SystemParametersInfoA(SPI_GETWORKAREA)`.
/// This rebuild deliberately supports non-primary monitor coordinates, so it queries the selected
/// monitor with `GetMonitorInfoW` instead. Query failure takes the same no-clamp arm.
///
/// Falls back to the requested extent when there is no monitor at all (a headless run, or a
/// session with the display asleep), which keeps the arithmetic defined.
fn monitor_metrics(
    monitor: Option<&winit::monitor::MonitorHandle>,
    frame_metrics: (i32, i32, i32),
) -> dereth_render::window_proc::ScreenMetrics {
    let (size, pos, work_area) = monitor.map_or(((800u32, 600u32), (0, 0), None), |m| {
        let s = m.size();
        let p = m.position();
        // The work area is a Win32 notion (`SPI_GETWORKAREA`); winit exposes no equivalent on
        // X11, Wayland or macOS, so those take retail's no-work-area branch.
        #[cfg(windows)]
        let work_area = {
            use winit::platform::windows::MonitorHandleExtWindows as _;
            dereth_render::window_proc::monitor_work_area(m.hmonitor())
        };
        #[cfg(not(windows))]
        let work_area = None;
        ((s.width, s.height), (p.x, p.y), work_area)
    });
    dereth_render::window_proc::ScreenMetrics {
        cx_screen: i32::try_from(size.0).unwrap_or(i32::MAX),
        cy_screen: i32::try_from(size.1).unwrap_or(i32::MAX),
        cx_dlg_frame: frame_metrics.0,
        cy_caption: frame_metrics.1,
        cy_dlg_frame: frame_metrics.2,
        work_area,
        origin: pos,
    }
}

/// `IDI_APP`, the icon ordinal `dereth-client.rc` gives the application icon. Keep the two in step:
/// nothing checks at build time that the resource exists, and a missing one is a silent fall back
/// to the generic executable icon -- which is exactly what a window with no icon at all looks like.
#[cfg(windows)]
const IDI_APP: u16 = 1;

/// Put the executable's own icon on the window.
///
/// `build.rs` links `assets/dereth.ico` into the binary, which is enough for Explorer, the taskbar
/// and Alt-Tab -- they read the icon out of the *file* before the process starts. A **window** is
/// a different thing: its icon comes from `WM_SETICON` or from the class winit registered, and
/// winit registers its class with none, so a window nobody sets an icon on gets the system
/// default. That is why the icon was visible on the exe and absent from the title bar.
///
/// Two sizes, because Windows asks for two and scales badly when it has to invent one: `ICON_SMALL`
/// (the title bar and the Alt-Tab row) at 16x16 and `ICON_BIG` (the taskbar button) at 32x32.
/// `assets/dereth.ico` carries both as real frames, so `LoadImage` picks rather than resamples.
///
/// A failure here is not a startup failure. `Icon::from_resource` can only fail when the resource
/// is missing, and a client that runs with the wrong icon is worth more than one that refuses to
/// start over a picture; the reason is printed instead, because a silent generic icon is precisely
/// the symptom that sent this round-trip.
#[cfg(windows)]
fn with_application_icon(
    builder: winit::window::WindowAttributes,
    _look: &WindowLook,
) -> winit::window::WindowAttributes {
    use winit::platform::windows::{IconExtWindows as _, WindowAttributesExtWindows as _};

    let load = |w: u32, h: u32| {
        winit::window::Icon::from_resource(IDI_APP, Some(winit::dpi::PhysicalSize::new(w, h)))
            .map_err(|e| tracing::warn!("icon resource {IDI_APP} at {w}x{h}: {e}"))
            .ok()
    };
    builder
        .with_window_icon(load(16, 16))
        .with_taskbar_icon(load(32, 32))
}

/// The window's icon, as pixels, for the window systems that want it that way.
///
/// X11 carries the icon on the window itself (`_NET_WM_ICON`, which is exactly this: width,
/// height and straight RGBA), so a client that sets it is a client whose task bar, window list
/// and Alt-Tab are right whether or not the session has a `.desktop` file installed for it.
/// Wayland is the exception -- a Wayland surface has no icon property, and the compositor takes
/// the picture from the `.desktop` entry the application was launched from -- so winit answers
/// that request by ignoring it, and nothing here is wasted but the decode.
/// The product's icon pixels ([`WindowLook::icon_png`]) decoded, or `None` with the reason logged.
///
/// **One size, and a large one.** `_NET_WM_ICON` is a *list* of images and the window manager
/// picks the one nearest the size it is about to draw -- but `winit`'s window attribute holds a
/// single `Icon` and its X11 backend writes a single entry, so the choice is not how many but
/// which. It is 256 because the consumers differ by an order of magnitude: a task-bar button is
/// 22 to 48 pixels and a desktop's window switcher or overview draws 128 or 256. Going down from
/// 256 is a resample every toolkit does well; coming up from 64 is blur that cannot be undone.
/// The cost of the larger one is an image sent once, when the window is created.
#[cfg(not(any(windows, target_os = "macos")))]
fn window_icon(icon_png: &[u8]) -> Option<winit::window::Icon> {
    let mut reader = png::Decoder::new(std::io::Cursor::new(icon_png))
        .read_info()
        .map_err(|e| tracing::warn!("window icon: {e}"))
        .ok()?;
    let mut pixels = vec![0; reader.output_buffer_size()];
    let frame = reader
        .next_frame(&mut pixels)
        .map_err(|e| tracing::warn!("window icon: {e}"))
        .ok()?;
    // The asset is 8-bit RGBA and `Icon::from_rgba` takes nothing else. Checked rather than
    // assumed, because the check costs one comparison and the alternative is a window wearing
    // whatever a misread buffer happens to look like.
    if (frame.color_type, frame.bit_depth) != (png::ColorType::Rgba, png::BitDepth::Eight) {
        tracing::warn!(
            "window icon: expected 8-bit RGBA, got {:?} {:?}",
            frame.color_type,
            frame.bit_depth
        );
        return None;
    }
    pixels.truncate(frame.buffer_size());
    winit::window::Icon::from_rgba(pixels, frame.width, frame.height)
        .map_err(|e| tracing::warn!("window icon: {e}"))
        .ok()
}

/// Put the icon on the window: X11 and the other window systems that take one.
#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
fn with_application_icon(
    builder: winit::window::WindowAttributes,
    look: &WindowLook,
) -> winit::window::WindowAttributes {
    builder.with_window_icon(window_icon(look.icon_png))
}

/// On Linux the icon is also found by name: the window carries the product's application id (its
/// Wayland app id and its X11 class), and the desktop takes the icon from the `.desktop` entry of
/// that name. A Wayland window has no other way to show one; the product writes the entry before
/// the window opens (`Product::before_window`).
#[cfg(target_os = "linux")]
fn with_application_icon(
    builder: winit::window::WindowAttributes,
    look: &WindowLook,
) -> winit::window::WindowAttributes {
    use winit::platform::wayland::WindowAttributesExtWayland as _;

    builder
        .with_window_icon(window_icon(look.icon_png))
        .with_name(look.app_id, look.app_id)
}

/// macOS puts no icon on a window at all -- a Cocoa window shows one only for a document, and
/// that one is the document's -- so there is nothing to add to the builder here, and nothing this
/// process can usefully do at run time either.
///
/// **An application is a bundle on this platform**, and its icon is read out of that bundle by
/// Finder, the Dock and the switcher before the process starts, exactly as Windows reads one out
/// of the executable. `cargo xtask bundle-macos` builds it, from `assets/Dereth.icon`. A binary
/// run straight out of `target/` is not an application in that sense and wears the generic
/// executable icon, which is the platform's answer and not a gap in this module.
#[cfg(target_os = "macos")]
fn with_application_icon(
    builder: winit::window::WindowAttributes,
    _look: &WindowLook,
) -> winit::window::WindowAttributes {
    builder
}

/// The adapter mode list used to build the resolution and refresh-rate drop-downs.
///
/// Retail fills it in, from D3D9's
/// `IDirect3D9::EnumAdapterModes`. This build has no D3D9 and this crate is
/// `#![forbid(unsafe_code)]`, so the same enumeration arrives through `winit`'s
/// `MonitorHandle::video_modes`, which on Windows is `EnumDisplaySettingsExW` over the monitor's
/// device — the same driver list D3D9 reports, one layer down.
///
/// Two conversions, both stated:
///
/// * **`refresh_rate_millihertz` to whole Hz**, rounded to nearest. The client's display-mode
///   record holds the refresh rate as an integer, and a driver that reports 59 940 mHz means 60.
/// * **`bit_depth` as the format's bits-per-pixel value.** The compatibility gate uses the *depth*
///   of the mode's pixel format and no other property of the format, so the depth is what this
///   carries. A monitor reporting a depth of 0 (which some backends do) is filtered out by the
///   same `< 32` comparison the client uses, which is why the fall-back below exists.
fn adapter_display_modes(
    monitor: Option<&winit::monitor::MonitorHandle>,
) -> Vec<dereth_client_contract::options::store::DisplayMode> {
    use dereth_client_contract::options::store::DisplayMode;

    let Some(m) = monitor else { return Vec::new() };
    m.video_modes()
        .map(|v| {
            let s = v.size();
            DisplayMode {
                width: s.width,
                height: s.height,
                refresh_rate: v.refresh_rate_millihertz().saturating_add(500) / 1000,
                bits_per_pixel: u32::from(v.bit_depth()),
            }
        })
        .collect()
}

// -------------------------------------------------------------------------------------------
// The real window
// -------------------------------------------------------------------------------------------

/// The window and the event loop it belongs to, shared by the [`WinitWindow`] that pumps the loop
/// and the cursor images that make cursors on it ([`cursor_window`]).
///
/// The window is declared first so it is dropped first, before the loop it was made on.
pub struct DesktopWindow {
    window: winit::window::Window,
    /// Borrowed mutably only while a drain runs; a cursor is made between drains.
    event_loop: std::cell::RefCell<winit::event_loop::EventLoop<()>>,
}

impl std::fmt::Debug for DesktopWindow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DesktopWindow")
            .field("window", &self.window.id())
            .finish_non_exhaustive()
    }
}

/// A dat cursor's picture as the window system takes it, before there is a cursor made from it:
/// what building a cursor needs no window for.
#[derive(Debug)]
pub struct CursorPicture(winit::window::CustomCursorSource);

/// A cursor the window system made from a [`CursorPicture`]; cheap to clone, and freed with its
/// last clone.
#[derive(Debug, Clone)]
pub struct HostCursor(winit::window::CustomCursor);

/// The picture of the cursor `bits` describes, with its hotspot.
///
/// The picture is the 32x32 image the icon builder made, so a cursor keeps its size, its
/// placement in the top-left corner and its transparent surround, and every pixel is either
/// shown or transparent ([`dereth_render::cursor::IconBits::rgba`]). The window system refuses a
/// hotspot outside the image, which a 32x32 cursor's bitmap pair would have accepted; one is held
/// to the image's last row or column instead, the nearest place the window system will take.
///
/// # Errors
/// The window system's reason when it will not take the picture.
pub fn cursor_picture(bits: &dereth_render::cursor::IconBits) -> Result<CursorPicture, String> {
    let extent = u16::try_from(dereth_render::cursor::CURSOR_EXTENT).unwrap_or(u16::MAX);
    winit::window::CustomCursor::from_rgba(
        bits.rgba(),
        extent,
        extent,
        picture_hotspot(bits.hot_x),
        picture_hotspot(bits.hot_y),
    )
    .map(CursorPicture)
    .map_err(|e| e.to_string())
}

/// One coordinate of a cursor's hotspot inside its 32x32 picture: itself, or the last row or
/// column when it is past the picture's edge.
fn picture_hotspot(h: u32) -> u16 {
    let last = dereth_render::cursor::CURSOR_EXTENT - 1;
    u16::try_from(h.min(last)).unwrap_or(0)
}

impl DesktopWindow {
    /// Make the window system's cursor from `picture`; `None` while the event loop is draining,
    /// which no caller does.
    #[must_use]
    pub fn make_cursor(&self, picture: CursorPicture) -> Option<HostCursor> {
        let event_loop = self.event_loop.try_borrow().ok()?;
        Some(HostCursor(event_loop.create_custom_cursor(picture.0)))
    }

    /// Make `cursor` the pointer's image over the window's client area.
    ///
    /// The window system keeps it there: on Windows the window answers every `WM_SETCURSOR` in
    /// the client area with it, and the frame and border keep the system's sizing cursors; X11,
    /// Wayland and macOS attach it to the window's surface.
    pub fn set_cursor(&self, cursor: &HostCursor) {
        self.window.set_cursor(cursor.0.clone());
    }
}

std::thread_local! {
    /// The window this thread opened, by its [`WindowHost::raw_handle`], for the cursor images.
    static CURSOR_WINDOW: std::cell::RefCell<Option<(isize, std::rc::Weak<DesktopWindow>)>> =
        const { std::cell::RefCell::new(None) };
}

/// The window opened on this thread whose [`WindowHost::raw_handle`] is `handle`, for the cursor
/// images to put cursors on. The host is handed the handle, not the window, so this is how the
/// cursor images reach it. It is a weak reference: the window closes when its [`WinitWindow`] is
/// dropped, whoever still holds cursors for it.
#[must_use]
pub fn cursor_window(handle: isize) -> Option<std::rc::Weak<DesktopWindow>> {
    CURSOR_WINDOW.with(|w| {
        w.borrow()
            .as_ref()
            .filter(|(h, window)| *h == handle && window.strong_count() > 0)
            .map(|(_, window)| window.clone())
    })
}

/// The window, its event loop, and the swap chain's handle.
pub struct WinitWindow {
    shared: std::rc::Rc<DesktopWindow>,
    /// Where the drain puts its events. See the module documentation.
    events: WindowEvents,
    /// Whether this module, rather than the window system's backend, keeps the fixed window's
    /// size and size limits: true on Wayland. See [`resize_fixed_window`].
    pins_size_limits: bool,
    /// The windowed size last asked for, and what is left of its second ask.
    held: HeldSize,
}

/// What a size change of the fixed-size window touches: its size limits, its size, in physical
/// pixels, and its full-screen state. `WinitWindow` is the real one; the tests model a compositor.
trait FixedSizeWindow {
    /// Hold the window's smallest and largest client size at `size`, or release them (`None`).
    fn set_size_limits(&self, size: Option<(u32, u32)>);
    /// Ask for a client size; answers the size the window has after asking.
    fn request_client_size(&self, size: (u32, u32)) -> (u32, u32);
    /// Whether the window system last configured the window full screen.
    fn is_full_screen(&self) -> bool;
    /// Ask the window system to make the window full screen, or to stop making it so.
    fn set_full_screen(&self, full_screen: bool);
}

/// What this module remembers about the fixed window's size from one frame to the next.
#[derive(Debug, Default)]
struct HeldSize {
    /// The windowed client size last asked for; `None` while the window is full screen.
    requested: std::cell::Cell<Option<(u32, u32)>>,
    /// How many more times a resize to another size is answered by asking for `requested` again.
    asks_left: std::cell::Cell<u8>,
    /// Whether full screen was asked for and not yet left. The window system may not have
    /// answered that request yet, so its own state is not enough to decide there is nothing to
    /// leave.
    full_screen_asked: std::cell::Cell<bool>,
}

/// Give the window the player cannot resize a new client size.
///
/// On Wayland a window that is not resizable is one whose smallest and largest size are both held
/// at its size, and the backend sets them once, when the window is created. A size request there
/// commits the new size with the limits still saying the old one, and the compositor answers by
/// configuring the window back to it. So there (`pin_limits`) the limits move to the new size
/// first, in the same commit as the size, and one answer at a different size is met by asking
/// again ([`settle_resized`]). On X11 the backend moves the limits itself with each request, and
/// Windows and macOS keep no such limits for a fixed window, so elsewhere the request alone is
/// enough.
fn resize_fixed_window(
    window: &impl FixedSizeWindow,
    held: &HeldSize,
    size: (u32, u32),
    pin_limits: bool,
) -> (u32, u32) {
    held.requested.set(Some(size));
    held.asks_left.set(u8::from(pin_limits));
    if pin_limits {
        window.set_size_limits(Some(size));
    }
    window.request_client_size(size)
}

/// Enter or leave full screen.
///
/// Entering releases the size limits this module holds (`pin_limits`), so the monitor's size is
/// not refused as larger than the windowed one; leaving puts them back, through
/// [`resize_fixed_window`].
///
/// **On Wayland a window that is not full screen is not asked to leave it.** Every windowed
/// presentation change passes through here, and a compositor may answer a request to leave full
/// screen with a configure event whatever the window's state; KWin does. While that configure is
/// outstanding the compositor disregards the size the window commits, and the configure then
/// carries the size the compositor last knew, which the backend applies as it arrives: the size
/// asked for in the same frame is undone. Elsewhere the request is made as it always was.
fn set_fixed_window_full_screen(
    window: &impl FixedSizeWindow,
    held: &HeldSize,
    full_screen: bool,
    pin_limits: bool,
) {
    if full_screen {
        if pin_limits {
            window.set_size_limits(None);
        }
        held.requested.set(None);
        held.full_screen_asked.set(true);
        window.set_full_screen(true);
        return;
    }
    if pin_limits && !held.full_screen_asked.get() && !window.is_full_screen() {
        tracing::debug!("windowed already; not asked to leave full screen");
        return;
    }
    held.full_screen_asked.set(false);
    window.set_full_screen(false);
}

/// The window system resized the window to `size`; answers the size the window has now.
///
/// Where this module holds the window's size (`pin_limits`), a windowed resize to a size other
/// than the one last asked for is met by asking for that size again, once per request. That is the
/// compositor's answer to the window leaving full screen: it restores the windowed size it knew,
/// and the size asked for in the same change could not be applied while the window was still full
/// screen. The single ask bounds it: a compositor that answers again with its own size is
/// believed. A zero size (a minimised window) is not a resize.
fn settle_resized(
    window: &impl FixedSizeWindow,
    held: &HeldSize,
    size: (u32, u32),
    pin_limits: bool,
) -> (u32, u32) {
    let Some(requested) = held.requested.get() else {
        return size;
    };
    if size == requested || size.0 == 0 || size.1 == 0 {
        return size;
    }
    if pin_limits && held.asks_left.get() > 0 {
        held.asks_left.set(held.asks_left.get() - 1);
        window.set_size_limits(Some(requested));
        let applied = window.request_client_size(requested);
        tracing::debug!(
            "window resized to {}x{}, not the {}x{} requested; asked again, {}x{} answered",
            size.0,
            size.1,
            requested.0,
            requested.1,
            applied.0,
            applied.1
        );
        return applied;
    }
    tracing::debug!(
        "window resized to {}x{}, not the {}x{} requested; kept",
        size.0,
        size.1,
        requested.0,
        requested.1
    );
    size
}

impl FixedSizeWindow for winit::window::Window {
    fn set_size_limits(&self, size: Option<(u32, u32)>) {
        let size = size.map(|(w, h)| winit::dpi::PhysicalSize::new(w, h));
        self.set_min_inner_size(size);
        self.set_max_inner_size(size);
    }

    fn request_client_size(&self, size: (u32, u32)) -> (u32, u32) {
        let applied = self
            .request_inner_size(winit::dpi::PhysicalSize::new(size.0, size.1))
            .unwrap_or_else(|| self.inner_size());
        (applied.width, applied.height)
    }

    fn is_full_screen(&self) -> bool {
        self.fullscreen().is_some()
    }

    /// `Fullscreen::Borderless(None)` is "the monitor this window is on", which is the one thing
    /// the three backends agree on and the one thing the style-and-rectangle route could not
    /// express. `None` for the monitor rather than a handle: on Wayland the window does not know
    /// which output it is on, and letting the compositor decide is the correct answer there.
    fn set_full_screen(&self, full_screen: bool) {
        self.set_fullscreen(full_screen.then_some(winit::window::Fullscreen::Borderless(None)));
    }
}

/// Whether `window` is a Wayland surface: the one backend whose size requests leave a fixed
/// window's size limits where they were, and whose compositor may undo a size asked for in the
/// same frame as a request to leave full screen.
fn is_wayland(window: &winit::window::Window) -> bool {
    use winit::raw_window_handle::{HasWindowHandle as _, RawWindowHandle};
    window
        .window_handle()
        .is_ok_and(|h| matches!(h.as_raw(), RawWindowHandle::Wayland(_)))
}

impl std::fmt::Debug for WinitWindow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WinitWindow")
            .field("window", &self.shared.window.id())
            .finish()
    }
}

/// What a product's window wears: its title, its icon and, on Linux, its application id.
#[derive(Debug, Clone, Copy)]
pub struct WindowLook {
    /// The title, before the account name the title adds (`Config::window_title_for`).
    pub title: &'static str,
    /// The icon, as an 8-bit RGBA PNG, for the window systems that take one as pixels. (Windows
    /// takes the executable's own icon resource instead, and macOS the application bundle's.)
    pub icon_png: &'static [u8],
    /// The application id the desktop finds the window's `.desktop` entry by, on Linux.
    pub app_id: &'static str,
}

/// The window the startup sequence asks for, on `monitor`.
fn window_attributes(
    cfg: &dereth_client_runtime::config::Config,
    look: &WindowLook,
    monitor: Option<&winit::monitor::MonitorHandle>,
) -> winit::window::WindowAttributes {
    use winit::dpi::PhysicalSize;
    use winit::window::{WindowButtons, WindowLevel};

    // The documented style is 0x12CA0000 = WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX |
    // WS_CLIPCHILDREN | WS_VISIBLE. **There is no WS_THICKFRAME and no WS_MAXIMIZEBOX**,
    // which is why the client's window procedure has no size handling; `with_resizable(false)` drops both and
    // `with_enabled_buttons` keeps close and minimise.
    //
    // The size is the client area: startup requests 800x600 and then adds the
    // frame metrics to reach the outer size (`window_proc::outer_size`), which is what
    // `with_inner_size` asks winit to do.
    //
    // **The window is created windowed, whatever `Display.FullScreen` says**, and that is
    // the original startup order rather than a departure from it. The initial window request asks for
    // a window whenever the windowed flag is set, which it is on the default path, so the window always
    // comes up framed. The later display-preference load ->
    // graphics-engine startup handing D3D9 a `Windowed = FALSE` presentation that makes the
    // **D3D9 runtime** restyle and resize it afterwards.
    //
    // This build goes one step further on purpose: full screen is applied when the player
    // **enters the game** and taken away when they leave it, so the whole pre-game flow --
    // login, character select, the intro -- is windowed. `App::follow_gameplay_full_screen` is
    // the other half; this function's job is only to not pre-empt it, so
    // `cfg.display.full_screen` is *not* read here.
    let screen = monitor_metrics(monitor, (0, 0, 0));
    let place = dereth_render::window_proc::placement(
        false,
        true,
        i32::try_from(cfg.width).unwrap_or(i32::MAX),
        i32::try_from(cfg.height).unwrap_or(i32::MAX),
        &screen,
    );
    let (client_w, client_h) = (cfg.width, cfg.height);
    // **A declared divergence** (client divergence CD-006). The original window title is a
    // fixed constant and never changes; this build puts the
    // account on it, because several clients are often up at once on one machine -- for
    // example a two-account test of trade or housing -- and the task bar is the only place
    // that tells them apart. See `Config::window_title`.
    let attributes = winit::window::Window::default_attributes()
        .with_title(cfg.window_title_for(look.title))
        .with_inner_size(PhysicalSize::new(client_w, client_h))
        .with_resizable(false)
        .with_maximized(false)
        .with_enabled_buttons(WindowButtons::CLOSE | WindowButtons::MINIMIZE)
        .with_window_level(if place.topmost {
            // `SetWindowPos(hWnd, HWND_TOPMOST, ...)` — startup step 10 and the presentation
            // change's final call both do this in full screen.
            //
            // **Unreachable by design.** This build does not keep the window topmost:
            // `window_proc::placement` answers `topmost: false` in both modes. The arm is kept
            // because it is the one line that would restore the retail behaviour if that choice
            // is reversed. Do not delete the field.
            WindowLevel::AlwaysOnTop
        } else {
            WindowLevel::Normal
        })
        .with_decorations(place.style & dereth_render::window_proc::WS_POPUP == 0)
        .with_visible(true);
    // After the rest of the attributes: the icon is the window's, so it has to be on the
    // attributes winit creates the window from.
    with_application_icon(attributes, look)
}

/// How long the startup sequence waits for the window system to let it make its window.
const OPEN_DEADLINE: std::time::Duration = std::time::Duration::from_secs(5);

/// The startup sequence's window-creation half.
///
/// The window is made inside the event loop, when the loop reports itself running (`resumed`),
/// which every desktop backend does on the first drain; the events the new window raises while
/// that drain lasts -- its first focus, its first size -- are queued as any later drain's are.
///
/// # Errors
/// The window system's reason when the event loop or the window cannot be created. `App` turns it
/// into a `StartupError::Device`.
pub fn open_window(
    cfg: &dereth_client_runtime::config::Config,
    look: &WindowLook,
    events: WindowEvents,
) -> Result<WinitWindow, String> {
    use winit::platform::pump_events::{EventLoopExtPumpEvents, PumpStatus};

    let mut builder = winit::event_loop::EventLoop::builder();
    // **A deliberate modernization** (client divergence CD-004). Display resolutions name
    // physical client pixels, not retail's DPI-virtualized logical pixels. winit's
    // Windows backend enables per-monitor awareness (V2 when available) before creating
    // the window; the PhysicalSize requests below then mean the same pixels as the
    // back buffer. X11, Wayland and macOS report physical pixels through `inner_size`
    // without being asked.
    #[cfg(windows)]
    {
        use winit::platform::windows::EventLoopBuilderExtWindows;
        builder.with_dpi_aware(true);
    }
    let mut event_loop = builder.build().map_err(|e| format!("event loop: {e}"))?;

    let held = HeldSize::default();
    let mut opening = Opening {
        cfg,
        look,
        opened: None,
        queue: Vec::new(),
        held: &held,
    };
    let deadline = std::time::Instant::now() + OPEN_DEADLINE;
    let mut wait = std::time::Duration::ZERO;
    let (window, pins_size_limits) = loop {
        let status = event_loop.pump_app_events(Some(wait), &mut opening);
        if let Some(opened) = opening.opened.take() {
            break opened?;
        }
        if let PumpStatus::Exit(code) = status {
            return Err(format!(
                "window: the event loop ended ({code}) before it was made"
            ));
        }
        if std::time::Instant::now() >= deadline {
            return Err("window: the window system never let the event loop make it".to_string());
        }
        wait = std::time::Duration::from_millis(10);
    };
    events.borrow_mut().append(&mut opening.queue);

    // The process is DPI-aware (see above), so these are physical pixels; an OS-side query of the
    // window's geometry stays the independent measurement if an external compatibility override
    // prevents the awareness request.
    tracing::debug!(
        "window inner={:?} outer={:?} scale_factor={} (physical-pixel \
         resolutions; per-monitor DPI awareness requested)",
        window.inner_size(),
        window.outer_size(),
        window.scale_factor()
    );

    // The three values startup measures off the window while there is still a frame to
    // measure are read back through the trait, by `App::bring_up`: [`WinitWindow::client_size`],
    // [`WinitWindow::frame_metrics`] and [`WinitWindow::window_rect`], which are those same three
    // expressions and are also what a later frame re-reads.
    let shared = std::rc::Rc::new(DesktopWindow {
        window,
        event_loop: std::cell::RefCell::new(event_loop),
    });
    let opened = WinitWindow {
        shared,
        events,
        pins_size_limits,
        held,
    };
    if let Some(handle) = opened.raw_handle() {
        let weak = std::rc::Rc::downgrade(&opened.shared);
        CURSOR_WINDOW.with(|w| *w.borrow_mut() = Some((handle, weak)));
    }
    Ok(opened)
}

/// The event loop's handler while the window is being made: it makes the window when the loop
/// is running, and routes the window's first events as [`Drain`] routes every later one.
struct Opening<'a> {
    cfg: &'a dereth_client_runtime::config::Config,
    look: &'a WindowLook,
    /// The window and whether its size limits are held here, or why it could not be made.
    opened: Option<Result<(winit::window::Window, bool), String>>,
    queue: Vec<HostEvent>,
    held: &'a HeldSize,
}

impl winit::application::ApplicationHandler for Opening<'_> {
    fn new_events(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _cause: winit::event::StartCause,
    ) {
        event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);
    }

    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if self.opened.is_some() {
            return;
        }
        let monitor = event_loop.primary_monitor();
        let attributes = window_attributes(self.cfg, self.look, monitor.as_ref());
        self.opened = Some(
            event_loop
                .create_window(attributes)
                .map(|window| {
                    let pins = is_wayland(&window);
                    (window, pins)
                })
                .map_err(|e| format!("window: {e}")),
        );
    }

    fn window_event(
        &mut self,
        _event_loop: &winit::event_loop::ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        let Some(Ok((window, pins))) = &self.opened else {
            return;
        };
        let requested = winit::dpi::PhysicalSize::new(self.cfg.width, self.cfg.height);
        route_window_event(
            window,
            self.held,
            *pins,
            (requested, false),
            &mut self.queue,
            event,
        );
    }
}

/// The event loop's handler for one drain: every window event, routed onto the queue.
struct Drain<'a> {
    window: &'a winit::window::Window,
    held: &'a HeldSize,
    pins_size_limits: bool,
    /// The client size last asked for, and whether the window is full screen.
    presentation: (winit::dpi::PhysicalSize<u32>, bool),
    queue: &'a mut Vec<HostEvent>,
}

impl winit::application::ApplicationHandler for Drain<'_> {
    fn new_events(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _cause: winit::event::StartCause,
    ) {
        event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);
    }

    /// The window is made once, by [`open_window`]; a loop resumed later has nothing to make.
    fn resumed(&mut self, _event_loop: &winit::event_loop::ActiveEventLoop) {}

    fn window_event(
        &mut self,
        _event_loop: &winit::event_loop::ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        route_window_event(
            self.window,
            self.held,
            self.pins_size_limits,
            self.presentation,
            self.queue,
            event,
        );
    }
}

/// One window event onto the queue, with the two the window answers itself on the way: a DPI
/// change's size, and a resize the fixed window settles.
///
/// `presentation` is the client size last asked for and whether the window is full screen.
fn route_window_event(
    window: &winit::window::Window,
    held: &HeldSize,
    pins_size_limits: bool,
    presentation: (winit::dpi::PhysicalSize<u32>, bool),
    queue: &mut Vec<HostEvent>,
    mut event: winit::event::WindowEvent,
) {
    let (requested, full_screen) = presentation;
    if let winit::event::WindowEvent::ScaleFactorChanged {
        inner_size_writer, ..
    } = &mut event
    {
        // winit's WM_DPICHANGED default preserves *logical* size. Override it while this writer
        // is live (it expires on callback return), keeping selected physical pixels windowed and
        // the monitor extent borderless.
        let size = if full_screen {
            window
                .current_monitor()
                .map_or_else(|| window.inner_size(), |m| m.size())
        } else {
            requested
        };
        if let Err(e) = inner_size_writer.request_inner_size(size) {
            tracing::warn!("DPI physical client-size request failed: {e}");
        }
        // Where the size limits are held here, they are held in the old scale's units; hold
        // them again at the same physical size in the new one.
        if pins_size_limits && !full_screen {
            window.set_size_limits(Some((requested.width, requested.height)));
        }
    }
    if let winit::event::WindowEvent::Resized(size) = &event {
        tracing::debug!("window resized to {}x{}", size.width, size.height);
        // The size the window has once the resize is settled, which is what the back buffer
        // must follow: after a second ask it is the one asked for.
        let (width, height) =
            settle_resized(window, held, (size.width, size.height), pins_size_limits);
        queue.push(HostEvent::Resized { width, height });
        return;
    }
    queue.extend(host_event(&event));
}

impl WindowHost for WinitWindow {
    fn has_window(&self) -> bool {
        true
    }

    /// The window's identity, which the cursor images find the window by ([`cursor_window`]).
    /// On Windows it is the window handle.
    fn raw_handle(&self) -> Option<isize> {
        isize::try_from(u64::from(self.shared.window.id())).ok()
    }

    /// The two raw handles `Gpu::new` turns into a surface, read off a winit window. No `unsafe`
    /// is involved: both are plain values `raw-window-handle` hands back.
    ///
    /// It lives here rather than in `crate::gpu` so that it needs no graphics backend feature.
    /// A window that cannot produce a handle
    /// answers `None`, and `Gpu::new(None, …)` is the offscreen device, which is what the caller
    /// would have made of the error anyway.
    fn render_handles(&self) -> Option<dereth_render::device::WindowHandles> {
        window_handles(&self.shared.window)
            .map_err(|e| tracing::warn!("{e}"))
            .ok()
    }

    fn client_size(&self) -> (u32, u32) {
        let s = self.shared.window.inner_size();
        (s.width, s.height)
    }

    /// The window rectangle is read at creation, while the
    /// window has just been created, so the first presentation change has a top-left to keep, and
    /// re-read by every later presentation change. It is read off the window rather than taken
    /// from `place` because `open_window`'s windowed arm never calls `with_position`: winit puts
    /// a framed window wherever the window manager chooses, and *that* is where the player's
    /// window is.
    fn window_rect(&self) -> Option<Rect> {
        let p = self.shared.window.outer_position().ok()?;
        let o = self.shared.window.outer_size();
        Some(Rect {
            left: p.x,
            top: p.y,
            right: p.x + i32::try_from(o.width).unwrap_or(0),
            bottom: p.y + i32::try_from(o.height).unwrap_or(0),
        })
    }

    /// `GetSystemMetrics(SM_CXDLGFRAME / SM_CYCAPTION / SM_CYDLGFRAME)`,
    /// read off the window rather than asked of `user32`: this crate is
    /// `#![forbid(unsafe_code)]` and `GetSystemMetrics` is an `unsafe fn`. A decorated
    /// window's outer size *is* those metrics:
    /// `cx = Width + 2*cxDlgFrame`, `cy = Height + cyCaption + 2*cyDlgFrame`), and
    /// `SM_CXDLGFRAME == SM_CYDLGFRAME` on every Windows this targets. A borderless window has
    /// no frame at all, which is why the caller drops this for a full-screen presentation.
    fn frame_metrics(&self) -> Option<(i32, i32, i32)> {
        let window = &self.shared.window;
        let (outer, inner) = (window.outer_size(), window.inner_size());
        let frame = i32::try_from(outer.width.saturating_sub(inner.width)).unwrap_or(0) / 2;
        let caption =
            i32::try_from(outer.height.saturating_sub(inner.height)).unwrap_or(0) - 2 * frame;
        Some((frame, caption.max(0), frame))
    }

    fn screen_metrics(&self, frame_metrics: (i32, i32, i32)) -> ScreenMetrics {
        let window = &self.shared.window;
        let monitor = window
            .current_monitor()
            .or_else(|| window.primary_monitor());
        monitor_metrics(monitor.as_ref(), frame_metrics)
    }

    fn display_modes(&self) -> Vec<DisplayMode> {
        let window = &self.shared.window;
        let monitor = window
            .current_monitor()
            .or_else(|| window.primary_monitor());
        adapter_display_modes(monitor.as_ref())
    }

    fn set_title(&self, title: &str) {
        self.shared.window.set_title(title);
    }

    fn set_decorations(&self, decorated: bool) {
        self.shared.window.set_decorations(decorated);
    }

    fn set_topmost(&self, topmost: bool) {
        self.shared.window.set_window_level(if topmost {
            winit::window::WindowLevel::AlwaysOnTop
        } else {
            winit::window::WindowLevel::Normal
        });
    }

    /// The window is not resizable, so this is [`resize_fixed_window`]: on Wayland its size limits
    /// move with it.
    fn request_inner_size(&self, width: u32, height: u32) -> (u32, u32) {
        let applied = resize_fixed_window(
            &self.shared.window,
            &self.held,
            (width, height),
            self.pins_size_limits,
        );
        tracing::debug!(
            "client size {width}x{height} requested, {}x{} answered (limits held here: {}, \
             scale factor {})",
            applied.0,
            applied.1,
            self.pins_size_limits,
            self.shared.window.scale_factor()
        );
        applied
    }

    fn set_outer_position(&self, x: i32, y: i32) {
        self.shared
            .window
            .set_outer_position(winit::dpi::PhysicalPosition::new(x, y));
    }

    /// [`set_fixed_window_full_screen`]: on Wayland the size limits are released on the way in,
    /// and a window that is not full screen is not asked to leave it.
    fn set_borderless_fullscreen(&self, full_screen: bool) {
        set_fixed_window_full_screen(
            &self.shared.window,
            &self.held,
            full_screen,
            self.pins_size_limits,
        );
    }

    /// Drain the event queue completely.
    ///
    /// "the pump drains the queue completely each frame — it is not the classic `PeekMessage`-or-
    /// render alternation", which is what a zero timeout on `pump_app_events` gives.
    fn pump_events(&mut self, requested: (u32, u32), full_screen: bool) -> PumpedEvents {
        use winit::platform::pump_events::{EventLoopExtPumpEvents, PumpStatus};

        let mut queue = self.events.borrow_mut();
        let mut drain = Drain {
            window: &self.shared.window,
            held: &self.held,
            pins_size_limits: self.pins_size_limits,
            presentation: (
                winit::dpi::PhysicalSize::new(requested.0, requested.1),
                full_screen,
            ),
            queue: &mut queue,
        };
        let status = self
            .shared
            .event_loop
            .borrow_mut()
            .pump_app_events(Some(std::time::Duration::ZERO), &mut drain);
        // PumpStatus::Exit is winit's WM_QUIT. "WM_QUIT stops the drain but does not by itself
        // set the done flag" -- but by the time winit reports Exit its window is gone, so the only
        // honest response is to end the loop.
        PumpedEvents {
            events: Vec::new(),
            exited: matches!(status, PumpStatus::Exit(_)),
        }
    }
}

/// The two raw handles `Gpu::new` turns into a surface, read off a winit window. No `unsafe`
/// is involved: both are plain values `raw-window-handle` hands back.
///
/// It lives here rather than in `crate::gpu` so that it needs no graphics backend feature.
///
/// # Errors
/// The backend's reason when the window cannot produce a handle (winit's `HandleError`).
pub fn window_handles(
    window: &winit::window::Window,
) -> Result<dereth_render::device::WindowHandles, String> {
    use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle};
    let display = window
        .display_handle()
        .map_err(|e| format!("display handle: {e}"))?
        .as_raw();
    let window = window
        .window_handle()
        .map_err(|e| format!("window handle: {e}"))?
        .as_raw();
    Ok(dereth_render::device::WindowHandles { display, window })
}

/// One `winit` window event as the plain `HostEvent` the client reads, or `None` for an event
/// neither the Win32 message mapping nor `App` consumes.
///
/// This is the only place a windowing type is turned into client data, and it is a projection and
/// nothing more.
fn host_event(event: &winit::event::WindowEvent) -> Option<HostEvent> {
    use winit::event::{ElementState, MouseScrollDelta, WindowEvent};
    use winit::keyboard::{ModifiersState, PhysicalKey};
    use winit::platform::modifier_supplement::KeyEventExtModifierSupplement as _;

    Some(match event {
        WindowEvent::CloseRequested => HostEvent::CloseRequested,
        WindowEvent::Destroyed => HostEvent::Destroyed,
        WindowEvent::Focused(gained) => HostEvent::Focused(*gained),
        WindowEvent::Resized(size) => HostEvent::Resized {
            width: size.width,
            height: size.height,
        },
        WindowEvent::ScaleFactorChanged { .. } => HostEvent::ScaleFactorChanged,
        WindowEvent::ModifiersChanged(mods) => HostEvent::ModifiersChanged {
            alt: mods.state().contains(ModifiersState::ALT),
        },
        WindowEvent::KeyboardInput { event, .. } => {
            let PhysicalKey::Code(code) = event.physical_key else {
                return None;
            };
            HostEvent::KeyboardInput {
                key: key_from_key_code(code)?,
                pressed: event.state == ElementState::Pressed,
                // `KeyEvent::text` intentionally removes Control before it translates the key.
                // That is useful for shortcut lookup and wrong for `TranslateMessage`, so the
                // supplement is what is carried; see `crate::pump::key_text_messages`.
                text: event.text_with_all_modifiers().map(str::to_owned),
            }
        }
        WindowEvent::CursorMoved { position, .. } => HostEvent::CursorMoved {
            x: position.x,
            y: position.y,
        },
        WindowEvent::CursorLeft { .. } => HostEvent::CursorLeft,
        WindowEvent::MouseInput { state, button, .. } => HostEvent::MouseInput {
            button: mouse_button(*button),
            pressed: *state == ElementState::Pressed,
        },
        WindowEvent::MouseWheel { delta, .. } => {
            // WHEEL_DELTA is 120 and rides in the high word of WPARAM.
            let notches = match delta {
                MouseScrollDelta::LineDelta(_, y) => *y,
                MouseScrollDelta::PixelDelta(p) => {
                    #[allow(clippy::cast_possible_truncation)]
                    // LINT-OK: a scroll distance, not an engine computation; the client only
                    // ever reads the sign and the 120-unit magnitude.
                    {
                        (p.y / 120.0) as f32
                    }
                }
            };
            HostEvent::MouseWheel { notches }
        }
        _ => return None,
    })
}

/// The host's physical key as the `MSG` names it: `wParam`'s virtual key and `lParam`'s scan code.
///
/// `None` for a key the platform has no scan code for or that `winuser.h` has no `VK_*` for: that
/// cannot produce a control specification, so it is not a message the input pipeline could read.
#[must_use]
pub fn key_from_key_code(code: winit::keyboard::KeyCode) -> Option<Key> {
    Some(Key::new(
        crate::pump::vk_from_key_code(code)?,
        crate::pump::scan_code_from_key_code(code)?,
    ))
}

/// Translate the window library's physical identity to the shared key vocabulary.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn input_key_code(code: winit::keyboard::KeyCode) -> dereth_input::keys::KeyCode {
    use dereth_input::keys::KeyCode as K;
    use winit::keyboard::KeyCode as W;
    match code {
        W::AltLeft => K::AltLeft,
        W::AltRight => K::AltRight,
        W::ArrowDown => K::ArrowDown,
        W::ArrowLeft => K::ArrowLeft,
        W::ArrowRight => K::ArrowRight,
        W::ArrowUp => K::ArrowUp,
        W::AudioVolumeDown => K::AudioVolumeDown,
        W::AudioVolumeMute => K::AudioVolumeMute,
        W::AudioVolumeUp => K::AudioVolumeUp,
        W::Backquote => K::Backquote,
        W::Backslash => K::Backslash,
        W::Backspace => K::Backspace,
        W::BracketLeft => K::BracketLeft,
        W::BracketRight => K::BracketRight,
        W::BrowserBack => K::BrowserBack,
        W::BrowserFavorites => K::BrowserFavorites,
        W::BrowserForward => K::BrowserForward,
        W::BrowserHome => K::BrowserHome,
        W::BrowserRefresh => K::BrowserRefresh,
        W::BrowserSearch => K::BrowserSearch,
        W::BrowserStop => K::BrowserStop,
        W::CapsLock => K::CapsLock,
        W::Comma => K::Comma,
        W::ContextMenu => K::ContextMenu,
        W::ControlLeft => K::ControlLeft,
        W::ControlRight => K::ControlRight,
        W::Convert => K::Convert,
        W::Delete => K::Delete,
        W::Digit0 => K::Digit0,
        W::Digit1 => K::Digit1,
        W::Digit2 => K::Digit2,
        W::Digit3 => K::Digit3,
        W::Digit4 => K::Digit4,
        W::Digit5 => K::Digit5,
        W::Digit6 => K::Digit6,
        W::Digit7 => K::Digit7,
        W::Digit8 => K::Digit8,
        W::Digit9 => K::Digit9,
        W::End => K::End,
        W::Enter => K::Enter,
        W::Equal => K::Equal,
        W::Escape => K::Escape,
        W::F1 => K::F1,
        W::F10 => K::F10,
        W::F11 => K::F11,
        W::F12 => K::F12,
        W::F13 => K::F13,
        W::F14 => K::F14,
        W::F15 => K::F15,
        W::F16 => K::F16,
        W::F17 => K::F17,
        W::F18 => K::F18,
        W::F19 => K::F19,
        W::F2 => K::F2,
        W::F20 => K::F20,
        W::F21 => K::F21,
        W::F22 => K::F22,
        W::F23 => K::F23,
        W::F24 => K::F24,
        W::F3 => K::F3,
        W::F4 => K::F4,
        W::F5 => K::F5,
        W::F6 => K::F6,
        W::F7 => K::F7,
        W::F8 => K::F8,
        W::F9 => K::F9,
        W::Home => K::Home,
        W::Insert => K::Insert,
        W::IntlBackslash => K::IntlBackslash,
        W::IntlRo => K::IntlRo,
        W::IntlYen => K::IntlYen,
        W::KanaMode => K::KanaMode,
        W::KeyA => K::KeyA,
        W::KeyB => K::KeyB,
        W::KeyC => K::KeyC,
        W::KeyD => K::KeyD,
        W::KeyE => K::KeyE,
        W::KeyF => K::KeyF,
        W::KeyG => K::KeyG,
        W::KeyH => K::KeyH,
        W::KeyI => K::KeyI,
        W::KeyJ => K::KeyJ,
        W::KeyK => K::KeyK,
        W::KeyL => K::KeyL,
        W::KeyM => K::KeyM,
        W::KeyN => K::KeyN,
        W::KeyO => K::KeyO,
        W::KeyP => K::KeyP,
        W::KeyQ => K::KeyQ,
        W::KeyR => K::KeyR,
        W::KeyS => K::KeyS,
        W::KeyT => K::KeyT,
        W::KeyU => K::KeyU,
        W::KeyV => K::KeyV,
        W::KeyW => K::KeyW,
        W::KeyX => K::KeyX,
        W::KeyY => K::KeyY,
        W::KeyZ => K::KeyZ,
        W::Lang1 => K::Lang1,
        W::Lang2 => K::Lang2,
        W::LaunchApp1 => K::LaunchApp1,
        W::LaunchApp2 => K::LaunchApp2,
        W::LaunchMail => K::LaunchMail,
        W::MediaPlayPause => K::MediaPlayPause,
        W::MediaSelect => K::MediaSelect,
        W::MediaStop => K::MediaStop,
        W::MediaTrackNext => K::MediaTrackNext,
        W::MediaTrackPrevious => K::MediaTrackPrevious,
        W::Minus => K::Minus,
        W::NonConvert => K::NonConvert,
        W::NumLock => K::NumLock,
        W::Numpad0 => K::Numpad0,
        W::Numpad1 => K::Numpad1,
        W::Numpad2 => K::Numpad2,
        W::Numpad3 => K::Numpad3,
        W::Numpad4 => K::Numpad4,
        W::Numpad5 => K::Numpad5,
        W::Numpad6 => K::Numpad6,
        W::Numpad7 => K::Numpad7,
        W::Numpad8 => K::Numpad8,
        W::Numpad9 => K::Numpad9,
        W::NumpadAdd => K::NumpadAdd,
        W::NumpadComma => K::NumpadComma,
        W::NumpadDecimal => K::NumpadDecimal,
        W::NumpadDivide => K::NumpadDivide,
        W::NumpadEnter => K::NumpadEnter,
        W::NumpadEqual => K::NumpadEqual,
        W::NumpadMultiply => K::NumpadMultiply,
        W::NumpadSubtract => K::NumpadSubtract,
        W::PageDown => K::PageDown,
        W::PageUp => K::PageUp,
        W::Pause => K::Pause,
        W::Period => K::Period,
        W::Power => K::Power,
        W::PrintScreen => K::PrintScreen,
        W::Quote => K::Quote,
        W::ScrollLock => K::ScrollLock,
        W::Semicolon => K::Semicolon,
        W::ShiftLeft => K::ShiftLeft,
        W::ShiftRight => K::ShiftRight,
        W::Slash => K::Slash,
        W::Space => K::Space,
        W::SuperLeft => K::SuperLeft,
        W::SuperRight => K::SuperRight,
        W::Tab => K::Tab,
        _ => K::Unidentified,
    }
}

/// The host's mouse button as the `0x201`–`0x20D` block names it.
#[must_use]
pub fn mouse_button(button: winit::event::MouseButton) -> MouseButton {
    match button {
        winit::event::MouseButton::Left => MouseButton::Left,
        winit::event::MouseButton::Right => MouseButton::Right,
        winit::event::MouseButton::Middle => MouseButton::Middle,
        winit::event::MouseButton::Back => MouseButton::Back,
        winit::event::MouseButton::Forward => MouseButton::Forward,
        winit::event::MouseButton::Other(n) => MouseButton::Other(n),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    const MONITOR: (u32, u32) = (2560, 1440);

    /// A fixed-size window under a Wayland compositor that behaves as KWin does, and the backend
    /// in front of it as winit's does.
    ///
    /// * The window's size and size limits reach the compositor with the next commit (a frame).
    /// * The compositor keeps its own record of the window's size. A commit replaces the record
    ///   with the committed size, unless a configure is scheduled, in which case the commit's size
    ///   is disregarded. A record outside the limits schedules a configure back within them.
    /// * A request to leave full screen schedules a configure whatever the window's state; one
    ///   that does leave full screen carries the windowed size the compositor saved on the way in.
    /// * The backend applies a configure's size as it arrives, and that is a resize. A size
    ///   request while the window is configured full screen is not applied.
    struct Compositor {
        /// The limits the next commit carries, when they changed.
        pending_limits: Cell<Option<Option<(u32, u32)>>>,
        limits: Cell<Option<(u32, u32)>>,
        /// The compositor's record of the window's size.
        record: Cell<(u32, u32)>,
        /// The backend's size for the window, which the next commit carries.
        client: Cell<(u32, u32)>,
        /// Whether the last configure was full screen.
        client_full_screen: Cell<bool>,
        full_screen: Cell<bool>,
        restore: Cell<(u32, u32)>,
        /// The size of the scheduled configure.
        scheduled: Cell<Option<(u32, u32)>>,
        /// A size the compositor configures the window to after every commit of another size.
        insists_on: Cell<Option<(u32, u32)>>,
        leave_requests: Cell<u32>,
        size_requests: Cell<u32>,
    }

    impl Compositor {
        /// A window created not resizable: both limits hold its first size.
        fn fixed(size: (u32, u32)) -> Self {
            Self {
                pending_limits: Cell::new(None),
                limits: Cell::new(Some(size)),
                record: Cell::new(size),
                client: Cell::new(size),
                client_full_screen: Cell::new(false),
                full_screen: Cell::new(false),
                restore: Cell::new(size),
                scheduled: Cell::new(None),
                insists_on: Cell::new(None),
                leave_requests: Cell::new(0),
                size_requests: Cell::new(0),
            }
        }

        fn commit(&self) {
            if let Some(limits) = self.pending_limits.take() {
                self.limits.set(limits);
            }
            if self.scheduled.get().is_some() {
                return;
            }
            self.record.set(self.client.get());
            let wanted = self.insists_on.get().or(self.limits.get());
            if let Some(wanted) = wanted.filter(|&w| w != self.record.get()) {
                self.scheduled.set(Some(wanted));
            }
        }

        /// Deliver the scheduled configure; answers the resize it causes.
        fn configure(&self) -> Option<(u32, u32)> {
            let size = self.scheduled.take()?;
            self.client_full_screen.set(self.full_screen.get());
            self.record.set(size);
            (size != self.client.replace(size)).then_some(size)
        }

        /// Run frames until the compositor has nothing more to say, handing each resize to
        /// [`settle_resized`] as the event pump does; answers the resizes the client was given.
        fn settle(&self, held: &HeldSize, pin_limits: bool) -> Vec<(u32, u32)> {
            let mut resizes = Vec::new();
            for _ in 0..8 {
                self.commit();
                let Some(size) = self.configure() else {
                    break;
                };
                resizes.push(settle_resized(self, held, size, pin_limits));
            }
            resizes
        }

        /// The windowed size the compositor and the backend agree on.
        fn agreed(&self) -> (u32, u32) {
            assert_eq!(
                self.record.get(),
                self.client.get(),
                "compositor and backend"
            );
            self.record.get()
        }
    }

    impl FixedSizeWindow for Compositor {
        fn set_size_limits(&self, size: Option<(u32, u32)>) {
            self.pending_limits.set(Some(size));
        }

        fn request_client_size(&self, size: (u32, u32)) -> (u32, u32) {
            self.size_requests.set(self.size_requests.get() + 1);
            if !self.client_full_screen.get() {
                self.client.set(size);
            }
            self.client.get()
        }

        fn is_full_screen(&self) -> bool {
            self.client_full_screen.get()
        }

        fn set_full_screen(&self, full_screen: bool) {
            if full_screen {
                if !self.full_screen.replace(true) {
                    self.restore.set(self.record.get());
                }
                self.scheduled.set(Some(MONITOR));
                return;
            }
            self.leave_requests.set(self.leave_requests.get() + 1);
            let size = if self.full_screen.replace(false) {
                self.restore.get()
            } else {
                self.record.get()
            };
            self.scheduled.set(Some(size));
        }
    }

    /// A windowed presentation change, in the order the runtime makes it: leave full screen, then
    /// ask for the size.
    fn present_windowed(
        window: &Compositor,
        held: &HeldSize,
        size: (u32, u32),
        pin_limits: bool,
    ) -> (u32, u32) {
        set_fixed_window_full_screen(window, held, false, pin_limits);
        resize_fixed_window(window, held, size, pin_limits)
    }

    /// In windowed mode, choosing a resolution resizes the window to it and it stays that size.
    #[test]
    fn a_resolution_change_resizes_the_fixed_window_and_the_compositor_keeps_it() {
        let window = Compositor::fixed((800, 600));
        let held = HeldSize::default();
        for size in [(1920, 1080), (800, 600), (1920, 1080), (1024, 768)] {
            assert_eq!(present_windowed(&window, &held, size, true), size);
            assert_eq!(window.settle(&held, true), [], "no resize back");
            assert_eq!(window.agreed(), size);
        }
        assert_eq!(window.leave_requests.get(), 0, "never full screen");
        assert_eq!(window.size_requests.get(), 4, "each size asked for once");
    }

    /// Full screen fills the monitor, and leaving it at a new resolution ends at that resolution:
    /// the compositor restores the old windowed size first, and the size is asked for once more.
    #[test]
    fn leaving_full_screen_at_a_new_resolution_ends_at_that_resolution() {
        let window = Compositor::fixed((800, 600));
        let held = HeldSize::default();
        set_fixed_window_full_screen(&window, &held, true, true);
        assert_eq!(window.settle(&held, true), [MONITOR]);
        assert_eq!(window.agreed(), MONITOR, "not held to the windowed size");

        // Still full screen when asked: the backend answers the monitor's size.
        assert_eq!(present_windowed(&window, &held, (1024, 768), true), MONITOR);
        assert_eq!(window.leave_requests.get(), 1);
        assert_eq!(
            window.settle(&held, true),
            [(1024, 768)],
            "800x600 asked past"
        );
        assert_eq!(window.agreed(), (1024, 768));
        assert_eq!(window.size_requests.get(), 2);
    }

    /// Full screen asked for and left before the compositor answered is still left.
    #[test]
    fn full_screen_asked_for_and_not_yet_answered_is_still_left() {
        let window = Compositor::fixed((800, 600));
        let held = HeldSize::default();
        set_fixed_window_full_screen(&window, &held, true, true);
        present_windowed(&window, &held, (800, 600), true);
        assert_eq!(window.leave_requests.get(), 1);
        window.settle(&held, true);
        assert_eq!(window.agreed(), (800, 600));
    }

    /// A compositor that keeps answering with its own size is believed after the one second ask.
    #[test]
    fn a_compositor_that_insists_on_its_own_size_is_asked_again_only_once() {
        let window = Compositor::fixed((800, 600));
        let held = HeldSize::default();
        window.insists_on.set(Some((1600, 900)));
        present_windowed(&window, &held, (1920, 1080), true);
        assert_eq!(
            window.settle(&held, true),
            [(1920, 1080), (1600, 900)],
            "asked again, then kept"
        );
        assert_eq!(window.agreed(), (1600, 900));
        assert_eq!(window.size_requests.get(), 2);
    }

    /// Elsewhere this module holds no limits, asks to leave full screen on every windowed change
    /// as it always did, and passes a differing resize through without asking again.
    #[test]
    fn a_backend_that_moves_the_limits_itself_is_only_asked_for_the_size() {
        let window = Compositor::fixed((800, 600));
        window.limits.set(None);
        let held = HeldSize::default();
        assert_eq!(
            present_windowed(&window, &held, (1024, 768), false),
            (1024, 768)
        );
        assert_eq!(window.pending_limits.get(), None, "limits untouched");
        assert_eq!(window.leave_requests.get(), 1, "leave asked for");
        assert_eq!(window.settle(&held, false), [(800, 600)], "not asked again");
        assert_eq!(window.size_requests.get(), 1);
        set_fixed_window_full_screen(&window, &held, true, false);
        assert_eq!(window.pending_limits.get(), None, "limits untouched");
    }

    /// A cursor's hotspot is carried into its picture, and one past the 32x32 image is held to
    /// its last row or column.
    #[test]
    fn a_cursor_hotspot_is_carried_and_held_inside_the_picture() {
        assert_eq!(picture_hotspot(0), 0);
        assert_eq!(picture_hotspot(14), 14);
        assert_eq!(picture_hotspot(31), 31);
        assert_eq!(picture_hotspot(32), 31);
        assert_eq!(picture_hotspot(u32::MAX), 31);
    }

    /// The named keys are what the host table answers.
    #[test]
    fn the_named_keys_are_what_the_host_table_answers() {
        use winit::keyboard::KeyCode as K;
        for (code, named, name) in [
            (K::Escape, Key::ESCAPE, "Escape"),
            (K::Space, Key::SPACE, "Space"),
            (K::KeyC, Key::KEY_C, "KeyC"),
            (K::KeyW, Key::KEY_W, "KeyW"),
            (K::ControlLeft, Key::CONTROL_LEFT, "ControlLeft"),
            (K::ArrowUp, Key::ARROW_UP, "ArrowUp"),
        ] {
            assert_eq!(key_from_key_code(code), Some(named), "{name}");
        }
    }

    /// Monitor metrics carries the selected monitors real work area.
    #[cfg(windows)]
    #[test]
    fn monitor_metrics_carries_the_selected_monitors_real_work_area() {
        use winit::platform::pump_events::EventLoopExtPumpEvents as _;
        use winit::platform::windows::{
            EventLoopBuilderExtWindows as _, MonitorHandleExtWindows as _,
        };

        /// Reads the primary monitor off the running loop, and makes no window.
        struct Primary(Option<winit::monitor::MonitorHandle>);
        impl winit::application::ApplicationHandler for Primary {
            fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
                self.0 = event_loop.primary_monitor();
            }
            fn window_event(
                &mut self,
                _: &winit::event_loop::ActiveEventLoop,
                _: winit::window::WindowId,
                _: winit::event::WindowEvent,
            ) {
            }
        }

        let mut builder = winit::event_loop::EventLoop::builder();
        builder.with_any_thread(true);
        let mut event_loop = builder.build().expect("read-only event loop");
        let mut primary = Primary(None);
        event_loop.pump_app_events(Some(std::time::Duration::ZERO), &mut primary);
        let monitor = primary.0.expect("primary monitor");
        let expected = dereth_render::window_proc::monitor_work_area(monitor.hmonitor())
            .expect("the primary monitor has a work area");
        let actual = monitor_metrics(Some(&monitor), (3, 23, 3));

        assert_eq!(actual.work_area, Some(expected));
        assert!(expected.left < expected.right && expected.top < expected.bottom);
        assert!(expected.left >= actual.origin.0 && expected.top >= actual.origin.1);
        assert!(expected.right <= actual.origin.0 + actual.cx_screen);
        assert!(expected.bottom <= actual.origin.1 + actual.cy_screen);
        assert_eq!(dereth_render::window_proc::monitor_work_area(0), None);
        assert_eq!(dereth_render::window_proc::monitor_work_area(-1), None);
        assert_eq!(monitor_metrics(None, (3, 23, 3)).work_area, None);
    }
}
