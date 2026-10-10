//! Window lifecycle and device events supplied by a host.

use crate::keys::{Key, MouseButton};

/// One window-system event, as the client reads it: the window's lifecycle, which the runtime
/// consumes, and the devices, which this client turns into actions.
///
/// A host event this list does not name is the equivalent of falling through to
/// `DefWindowProcA` and is dropped by the host rather than carried.
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
    /// Which of the modifier bits are held. Only Alt is read — it is what separates `WM_SYSKEY*`
    /// from `WM_KEY*` — so only Alt is carried.
    ModifiersChanged { alt: bool },
    /// One key transition. `text` is the host's **fully translated** text — `U+0016` for Ctrl+V,
    /// not `v` — which is what `TranslateMessage`'s `WM_CHAR` carries.
    KeyboardInput {
        key: Key,
        pressed: bool,
        text: Option<String>,
    },
    /// `WM_MOUSEMOVE`, in client pixels.
    CursorMoved { x: f64, y: f64 },
    /// The mouse's movement while the host holds the pointer still for a camera drag, in client
    /// pixels as the pointer would have moved: what the host reports in place of
    /// [`HostEvent::CursorMoved`] while it holds the pointer, which itself stays where it is.
    PointerMotion { dx: f64, dy: f64 },
    /// `WM_MOUSELEAVE`.
    CursorLeft,
    /// One button transition, at the last position [`HostEvent::CursorMoved`] reported.
    MouseInput { button: MouseButton, pressed: bool },
    /// `WM_MOUSEWHEEL`, in notches — one detent is 1.0.
    MouseWheel { notches: f32 },
}
