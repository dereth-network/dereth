//! The window message table and the frame sleep, as a pure state machine. It lives here rather
//! than in the renderer so the application loop can own the window state without naming the
//! renderer. `dereth_render::window_proc` re-exports every item and keeps the two host queries (the
//! caret blink interval and a monitor's work area), which call the operating system.
//!
//! The state machine preserves the message pump, window class and window style behavior.
//!
//! In the client the window state is a block of statics rather than an object, so `DeviceState`
//! is that block and `wnd_proc` is the switch over it. Keeping the table a pure function of
//! `(state, message)` is what makes
//! it testable without a window: the message table is exercised by a scripted sequence, and a
//! scripted sequence needs the switch to be callable.

/// The Win32 message ids the client handles. Values are Windows'.
pub mod msg {
    pub const WM_DESTROY: u32 = 0x0002;
    pub const WM_SIZE: u32 = 0x0005;
    pub const WM_ACTIVATE: u32 = 0x0006;
    pub const WM_SETFOCUS: u32 = 0x0007;
    pub const WM_KILLFOCUS: u32 = 0x0008;
    pub const WM_CLOSE: u32 = 0x0010;
    pub const WM_ERASEBKGND: u32 = 0x0014;
    pub const WM_ACTIVATEAPP: u32 = 0x001C;
    pub const WM_CANCELMODE: u32 = 0x001F;
    pub const WM_INPUTLANGCHANGE: u32 = 0x0051;
    pub const WM_KEYDOWN: u32 = 0x0100;
    pub const WM_KEYUP: u32 = 0x0101;
    pub const WM_CHAR: u32 = 0x0102;
    pub const WM_SYSKEYDOWN: u32 = 0x0104;
    pub const WM_SYSKEYUP: u32 = 0x0105;
    pub const WM_SYSCHAR: u32 = 0x0106;
    pub const WM_IME_STARTCOMPOSITION: u32 = 0x010D;
    pub const WM_IME_ENDCOMPOSITION: u32 = 0x010E;
    pub const WM_IME_COMPOSITION: u32 = 0x010F;
    pub const WM_SYSCOMMAND: u32 = 0x0112;
    pub const WM_IME_SETCONTEXT: u32 = 0x0281;
    pub const WM_IME_NOTIFY: u32 = 0x0282;
    pub const WM_IME_CONTROL: u32 = 0x0283;
    pub const WM_IME_COMPOSITIONFULL: u32 = 0x0284;
    pub const WM_MOUSEMOVE: u32 = 0x0200;
    pub const WM_LBUTTONDOWN: u32 = 0x0201;
    pub const WM_LBUTTONUP: u32 = 0x0202;
    pub const WM_LBUTTONDBLCLK: u32 = 0x0203;
    pub const WM_RBUTTONDOWN: u32 = 0x0204;
    pub const WM_RBUTTONUP: u32 = 0x0205;
    pub const WM_RBUTTONDBLCLK: u32 = 0x0206;
    pub const WM_MBUTTONDOWN: u32 = 0x0207;
    pub const WM_MBUTTONUP: u32 = 0x0208;
    pub const WM_MBUTTONDBLCLK: u32 = 0x0209;
    pub const WM_MOUSEWHEEL: u32 = 0x020A;
    pub const WM_XBUTTONDOWN: u32 = 0x020B;
    pub const WM_XBUTTONUP: u32 = 0x020C;
    pub const WM_XBUTTONDBLCLK: u32 = 0x020D;
    pub const WM_POWERBROADCAST: u32 = 0x0218;
    pub const WM_DEVICECHANGE: u32 = 0x0219;
    pub const WM_ENTERSIZEMOVE: u32 = 0x0231;
    pub const WM_EXITSIZEMOVE: u32 = 0x0232;
    pub const WM_MOUSELEAVE: u32 = 0x02A3;

    /// `SIZE_MINIMIZED`.
    pub const SIZE_MINIMIZED: usize = 1;
    /// `WA_INACTIVE`.
    pub const WA_INACTIVE: u32 = 0;
    pub const SC_CLOSE: usize = 0xF060;
    pub const SC_SCREENSAVE: usize = 0xF140;
    pub const SC_MONITORPOWER: usize = 0xF170;
    pub const VK_RETURN: usize = 0x0D;
    pub const VK_F4: usize = 0x73;
    /// `PBT_APMQUERYSUSPEND`.
    pub const PBT_APMQUERYSUSPEND: usize = 0;
    /// `BROADCAST_QUERY_DENY` — `'BMQD'`, i.e. deny suspend.
    pub const BROADCAST_QUERY_DENY: isize = 0x424D_5144;
}

/// The window styles: `0x02CA0000` when windowed and `0x82000000` full-screen, then `| 0x10000000`
/// when visible. **There is no `WS_THICKFRAME` and no `WS_MAXIMIZEBOX`**: the user can never resize
/// or maximise the window, which is why `wnd_proc` has no size handling.
pub mod style {
    /// `WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_CLIPCHILDREN | WS_VISIBLE`.
    pub const WINDOWED: u32 = 0x12CA_0000;
    /// `WS_POPUP | WS_CLIPCHILDREN | WS_VISIBLE`.
    pub const FULLSCREEN: u32 = 0x9200_0000;
    /// `dwExStyle` is 0 in both modes.
    pub const EX_STYLE: u32 = 0;
    /// The registered class name.
    pub const CLASS_NAME: &str = "Turbine Device Class";

    /// The window style computed during initialization and presentation changes.
    #[must_use]
    pub const fn for_mode(windowed: bool) -> u32 {
        if windowed {
            WINDOWED
        } else {
            FULLSCREEN
        }
    }
}

/// The block of device statics that the message table reads and writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceState {
    /// Whether device initialization has completed.
    pub is_initialized: bool,
    /// Whether the graphics engine is ready; gates `WM_ERASEBKGND`.
    pub is_ready: bool,
    /// Whether quitting was requested; returned by the event loop.
    pub is_done: bool,
    /// Whether the app is in the foreground; gates input and the background frame cap.
    pub is_active_app: bool,
    /// Whether the window is minimized; **initially true**.
    pub is_minimized: bool,
    /// Whether an Alt+Enter fullscreen toggle is pending.
    pub toggle_full_screen_mode: bool,
    /// Whether mouse-leave tracking is armed.
    pub track_leave_called: bool,
    /// Whether system keys are enabled; initially true, then **cleared during initialization** because the window has no
    /// menu, so Alt-key system commands are disabled for the life of the client.
    pub sys_keys_enabled: bool,
    /// Whether fullscreen mode is allowed.
    pub allow_full_screen_mode: bool,
    /// The display preferences' full-screen flag.
    pub full_screen: bool,
}

