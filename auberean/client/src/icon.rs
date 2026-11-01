//! The application icon on the window.
//!
//! `build.rs` links `assets/auberean.ico` into the executable, which is enough for Explorer, the
//! task bar's pinned button and Alt-Tab: they read the file. A window's own icon is separate:
//! winit registers its window class with none, so on Windows the window takes the linked
//! resource by number, and on the window systems that take an icon as pixels it takes
//! `assets/auberean-256.png`. A macOS window shows its bundle's icon, so there is nothing to set.
//! A missing icon is logged, never a reason not to start.

use winit::window::WindowAttributes;

/// The icon's resource number in `assets/auberean-client.rc`. Keep the two in step.
#[cfg(windows)]
const IDI_APP: u16 = 1;

/// The attributes with the application's icon on the window.
#[cfg(windows)]
pub fn with_icon(attrs: WindowAttributes) -> WindowAttributes {
    use winit::platform::windows::{IconExtWindows as _, WindowAttributesExtWindows as _};

    // The title bar's small icon and the task bar's big one, at 100% scale; the `.ico` carries
    // a frame for each, so Windows picks rather than resamples.
    let load = |px: u32| {
        winit::window::Icon::from_resource(IDI_APP, Some(winit::dpi::PhysicalSize::new(px, px)))
            .map_err(|e| tracing::warn!("icon resource {IDI_APP} at {px}x{px}: {e}"))
            .ok()
    };
    attrs.with_window_icon(load(16)).with_taskbar_icon(load(32))
}

/// The attributes with the application's icon on the window.
#[cfg(target_os = "macos")]
pub fn with_icon(attrs: WindowAttributes) -> WindowAttributes {
    attrs
}

/// The attributes with the application's icon on the window.
#[cfg(not(any(windows, target_os = "macos")))]
pub fn with_icon(attrs: WindowAttributes) -> WindowAttributes {
    attrs.with_window_icon(from_png(include_bytes!("../assets/auberean-256.png")))
}

/// The 256-pixel picture decoded: one large size, which window managers scale down well.
#[cfg(not(any(windows, target_os = "macos")))]
fn from_png(bytes: &[u8]) -> Option<winit::window::Icon> {
    let mut reader = png::Decoder::new(std::io::Cursor::new(bytes))
        .read_info()
        .map_err(|e| tracing::warn!("window icon: {e}"))
        .ok()?;
    let mut pixels = vec![0; reader.output_buffer_size()];
    let frame = reader
        .next_frame(&mut pixels)
        .map_err(|e| tracing::warn!("window icon: {e}"))
        .ok()?;
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
