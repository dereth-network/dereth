//! The cursor: the client shell's cursor state ([`dereth_client_shell::cursor`], re-exported
//! here) over the desktop's cursor images.
//!
//! On Windows a cursor image is a system cursor built from the dat surface's icon bits and
//! installed as the window's. Off Windows, where winit 0.29 offers no custom-cursor API, the images
//! are resolved and cached and the pointer stays the system arrow: the shell's
//! [`PortableCursors`].

pub use dereth_client_shell::cursor::*;

/// The desktop's cursor images, put on `window`.
#[must_use]
pub fn desktop_cursor_images(window: Option<isize>) -> Box<dyn CursorImages> {
    #[cfg(windows)]
    {
        Box::new(WindowCursors {
            hwnd: window,
            built: std::collections::HashMap::new(),
        })
    }
    #[cfg(not(windows))]
    {
        let _ = window;
        Box::new(PortableCursors)
    }
}

/// System cursors built from icon bits and installed on the window `SetCursor` applies to.
#[cfg(windows)]
struct WindowCursors {
    /// `None` under `--headless`.
    hwnd: Option<isize>,
    /// Every `HCURSOR` built so far, keyed by what `CreateIconIndirect` was given.
    // ORDER-OK: a cache, only ever looked up.
    built: std::collections::HashMap<CursorKey, dereth_render::cursor::WinCursor>,
}

#[cfg(windows)]
impl CursorImages for WindowCursors {
    fn build(&mut self, key: CursorKey, bits: &dereth_render::cursor::IconBits) -> bool {
        match dereth_render::cursor::WinCursor::new(bits) {
            Ok(c) => {
                self.built.insert(key, c);
                true
            }
            Err(_) => false,
        }
    }

    fn install(&mut self, key: CursorKey) -> Option<CursorInstall> {
        let icon = self.built.get(&key);
        let mut took = false;
        if let (Some(hwnd), Some(c)) = (self.hwnd, icon) {
            // Install the icon as the window-class cursor, then make it current immediately.
            took = dereth_render::cursor::set_cursor(c);
            dereth_render::cursor::take_over_wm_setcursor(hwnd, Some(c));
        }
        Some(CursorInstall {
            window: self.hwnd.is_some(),
            icon: icon.is_some(),
            took,
        })
    }
}