impl Default for DeviceState {
    /// The initial device-global values after initialization step 2 has cleared
    /// `sys_keys_enabled`.
    fn default() -> Self {
        Self {
            is_initialized: true,
            is_ready: false,
            is_done: false,
            is_active_app: false,
            is_minimized: true,
            toggle_full_screen_mode: false,
            track_leave_called: false,
            // Initialization step 2: `GetMenu(hwnd)`; the (still null) window has no menu, so
            // system keys are disabled. This is effectively always taken.
            sys_keys_enabled: false,
            allow_full_screen_mode: true,
            full_screen: false,
        }
    }
}

/// Side effects the switch asks the host for. The window procedure calls into the input manager,
/// the browser forwarder and the cursor; recording the requests keeps the table pure and lets a
/// test assert them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// Forward the message to the input manager: build a `MSG` from the four parameters **plus
    /// `GetMessageTime()`** and hand it to the input manager's message handler.
    ///
    /// The input pipeline's tap and double-click thresholds are driven by the Windows
    /// message time. A rebuild must pass the OS value through, not a high-resolution clock.
    ForwardToInputManager,
    /// The copy-protection layer's own message dispatch gets first refusal.
    ForwardToBrowser,
    /// The window became the active application.
    Activate,
    /// The window stopped being the active application.
    Deactivate,
    /// `SetFocus(hwnd)`, issued **before** the mouse path on a button-down.
    SetFocus,
    /// Arm `_TrackMouseEvent(TME_LEAVE)`.
    TrackMouseEvent,
}

/// What the window procedure decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WndProcResult {
    /// When false the caller runs `DefWindowProcA`.
    pub handled: bool,
    /// The value returned to Windows.
    pub result: isize,
    /// In call order.
    pub effects: Vec<Effect>,
}

impl WndProcResult {
    fn unhandled() -> Self {
        Self {
            handled: false,
            result: 0,
            effects: Vec::new(),
        }
    }
    fn handled_with(effects: Vec<Effect>) -> Self {
        Self {
            handled: true,
            result: 0,
            effects,
        }
    }
}

/// Every message the client handles.
///
/// The prologue's pre-hook is a linker-folded stub and does nothing observable, so it is not
/// modelled.
#[must_use]
pub fn wnd_proc(s: &mut DeviceState, message: u32, wparam: usize, _lparam: isize) -> WndProcResult {
    use msg::*;
    match message {
        WM_DESTROY | WM_CLOSE => {
            s.is_done = true;
            WndProcResult::handled_with(vec![])
        }

        // No geometry handling at all: the window is not resizable.
        WM_SIZE => {
            if wparam == SIZE_MINIMIZED {
                s.is_minimized = true;
                let e = deactivate(s);
                return WndProcResult::handled_with(e);
            }
            WndProcResult::handled_with(vec![])
        }

        WM_ACTIVATE => {
            // HIWORD is the minimized flag, LOWORD the activation state. Both are 16-bit
            // fields of the WPARAM, so the masks below make the narrowing exact.
            #[allow(clippy::cast_possible_truncation)]
            // LINT-OK: integer field extraction from a WPARAM, not a float conversion.
            let hi = ((wparam >> 16) & 0xFFFF) as u32;
            #[allow(clippy::cast_possible_truncation)]
            // LINT-OK: as above.
            let lo = (wparam & 0xFFFF) as u32;
            s.is_minimized = hi != 0;
            let mut e = Vec::new();
            if lo == WA_INACTIVE {
                e.extend(deactivate(s));
            } else if lo < 3 && !s.is_minimized {
                e.extend(activate(s));
            }
            WndProcResult::handled_with(e)
        }

        WM_SETFOCUS | WM_KILLFOCUS | WM_CANCELMODE | WM_DEVICECHANGE => {
            WndProcResult::handled_with(vec![Effect::ForwardToInputManager])
        }

        // Suppress the background erase -- this is what stops the black BLACK_BRUSH flash.
        WM_ERASEBKGND => {
            if s.is_ready && s.is_active_app {
                WndProcResult {
                    handled: true,
                    result: 1,
                    effects: Vec::new(),
                }
            } else {
                WndProcResult::unhandled()
            }
        }

        WM_ACTIVATEAPP => {
            let mut e = Vec::new();
            if wparam != 0 {
                if !s.is_minimized {
                    e.extend(activate(s));
                }
            } else {
                e.extend(deactivate(s));
            }
            WndProcResult::handled_with(e)
        }

        WM_KEYDOWN | WM_KEYUP | WM_CHAR => WndProcResult::handled_with(vec![
            Effect::ForwardToBrowser,
            Effect::ForwardToInputManager,
        ]),

        WM_SYSKEYDOWN => {
            let e = vec![Effect::ForwardToInputManager];
            // Alt+F4 and Alt+Enter are the two system keys the default procedure always sees,
            // whatever the system-keys flag says. For Alt+F4 that is the close: the default
            // procedure answers with `SC_CLOSE`, which ends the client below. Alt+Enter also
            // latches the full-screen toggle, applied when the event loop finishes.
            if wparam == VK_F4 || wparam == VK_RETURN {
                if wparam == VK_RETURN {
                    s.toggle_full_screen_mode = true;
                }
                return WndProcResult {
                    handled: false,
                    result: 0,
                    effects: e,
                };
            }
            WndProcResult {
                handled: !s.sys_keys_enabled,
                result: 0,
                effects: e,
            }
        }

        WM_SYSKEYUP => WndProcResult {
            handled: !s.sys_keys_enabled,
            result: 0,
            effects: vec![Effect::ForwardToInputManager],
        },

        WM_SYSCHAR => WndProcResult::handled_with(vec![Effect::ForwardToInputManager]),

        WM_SYSCOMMAND => match wparam & 0xFFF0 {
            SC_CLOSE => {
                s.is_done = true;
                WndProcResult::handled_with(vec![])
            }
            // Screensaver and monitor power-down are blocked for the whole session.
            SC_SCREENSAVE | SC_MONITORPOWER => WndProcResult::handled_with(vec![]),
            _ => WndProcResult::unhandled(),
        },

        // ForwardToBrowser gets these first; if the browser does not consume them, DefWindowProc drives the
        // normal IME. The pure model reports the ForwardToBrowser call and leaves the message unhandled,
        // because whether ForwardToBrowser consumed it is not a property of this table.
        WM_INPUTLANGCHANGE
        | WM_IME_STARTCOMPOSITION
        | WM_IME_ENDCOMPOSITION
        | WM_IME_COMPOSITION
        | WM_IME_SETCONTEXT
        | WM_IME_NOTIFY
        | WM_IME_CONTROL
        | WM_IME_COMPOSITIONFULL => WndProcResult {
            handled: false,
            result: 0,
            effects: vec![Effect::ForwardToBrowser],
        },

        WM_MOUSEMOVE | WM_LBUTTONUP | WM_LBUTTONDBLCLK | WM_RBUTTONUP | WM_RBUTTONDBLCLK
        | WM_MBUTTONUP | WM_MBUTTONDBLCLK | WM_MOUSEWHEEL | WM_XBUTTONUP | WM_XBUTTONDBLCLK => {
            WndProcResult::handled_with(mouse_path(s, false))
        }

        // SetFocus(hwnd) *first*, then the WM_MOUSEMOVE path.
        WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_XBUTTONDOWN => {
            WndProcResult::handled_with(mouse_path(s, true))
        }

        WM_POWERBROADCAST => {
            if wparam == PBT_APMQUERYSUSPEND {
                WndProcResult {
                    handled: true,
                    result: BROADCAST_QUERY_DENY,
                    effects: Vec::new(),
                }
            } else {
                WndProcResult::unhandled()
            }
        }

        WM_ENTERSIZEMOVE => {
            if s.full_screen {
                WndProcResult::handled_with(vec![])
            } else {
                WndProcResult::handled_with(deactivate(s))
            }
        }

        WM_EXITSIZEMOVE => {
            let mut e = Vec::new();
            if !s.full_screen {
                e.extend(activate(s));
            }
            WndProcResult::handled_with(e)
        }

        WM_MOUSELEAVE => {
            s.track_leave_called = false;
            WndProcResult::handled_with(vec![Effect::ForwardToInputManager])
        }

        _ => WndProcResult::unhandled(),
    }
}

