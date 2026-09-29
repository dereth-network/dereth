//! The window message table and the frame sleep, as a pure state machine.
//!
//! The state machine preserves the message pump, window class and window style behavior.
//!
//! In the client the window state is a block of statics rather than an object, so `DeviceState`
//! is that block and `wnd_proc` is the switch over it. Keeping the table a pure function of
//! `(state, message)` is what makes
//! it testable without a window: the message table is exercised by a scripted sequence, and a
//! scripted sequence needs the switch to be callable.
//!
//! The Win32 side — registering the class, creating the window and pumping messages — is not
//! implemented here. Everything this module describes is testable without it.
//! The one host query here reads a monitor's work area through an opaque handle; it neither creates
//! nor changes a window.
//!
//! The table, the sleep and the placement arithmetic live in
//! [`dereth_client_contract::window_proc`] and are re-exported here; what this module adds is the two
//! host queries below.

pub use dereth_client_contract::window_proc::*;

/// Read the player's Windows caret blink interval without changing it.
///
/// The native text tick makes this query whenever a focused text element ticks. The host calls
/// it once immediately before the UI frame, where at most one text element can hold focus.
#[cfg(windows)]
#[must_use]
pub fn caret_blink_time_seconds() -> f64 {
    use windows::Win32::UI::WindowsAndMessaging::GetCaretBlinkTime;

    // SAFETY: `GetCaretBlinkTime` has no arguments, dereferences no caller-owned memory and
    // returns the process desktop's read-only UINT setting. It retains no resources or pointers.
    caret_blink_time_seconds_from_millis(unsafe { GetCaretBlinkTime() })
}

/// The portable build has no USER32 binding. Keep the pre-host-wiring default used by Windows.
#[cfg(not(windows))]
#[must_use]
pub fn caret_blink_time_seconds() -> f64 {
    caret_blink_time_seconds_from_millis(530)
}

/// Read the usable rectangle for a monitor handle supplied by `winit`.
///
/// This is the per-monitor counterpart of retail's
/// `SystemParametersInfoA(SPI_GETWORKAREA)` call. It is read-only: no display mode, window, or
/// monitor setting is changed. Invalid handles and failed queries take retail's existing
/// no-work-area branch.
#[cfg(windows)]
#[must_use]
pub fn monitor_work_area(hmonitor: isize) -> Option<Rect> {
    use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, HMONITOR, MONITORINFO};

    if hmonitor == 0 || hmonitor == -1 {
        return None;
    }
    let mut info = MONITORINFO {
        cbSize: u32::try_from(std::mem::size_of::<MONITORINFO>()).ok()?,
        ..MONITORINFO::default()
    };
    let handle = HMONITOR(hmonitor as *mut core::ffi::c_void);
    // SAFETY: Win32 validates the opaque handle and reports failure for invalid or stale values.
    // `info` is a live, writable stack allocation whose `cbSize` identifies its complete
    // initialized extent; `GetMonitorInfoW` retains neither pointer nor handle.
    if !unsafe { GetMonitorInfoW(handle, &mut info) }.as_bool() {
        return None;
    }
    Some(Rect {
        left: info.rcWork.left,
        top: info.rcWork.top,
        right: info.rcWork.right,
        bottom: info.rcWork.bottom,
    })
}
