//! The plain data a host window contributes: the two raw handles, a `RECT` and the monitor
//! metrics.
//!
//! None of the three is presentation and none is a platform call — they are the *shape* of the
//! answers a window gives, which `dereth_client_runtime::platform::window::WindowHost` returns and
//! `dereth_desktop::platform::window::WinitWindow` fills in. They live here rather than in the
//! renderer because that trait is in a core crate, and a core crate may not name a renderer.
//!
//! `dereth_render::device::WindowHandles` and `dereth_render::window_proc::{Rect, ScreenMetrics}`
//! are `pub use`s of what is here.

pub use raw_window_handle::{RawDisplayHandle, RawWindowHandle};

/// The two handles a window contributes: enough for `ash-window` to make a surface on any of the
/// three platforms, and enough for `CreateSwapChainForHwnd` on Windows.
#[derive(Debug, Clone, Copy)]
pub struct WindowHandles {
    pub display: RawDisplayHandle,
    pub window: RawWindowHandle,
}

/// A monitor's rectangle and the frame metrics reads, in the process's
/// own (DPI-unaware, therefore virtual) coordinate space.
///
/// `cx_screen`/`cy_screen` are `GetSystemMetrics(SM_CXSCREEN = 0)` / `(SM_CYSCREEN = 1)`, which
/// describe the **primary** monitor and are the only screen the retail client can see;
/// [`Self::origin`] is this rebuild's one addition, so a borderless window can be put on the
/// monitor the player is actually using. It is `(0, 0)` for the primary monitor, which is where
/// retail always is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenMetrics {
    /// `GetSystemMetrics(SM_CXSCREEN = 0)`.
    pub cx_screen: i32,
    /// `GetSystemMetrics(SM_CYSCREEN = 1)`.
    pub cy_screen: i32,
    /// `GetSystemMetrics(SM_CXDLGFRAME = 7)`.
    pub cx_dlg_frame: i32,
    /// `GetSystemMetrics(SM_CYCAPTION = 4)`.
    pub cy_caption: i32,
    /// `GetSystemMetrics(SM_CYDLGFRAME = 8)`.
    pub cy_dlg_frame: i32,
    /// `SystemParametersInfoA(SPI_GETWORKAREA = 0x30, 16, &rect, 0)`. `None` is the call
    /// **failing**, which the device answers by skipping the whole clamp.
    pub work_area: Option<Rect>,
    /// The monitor's top-left. Retail has no such concept and is always at `(0, 0)`.
    pub origin: (i32, i32),
}

/// A Win32 `RECT`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}