/// `_TrackMouseEvent({TME_LEAVE})` is armed on the first mouse message after each `WM_MOUSELEAVE`.
fn mouse_path(s: &mut DeviceState, set_focus: bool) -> Vec<Effect> {
    let mut e = Vec::new();
    if set_focus {
        e.push(Effect::SetFocus);
    }
    if !s.track_leave_called {
        s.track_leave_called = true;
        e.push(Effect::TrackMouseEvent);
    }
    e.push(Effect::ForwardToBrowser);
    e.push(Effect::ForwardToInputManager);
    e
}

/// Activation: if not already active, tell the input manager and mark the app active.
fn activate(s: &mut DeviceState) -> Vec<Effect> {
    if s.is_active_app {
        return Vec::new();
    }
    s.is_active_app = true;
    vec![Effect::Activate]
}

/// Deactivation: if active, mark the app inactive, tell the input manager and, **unless full
/// screen**, put the arrow cursor back.
fn deactivate(s: &mut DeviceState) -> Vec<Effect> {
    if !s.is_active_app {
        return Vec::new();
    }
    s.is_active_app = false;
    vec![Effect::Deactivate]
}

/// The event loop's epilogue — the Alt+Enter latch.
///
/// ```text
/// if (toggle_full_screen_mode) {
///     if a renderer exists and the app is active
///         full_screen = (allow_full_screen_mode && !full_screen)
///     toggle_full_screen_mode = false
/// }
/// ```
///
/// Returns `is_done`.
pub fn finish_event_loop(s: &mut DeviceState, renderer_exists: bool) -> bool {
    if s.toggle_full_screen_mode {
        if renderer_exists && s.is_active_app {
            s.full_screen = s.allow_full_screen_mode && !s.full_screen;
        }
        s.toggle_full_screen_mode = false;
    }
    s.is_done
}

/// The background frame cap: ≥ 99 ms per frame, i.e. roughly 10.1 fps.
pub const BACKGROUND_FRAME_MS: u32 = 99;

/// The frame sleep, as the client performs it:
///
/// ```text
/// ms = 0
/// if (!is_active_app) {
///     now = timeGetTime()
///     if (now - last_frame_tick < 99) ms = 99 - (now - last_frame_tick)
/// }
/// Sleep(ms)
/// last_frame_tick = timeGetTime()
/// ```
///
/// While the app is in the foreground this is `Sleep(0)` — a bare yield, no frame limiter; pacing
/// comes only from `PresentationInterval`.
#[must_use]
pub fn frame_sleep_ms(is_active_app: bool, now_ms: u32, last_frame_tick_ms: u32) -> u32 {
    if is_active_app {
        return 0;
    }
    // The original subtracts two DWORDs, so the arithmetic wraps rather than saturating.
    let elapsed = now_ms.wrapping_sub(last_frame_tick_ms);
    BACKGROUND_FRAME_MS.saturating_sub(elapsed)
}

/// The windowed outer size computed from the window metrics:
///
/// ```text
/// cx = Width  + 2*GetSystemMetrics(SM_CXDLGFRAME)
/// cy = Height + GetSystemMetrics(SM_CYCAPTION) + 2*GetSystemMetrics(SM_CYDLGFRAME)
/// ```
///
/// In full-screen mode the branch is skipped entirely and `cx, cy = Width, Height`.
#[must_use]
pub fn outer_size(
    width: u32,
    height: u32,
    windowed: bool,
    cx_dlg_frame: u32,
    cy_caption: u32,
    cy_dlg_frame: u32,
) -> (u32, u32) {
    if windowed {
        (
            width + 2 * cx_dlg_frame,
            height + cy_caption + 2 * cy_dlg_frame,
        )
    } else {
        (width, height)
    }
}

