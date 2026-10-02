//! Everything `App` needs from the operating system, behind traits: the clock and the frame
//! pacer, the window host and its event loop, the display-mode enumeration, the monitor metrics,
//! the window icon, and the fly-cam's raw key input. Keeping them here is what lets `App` be
//! built and driven without a window.
//!
//! The three traits:
//!
//! * `clock::Clock` — what time is it (monotonic, wall, and the local zone's shift);
//! * `clock::Pacer` — the single frame limiter;
//! * `window::WindowHost` — the window, its event pump, and the display it sits on.
//!
//! Each has a real implementation (`clock::SystemClock`, `window::WinitWindow`) and a headless
//! one (`clock::FixedStepClock`, `window::NullWindow`), and `App::with_presentation` takes all
//! three, so a headless `App` is `NullPresentation` + `NullWindow` + `FixedStepClock` and names no
//! platform API at all. `winit` and the `windows` crate are named **here and in `crate::pump`**
//! and nowhere else in the client's logic modules.
//!
//! The `cfg(windows)` items — the `SPI_GETWORKAREA` work-area query and the WinRT time-zone call —
//! stay Win32-only inside this module. The traits are cross-platform. The application icon is in
//! `window` and has an arm per platform: a linked resource on Windows, window pixels on X11, and
//! nothing at all on macOS, which reads it out of the application bundle before the process
//! starts.
//!
//! All three *traits* are defined in [`dereth_client_runtime::platform`], with `HostEvent`,
//! `PumpedEvents`, `NullWindow`, `headless_screen_metrics`, `system_unix_time` and
//! `Win32Message`; the two submodules below re-export them, and what they add is what names the
//! zone, the renderer or `winit`.

/// `Timer`, and the local time zone: every clock
/// the client reads.
pub mod clock;
/// The plain key and mouse-button identities the window pump hands the client, re-exported from
/// `dereth-client-runtime`.
pub mod keys;
/// The host's text conversion and the chat log's file handle, re-exported from
/// `dereth-client-runtime`.
pub use dereth_client_runtime::platform::text;
/// The modal error box: the seam is `dereth_client_runtime`'s, the OS message box is here.
pub mod dialog;
/// The window, the display, and the event pump.
pub mod window;

/// Re-exported as `crate::platform::local_utc_offset_secs`, the path tests and other crates
/// name.
pub use clock::local_utc_offset_secs;