/// The monitor rectangle window creation reads, and the Win32 `RECT` it carries. Both are
/// `dereth_client_contract::window`'s, because
/// `dereth_client_runtime::platform::window::WindowHost` returns them; every function that builds
/// or reads one is in this module.
pub use crate::window::{Rect, ScreenMetrics};

/// Convert `USER32!GetCaretBlinkTime`'s unsigned millisecond result to the seconds interval the
/// text element's caret loop compares against.
///
/// Retail does not special-case either API sentinel: `0` remains a zero-second interval and
/// `INFINITE` (`u32::MAX`) remains about 4.295 million seconds after the unsigned conversion.
#[must_use]
pub fn caret_blink_time_seconds_from_millis(milliseconds: u32) -> f64 {
    // The client multiplies by a 32-bit float constant, not by a double literal. Widen that
    // exact float before multiplying so its precision survives: 0.001_f32 widened is not 0.001.
    f64::from(milliseconds) * f64::from(0.001_f32)
}

/// What `CreateWindowExA` / `SetWindowPos` are given: the style and the **outer** rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement {
    /// `dwStyle` — [`style::WINDOWED`] or [`style::FULLSCREEN`], `| WS_VISIBLE` when visible.
    pub style: u32,
    pub x: i32,
    pub y: i32,
    /// The outer width passed at window creation.
    pub cx: i32,
    /// The outer height passed at window creation.
    pub cy: i32,
    /// `SetWindowPos`'s `hWndInsertAfter`. Retail passes `HWND_TOPMOST (-1)` in full screen and
    /// `HWND_NOTOPMOST (-2)` windowed, both at window creation and on every mode change;
    /// **this build answers `false` in both modes** -- see [`placement`]'s "always-on-top"
    /// section for why. It is kept as a field rather than deleted because it is
    /// what `SetWindowPos` is *given*, and a station asserts the value.
    pub topmost: bool,
}

/// The window geometry the client computes when it creates the window, and the same arithmetic
/// it re-runs on every mode change.
///
/// Full screen is `x = y = 0`, `cx = Width`, `cy = Height`, and nothing below runs. Windowed, the
/// client adds the frame -- `cx = Width + 2 * SM_CXDLGFRAME`,
/// `cy = Height + SM_CYCAPTION + 2 * SM_CYDLGFRAME` -- centres that on the screen with signed
/// halves (`x = SM_CXSCREEN / 2 - cx / 2`, `y = SM_CYSCREEN / 2 - cy / 2`), and then clamps
/// against `SPI_GETWORKAREA`: a window wider than the work area is pushed one frame width to the
/// left of it and bleeds off that edge, otherwise it is pulled back inside, right edge first and
/// then left. The vertical rule is the same pair with the caption included. **If the work-area
/// query fails the clamp is skipped entirely** and the centred rectangle stands. The visible flag
/// adds `WS_VISIBLE` to the style.
///
/// Note `x = (work.right - work.left) - cx` and not `work.right - cx`: on the primary monitor,
/// whose work area starts at `left = 0`, the two are the same number, and this reproduces what
/// retail does rather than what it meant.
///
/// # Two deliberate divergences from retail
///
/// ## 1. Borderless, not exclusive (client divergence CD-002)
///
/// Full screen is borderless windowed, not exclusive full screen. Retail's full-screen arm is
/// `x = y = 0, cx = Width, cy = Height` with the **display mode switched** to `Width x Height`
/// when the rendering system resets the device, i.e. a D3D9 exclusive-mode window. A borderless
/// window cannot change the display mode, so `cx, cy` here are the **monitor's** extent rather
/// than the requested one and the window is placed at the monitor's origin. When
/// `Display.Resolution` is the desktop resolution — the normal case, and the only one retail's
/// own resolution list offers on a modern machine — the two readings are the same rectangle. The
/// style bits and everything windowed are retail's unchanged.
///
/// ## 2. Full screen is not always-on-top (client divergence CD-003)
///
/// Retail's full-screen `SetWindowPos` passes `HWND_TOPMOST` twice — once at window creation
/// and once on the final call of a presentation change. Divergence 1 is what makes divergence 2
/// free: `HWND_TOPMOST` exists in retail because a D3D9 **exclusive**-mode window must stay in
/// front of the desktop, and a borderless window has no such requirement.
/// [`Placement::topmost`] is therefore `false` in **both** arms, and a shell scenario test keeps
/// it that way.
#[must_use]
pub fn placement(
    full_screen: bool,
    visible: bool,
    width: i32,
    height: i32,
    m: &ScreenMetrics,
) -> Placement {
    let mut style = style::for_mode(!full_screen) & !WS_VISIBLE;
    if visible {
        style |= WS_VISIBLE;
    }
    if full_screen {
        // The divergence, above: the monitor's own rectangle, and no mode switch.
        return Placement {
            style,
            x: m.origin.0,
            y: m.origin.1,
            cx: m.cx_screen,
            cy: m.cy_screen,
            // **DELIBERATE DIVERGENCE FROM RETAIL.**
            // Retail passes `HWND_TOPMOST` here -- window creation and the final
            // `SetWindowPos` of a presentation change both do, because a
            // D3D9 exclusive-mode window has to float over everything, which is the thing most
            // likely to annoy a player. This build's full screen is
            // **borderless windowed** (see above), which has no exclusive-mode requirement, so
            // the window sits at its normal Z order and Alt-Tab, second monitors and overlays
            // behave. **Do not change this back to `true`** -- a shell scenario test stops it
            // coming back.
            topmost: false,
        };
    }
    let cx = width + 2 * m.cx_dlg_frame;
    let cy = height + m.cy_caption + 2 * m.cy_dlg_frame;
    // The two signed halvings are of already-positive numbers, so a plain `/ 2` is the same
    // value; `origin` is added because a second monitor's centre is not the primary's.
    let mut x = m.origin.0 + m.cx_screen / 2 - cx / 2;
    let mut y = m.origin.1 + m.cy_screen / 2 - cy / 2;
    if let Some(w) = m.work_area {
        if cx > w.right - w.left {
            x = w.left - m.cx_dlg_frame;
        } else {
            if x + cx > w.right {
                x = (w.right - w.left) - cx;
            }
            if x < w.left {
                x = w.left;
            }
        }
        if cy > w.bottom - w.top {
            y = w.top;
        } else {
            if y + cy > w.bottom {
                y = (w.bottom - w.top) - cy;
            }
            if y < w.top {
                y = w.top;
            }
        }
    }
    Placement {
        style,
        x,
        y,
        cx,
        cy,
        topmost: false,
    }
}

/// A presentation change's placement -- the rectangle its **final**
/// `SetWindowPos` is given, which is **not** [`placement`]'s.
///
/// The presentation is re-read off the render device *after* the rendering system has been
/// restarted, so the `Width`, `Height` and `FullScreen` used below are what the device actually
/// ended up with rather than what was asked for.
///
/// Full screen is `x = y = 0`, `cx = Width`, `cy = Height`. Windowed, the client adds the frame
/// exactly as at creation (`cx = Width + 2 * SM_CXDLGFRAME`,
/// `cy = Height + SM_CYCAPTION + 2 * SM_CYDLGFRAME`) and then picks the centre from the **old**
/// presentation's full-screen flag, read before the display preferences were reloaded: coming
/// *from* full screen it centres on the screen, coming from a window it keeps **the window's own
/// current centre** from `GetWindowRect`, halving with a round-toward-zero division. A failed
/// `GetWindowRect` leaves the seeds at zero and the clamp still runs. The work-area clamp is the
/// same as at creation, horizontally and vertically, and is skipped whole if the query fails.
/// The final `SetWindowPos` passes `HWND_TOPMOST` in full screen and `HWND_NOTOPMOST` windowed
/// with `SWP_NOCOPYBITS` and no other flag, so both the move and the resize take effect.
///
/// **So, plainly, with the numbers.** Windowed 800x600 -> 1280x720 on a 1920x1080 screen with
/// Windows' usual 3/23/3 frame: `cx, cy` go 806x629 -> 1286x749, and retail keeps the window's
/// **centre** -- a window at `(100, 100)`, centre `(503, 414)`, is moved to
/// `(503 - 1286/2, 414 - 749/2) = (-140, 39)`, and the work-area clamp then pulls it to
/// `(0, 39)`. Retail does **not** return to the screen centre on a size change; that is the
/// full-screen arm and it runs only when the previous presentation was full screen.
/// The creation placement, [`placement`], *is* the screen centre, and it
/// is a different function, and `App::change_presentation` must not call it for a size change.
///
/// # DELIBERATE DIVERGENCE FROM RETAIL (client divergence CD-001)
///
/// A size change does not snap the window to the centre of the screen: **a size change keeps
/// the window's top-left and alters only its extent**, in the same spirit as full screen not
/// being always-on-top. Retail keeps the *centre*, so every retail size change still moves the
/// window. Both readings are recorded above so that a later reader can see this is a choice and
/// not an error. **Do not change this back to either centring** -- the GPU-tier window-position
/// test stops it coming back.
///
/// The divergence covers every arm that ends windowed. The **windowed -> windowed** arm is every
/// resolution pick *and* both edges of a forced resolution change -- the forced-resolution
/// override is inside the display-preference load, so the login and character-select screens
/// drive this path on **log-out and log-in** as well as on a drop-down pick. **Leaving full
/// screen** is the same rule applied to the rectangle the window had *before* it went full
/// screen: the window returns to that top-left, where retail centres it on the screen. The
/// caller remembers that rectangle, because the window's current one is the monitor's by then.
///
/// So `keep` is the windowed rectangle whose top-left is kept: the window's current rectangle for
/// a windowed change, and the rectangle it had before full screen when leaving full screen.
/// `None` -- the window-rectangle query failing, or a window that was never windowed before it
/// went full screen -- has no top-left to keep and falls back to the screen centre. Retail
/// answers `(0, 0)` for a failed query; a corner is not a position worth preserving, and the
/// centre is what window creation would have given the window.
///
/// # The off-screen rule
///
/// Keeping the top-left means a larger extent can hang off the bottom-right, so the rectangle is
/// pushed back by the **smallest move that puts it inside** the work area -- or, when the host work
/// area query gives nothing (see [`ScreenMetrics::work_area`]), inside the monitor's own rectangle,
/// `origin` to `origin + (cx_screen, cy_screen)`:
///
/// ```text
/// if (x + cx > area.right)  x = area.right  - cx
/// if (x < area.left)        x = area.left
/// if (y + cy > area.bottom) y = area.bottom - cy
/// if (y < area.top)         y = area.top
/// ```
///
/// Left and top are tested **last** and therefore win, so a window larger than the area is pinned
/// to the area's top-left and bleeds off the far edge rather than off the near one: the caption
/// and the close button stay reachable, which is the whole point of a clamp.
///
/// This is not retail's clamp, deliberately. Retail's `x = (work.right - work.left) - cx`,
/// which it uses both here and at window creation, is the client's own confusion of a width with a
/// right edge -- on the primary monitor, whose work area starts at `left = 0`, the two are the
/// same number, and anywhere else it teleports the window into the primary monitor's coordinate
/// space. A clamp that moves the window further than it must is exactly what this divergence exists
/// to stop. [`placement`] keeps retail's form unchanged, because that is the creation path and a
/// window being created has no position to preserve.
#[must_use]
pub fn change_presentation_placement(
    full_screen: bool,
    visible: bool,
    width: i32,
    height: i32,
    keep: Option<Rect>,
    m: &ScreenMetrics,
) -> Placement {
    // The full-screen test jumps the whole windowed block, leaving the seeds at zero, so full
    // screen is the creation arm unchanged, which in this
    // build is the borderless monitor rectangle.
    if full_screen {
        return placement(true, visible, width, height, m);
    }
    // The style and `cx`/`cy` are the same arithmetic in both of the client's placement
    // functions; only `x`/`y` differ, and they are replaced wholesale below
    // rather than adjusted, so retail's clamp inside [`placement`] cannot leak into this
    // rectangle.
    let mut p = placement(false, visible, width, height, m);
    let (cx, cy) = (p.cx, p.cy);
    let (mut x, mut y) = match keep {
        // **The divergence.** Retail computes `(r.left + r.right - cx)/2` here for a windowed
        // change, and the screen centre on leaving full screen.
        Some(r) => (r.left, r.top),
        // No windowed top-left to keep, so this is the creation path's screen centre.
        None => (
            m.origin.0 + m.cx_screen / 2 - cx / 2,
            m.origin.1 + m.cy_screen / 2 - cy / 2,
        ),
    };
    let area = m.work_area.unwrap_or(Rect {
        left: m.origin.0,
        top: m.origin.1,
        right: m.origin.0 + m.cx_screen,
        bottom: m.origin.1 + m.cy_screen,
    });
    if x + cx > area.right {
        x = area.right - cx;
    }
    if x < area.left {
        x = area.left;
    }
    if y + cy > area.bottom {
        y = area.bottom - cy;
    }
    if y < area.top {
        y = area.top;
    }
    p.x = x;
    p.y = y;
    p
}

/// `WS_VISIBLE`, the one style bit window creation adds after the mode arithmetic.
pub const WS_VISIBLE: u32 = 0x1000_0000;

/// `WS_POPUP` -- the bit that separates [`style::FULLSCREEN`] from [`style::WINDOWED`],
/// and the one a host asks about to decide whether the window has a frame at all.
pub const WS_POPUP: u32 = 0x8000_0000;

#[cfg(test)]
mod tests {
    use super::msg::*;
    use super::*;

    fn ready_active() -> DeviceState {
        DeviceState {
            is_ready: true,
            is_active_app: true,
            is_minimized: false,
            ..DeviceState::default()
        }
    }

    // The message table, exercised by one scripted sequence: activate/deactivate, WM_CLOSE,
    // Alt+Enter toggle, resize, cursor clip.
    #[test]
    fn the_scripted_sequence_walks_the_message_table() {
        let mut s = DeviceState {
            is_ready: true,
            ..DeviceState::default()
        };

        // Activation: WM_ACTIVATEAPP with a non-zero wParam while not minimized activates.
        s.is_minimized = false;
        let r = wnd_proc(&mut s, WM_ACTIVATEAPP, 1, 0);
        assert!(r.handled);
        assert_eq!(r.effects, vec![Effect::Activate]);
        assert!(s.is_active_app);
        // Doing it twice is idempotent: Activate only fires on a transition.
        let r = wnd_proc(&mut s, WM_ACTIVATEAPP, 1, 0);
        assert!(r.effects.is_empty());

        // Deactivation.
        let r = wnd_proc(&mut s, WM_ACTIVATEAPP, 0, 0);
        assert_eq!(r.effects, vec![Effect::Deactivate]);
        assert!(!s.is_active_app);

        // WM_ACTIVATE: HIWORD is the minimized flag, LOWORD the activation state.
        let r = wnd_proc(&mut s, WM_ACTIVATE, 1, 0); // WA_ACTIVE, not minimized
        assert!(s.is_active_app);
        assert_eq!(r.effects, vec![Effect::Activate]);
        let r = wnd_proc(&mut s, WM_ACTIVATE, 0, 0); // WA_INACTIVE
        assert_eq!(r.effects, vec![Effect::Deactivate]);
        // Activating while minimized (HIWORD != 0) does not activate.
        let r = wnd_proc(&mut s, WM_ACTIVATE, (1 << 16) | 1, 0);
        assert!(s.is_minimized);
        assert!(r.effects.is_empty());
        assert!(!s.is_active_app);

        // Resize: minimizing sets the flag and deactivates; there is no geometry handling at all.
        let mut s = ready_active();
        let r = wnd_proc(&mut s, WM_SIZE, SIZE_MINIMIZED, 0);
        assert!(s.is_minimized);
        assert!(!s.is_active_app);
        assert_eq!(r.effects, vec![Effect::Deactivate]);
        // A restore-size message is handled and changes nothing.
        let r = wnd_proc(&mut s, WM_SIZE, 0, 0);
        assert!(r.handled);
        assert!(r.effects.is_empty());

        // Alt+Enter latches the toggle; the flip happens at the end of the event loop.
        let mut s = ready_active();
        let r = wnd_proc(&mut s, WM_SYSKEYDOWN, VK_RETURN, 0);
        assert!(!r.handled, "the default procedure still sees Alt+Enter");
        assert!(s.toggle_full_screen_mode);
        assert!(!s.full_screen);
        assert!(!finish_event_loop(&mut s, true));
        assert!(
            s.full_screen,
            "the toggle applies in the event loop, not in WndProc"
        );
        assert!(!s.toggle_full_screen_mode);

        // WM_CLOSE ends the loop.
        let r = wnd_proc(&mut s, WM_CLOSE, 0, 0);
        assert!(r.handled);
        assert!(s.is_done);
        assert!(finish_event_loop(&mut s, true));
    }

    // Oracle: the retail window procedure's `WM_SYSKEYDOWN` arm clears its handled flag for
    // `VK_F4` (and `VK_RETURN`) before the common exit, so both reach `DefWindowProcA` even though
    // every other system key is swallowed. The default procedure turns Alt+F4 into
    // `WM_SYSCOMMAND`/`SC_CLOSE`, and that arm ends the client.
    #[test]
    fn alt_f4_reaches_the_default_procedure_and_its_close_ends_the_client() {
        let mut s = ready_active();
        assert!(!s.sys_keys_enabled, "every other system key is swallowed");
        let r = wnd_proc(&mut s, WM_SYSKEYDOWN, VK_F4, 0);
        assert!(!r.handled, "the default procedure sees Alt+F4");
        assert_eq!(r.effects, vec![Effect::ForwardToInputManager]);
        assert!(!s.is_done, "the key itself changes nothing");
        // The default procedure's answer to Alt+F4.
        let r = wnd_proc(&mut s, WM_SYSCOMMAND, SC_CLOSE, 0);
        assert!(r.handled);
        assert!(s.is_done, "the close ends the client");
        assert!(finish_event_loop(&mut s, true));
    }

    // Oracle: the same table's WM_SYSKEYDOWN/UP rows -- "any other key -> handled only if
    // !sys_keys_enabled", combined with Init step 2, which clears sys_keys_enabled because the
    // window has no menu. "This is effectively always taken, so Alt-key system commands are
    // disabled for the life of the client."
    #[test]
    fn alt_key_system_commands_are_disabled_for_the_life_of_the_client() {
        let mut s = ready_active();
        assert!(!s.sys_keys_enabled, "Init clears it");
        // Any other system key is swallowed.
        let r = wnd_proc(&mut s, WM_SYSKEYDOWN, 0x41, 0);
        assert!(r.handled);
        let r = wnd_proc(&mut s, WM_SYSKEYUP, 0x41, 0);
        assert!(r.handled);
        // With the flag set (which the shipped client never reaches) they fall through.
        s.sys_keys_enabled = true;
        let r = wnd_proc(&mut s, WM_SYSKEYDOWN, 0x41, 0);
        assert!(!r.handled);
        // ..but the input manager is told either way.
        assert_eq!(r.effects, vec![Effect::ForwardToInputManager]);
    }

    // Oracle: the WM_ERASEBKGND row -- "if is_ready && is_active_app return 1 (suppress the
    // background erase -- this is what stops the black BLACK_BRUSH flash); otherwise fall through
    // to DefWindowProc".
    #[test]
    fn the_background_erase_is_suppressed_only_when_ready_and_active() {
        let mut s = ready_active();
        let r = wnd_proc(&mut s, WM_ERASEBKGND, 0, 0);
        assert!(r.handled);
        assert_eq!(r.result, 1);

        let mut s = DeviceState {
            is_ready: false,
            is_active_app: true,
            ..DeviceState::default()
        };
        assert!(!wnd_proc(&mut s, WM_ERASEBKGND, 0, 0).handled);
        let mut s = DeviceState {
            is_ready: true,
            is_active_app: false,
            ..DeviceState::default()
        };
        assert!(!wnd_proc(&mut s, WM_ERASEBKGND, 0, 0).handled);
    }

    // Oracle: the WM_SYSCOMMAND row -- SC_CLOSE quits, SC_SCREENSAVE and SC_MONITORPOWER are
    // blocked, everything else goes to DefWindowProc. Plus the rebuild note that
    // SetThreadExecutionState(ES_DISPLAY_REQUIRED) is held for the whole session.
    #[test]
    fn the_screensaver_and_monitor_power_down_are_blocked() {
        let mut s = ready_active();
        // The low nibble is masked off before the compare, which is what Windows requires.
        assert!(wnd_proc(&mut s, WM_SYSCOMMAND, SC_SCREENSAVE | 0x0F, 0).handled);
        assert!(!s.is_done);
        assert!(wnd_proc(&mut s, WM_SYSCOMMAND, SC_MONITORPOWER, 0).handled);
        assert!(!s.is_done);
        // SC_MOVE and friends fall through.
        assert!(!wnd_proc(&mut s, WM_SYSCOMMAND, 0xF010, 0).handled);
        // SC_CLOSE quits.
        assert!(wnd_proc(&mut s, WM_SYSCOMMAND, SC_CLOSE, 0).handled);
        assert!(s.is_done);
    }

    // Oracle: the mouse rows -- "WM_?BUTTONDOWN (L/R/M/X): SetFocus(hwnd) first, then the
    // WM_MOUSEMOVE path", and "_TrackMouseEvent(...) is armed on the first mouse message after each
    // WM_MOUSELEAVE, so the UI gets a reliable 'cursor left the window' event".
    #[test]
    fn the_mouse_path_arms_track_mouse_event_once_per_leave() {
        let mut s = ready_active();
        // The first mouse message arms the leave notification.
        let r = wnd_proc(&mut s, WM_MOUSEMOVE, 0, 0);
        assert_eq!(
            r.effects,
            vec![
                Effect::TrackMouseEvent,
                Effect::ForwardToBrowser,
                Effect::ForwardToInputManager
            ]
        );
        assert!(s.track_leave_called);
        // Subsequent ones do not re-arm it.
        let r = wnd_proc(&mut s, WM_MOUSEMOVE, 0, 0);
        assert_eq!(
            r.effects,
            vec![Effect::ForwardToBrowser, Effect::ForwardToInputManager]
        );
        // WM_MOUSELEAVE disarms, and the next mouse message arms again.
        let r = wnd_proc(&mut s, WM_MOUSELEAVE, 0, 0);
        assert!(!s.track_leave_called);
        assert_eq!(r.effects, vec![Effect::ForwardToInputManager]);
        let r = wnd_proc(&mut s, WM_MOUSEMOVE, 0, 0);
        assert!(r.effects.contains(&Effect::TrackMouseEvent));

        // A button-down takes focus *first*.
        for m in [
            WM_LBUTTONDOWN,
            WM_RBUTTONDOWN,
            WM_MBUTTONDOWN,
            WM_XBUTTONDOWN,
        ] {
            let mut s = ready_active();
            s.track_leave_called = true;
            let r = wnd_proc(&mut s, m, 0, 0);
            assert_eq!(r.effects.first(), Some(&Effect::SetFocus), "message {m:#x}");
        }
        // A button-up does not.
        let mut s = ready_active();
        s.track_leave_called = true;
        let r = wnd_proc(&mut s, WM_LBUTTONUP, 0, 0);
        assert!(!r.effects.contains(&Effect::SetFocus));
    }

    // Oracle: the WM_POWERBROADCAST row -- "wParam == 0 (PBT_APMQUERYSUSPEND) -> return 0x424D5144
    // ('BMQD', i.e. deny suspend) and handled".
    #[test]
    fn a_suspend_query_is_denied() {
        let mut s = ready_active();
        let r = wnd_proc(&mut s, WM_POWERBROADCAST, PBT_APMQUERYSUSPEND, 0);
        assert!(r.handled);
        assert_eq!(r.result, 0x424D_5144);
        assert_eq!(&r.result.to_be_bytes()[4..], b"BMQD");
        // Any other power event falls through.
        assert!(!wnd_proc(&mut s, WM_POWERBROADCAST, 4, 0).handled);
    }

    // Oracle: the WM_ENTERSIZEMOVE / WM_EXITSIZEMOVE rows -- "in full screen: handled (ignored);
    // otherwise Deactivate()" and "not full screen -> Activate(); handled".
    #[test]
    fn the_size_move_modal_loop_deactivates_only_in_windowed_mode() {
        let mut s = ready_active();
        let r = wnd_proc(&mut s, WM_ENTERSIZEMOVE, 0, 0);
        assert_eq!(r.effects, vec![Effect::Deactivate]);
        let r = wnd_proc(&mut s, WM_EXITSIZEMOVE, 0, 0);
        assert_eq!(r.effects, vec![Effect::Activate]);

        let mut s = DeviceState {
            full_screen: true,
            ..ready_active()
        };
        let r = wnd_proc(&mut s, WM_ENTERSIZEMOVE, 0, 0);
        assert!(r.handled);
        assert!(r.effects.is_empty());
        assert!(s.is_active_app);
    }

    // Oracle: the event loop's epilogue -- the toggle only takes effect while a renderer exists and the
    // app is active, and allow_full_screen_mode is the master switch.
    #[test]
    fn the_full_screen_toggle_respects_the_master_switch_and_activity() {
        // Inactive: the latch clears without flipping.
        let mut s = DeviceState {
            is_active_app: false,
            toggle_full_screen_mode: true,
            ..DeviceState::default()
        };
        finish_event_loop(&mut s, true);
        assert!(!s.full_screen);
        assert!(!s.toggle_full_screen_mode);
        // No renderer: likewise.
        let mut s = DeviceState {
            is_active_app: true,
            toggle_full_screen_mode: true,
            ..DeviceState::default()
        };
        finish_event_loop(&mut s, false);
        assert!(!s.full_screen);
        // Full-screen mode disallowed: the flip always lands on false.
        let mut s = DeviceState {
            is_active_app: true,
            toggle_full_screen_mode: true,
            allow_full_screen_mode: false,
            full_screen: true,
            ..DeviceState::default()
        };
        finish_event_loop(&mut s, true);
        assert!(!s.full_screen);
        // And back out of full screen when allowed.
        let mut s = DeviceState {
            is_active_app: true,
            toggle_full_screen_mode: true,
            full_screen: true,
            ..DeviceState::default()
        };
        finish_event_loop(&mut s, true);
        assert!(!s.full_screen);
    }

    // Oracle: the frame sleep -- Sleep(0) when active, and frames at least 99 ms apart when
    // inactive (about a 10 fps background cap).
    #[test]
    fn the_frame_sleep_is_zero_when_active_and_caps_the_background_at_99ms() {
        // Foreground: a bare yield, no frame limiter.
        assert_eq!(frame_sleep_ms(true, 1000, 0), 0);
        assert_eq!(frame_sleep_ms(true, 0, 0), 0);

        // Background: sleep out the remainder of the 99 ms budget.
        assert_eq!(frame_sleep_ms(false, 1000, 1000), 99);
        assert_eq!(frame_sleep_ms(false, 1050, 1000), 49);
        assert_eq!(
            frame_sleep_ms(false, 1099, 1000),
            0,
            "exactly 99 ms elapsed sleeps zero"
        );
        assert_eq!(
            frame_sleep_ms(false, 5000, 1000),
            0,
            "a long frame is not penalised"
        );

        // Consecutive background frames are at least 99 ms apart, which is the ~10.1 fps cap.
        let mut now = 0u32;
        let mut last = 0u32;
        let mut stamps = Vec::new();
        for _ in 0..5 {
            let ms = frame_sleep_ms(false, now, last);
            now += ms;
            stamps.push(now);
            last = now;
            now += 1; // a 1 ms frame
        }
        for pair in stamps.windows(2) {
            assert!(pair[1] - pair[0] >= 99, "{pair:?}");
        }

        // timeGetTime wraps every 49.7 days; the subtraction is on DWORDs, so it must wrap too
        // rather than saturate.
        assert_eq!(frame_sleep_ms(false, 10, u32::MAX - 10), 78);
    }

    // Oracle: the two `dwStyle` values and their decomposition; neither contains `WS_THICKFRAME`
    // or `WS_MAXIMIZEBOX`.
    #[test]
    fn the_window_styles_are_the_documented_ones() {
        assert_eq!(style::WINDOWED, 0x12CA_0000);
        assert_eq!(style::FULLSCREEN, 0x9200_0000);
        assert_eq!(style::for_mode(true), style::WINDOWED);
        assert_eq!(style::for_mode(false), style::FULLSCREEN);
        assert_eq!(style::EX_STYLE, 0);
        assert_eq!(style::CLASS_NAME, "Turbine Device Class");
        // WS_THICKFRAME (0x00040000) and WS_MAXIMIZEBOX (0x00010000) are absent from both.
        assert_eq!(style::WINDOWED & 0x0004_0000, 0, "no WS_THICKFRAME");
        assert_eq!(style::WINDOWED & 0x0001_0000, 0, "no WS_MAXIMIZEBOX");
        assert_eq!(style::FULLSCREEN & 0x0004_0000, 0);
        // Reconstruct both styles from their individual flags, bit for bit.
        let ws_caption = 0x00C0_0000u32;
        let ws_sysmenu = 0x0008_0000;
        let ws_minimizebox = 0x0002_0000;
        let ws_clipchildren = 0x0200_0000;
        let ws_visible = 0x1000_0000;
        let ws_popup = 0x8000_0000u32;
        assert_eq!(
            style::WINDOWED,
            ws_caption | ws_sysmenu | ws_minimizebox | ws_clipchildren | ws_visible
        );
        assert_eq!(style::FULLSCREEN, ws_popup | ws_clipchildren | ws_visible);
    }

    // Oracle: windowed outer size adds the dialog frame on each side and the caption above.
    #[test]
    fn the_windowed_outer_size_adds_the_frame_and_caption() {
        // Typical Windows metrics: SM_CXDLGFRAME 3, SM_CYCAPTION 23, SM_CYDLGFRAME 3.
        assert_eq!(outer_size(800, 600, true, 3, 23, 3), (806, 629));
        // Full screen skips the branch entirely.
        assert_eq!(outer_size(800, 600, false, 3, 23, 3), (800, 600));
    }

    // Oracle: the table's default row -- "anything else: not handled -> DefWindowProcA".
    #[test]
    fn unknown_messages_fall_through_to_defwindowproc() {
        let mut s = ready_active();
        for m in [0x0003u32, 0x0007_0000, 0x00FF, 0x0400] {
            if m == 0x0007 {
                continue;
            }
            let r = wnd_proc(&mut s, m, 0, 0);
            assert!(!r.handled, "message {m:#x} must fall through");
        }
    }
}
