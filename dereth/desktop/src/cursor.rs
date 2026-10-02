//! The cursor: the client shell's cursor state ([`dereth_client_shell::cursor`], re-exported
//! here) over the desktop's cursor images.
//!
//! A cursor image is the window system's own cursor, made from the dat surface's 32x32 icon and
//! installed as the window's pointer, on every desktop platform: Windows, macOS, X11 and Wayland.
//! Building one needs no window, which is why a headless run still builds them; installing one
//! needs the window, which a headless run does not have.

use std::collections::HashMap;
use std::rc::Weak;

use crate::platform::window::{
    cursor_picture, cursor_window, CursorPicture, DesktopWindow, HostCursor,
};

pub use dereth_client_shell::cursor::*;

/// The desktop's cursor images, put on the window whose handle is `window`.
#[must_use]
pub fn desktop_cursor_images(window: Option<isize>) -> Box<dyn CursorImages> {
    Box::new(WindowCursors::new(window.and_then(cursor_window)))
}

/// The dat cursors as the window system's cursors, put on the window.
///
/// Each cursor is made once, the first time it is installed, and kept for the rest of the run,
/// one per did and hotspot, and freed with these images.
struct WindowCursors {
    /// `None` under `--headless`; a window that has since closed does not upgrade.
    window: Option<Weak<DesktopWindow>>,
    /// Pictures built and not yet made into cursors, by what the icon builder was given.
    // ORDER-OK: a cache, only ever looked up.
    pictures: HashMap<CursorKey, CursorPicture>,
    /// Every cursor made so far.
    // ORDER-OK: a cache, only ever looked up.
    made: HashMap<CursorKey, HostCursor>,
}

impl WindowCursors {
    fn new(window: Option<Weak<DesktopWindow>>) -> Self {
        Self {
            window,
            pictures: HashMap::new(),
            made: HashMap::new(),
        }
    }

    /// The cursor for `key`, made from its picture the first time it is asked for.
    fn cursor(&mut self, window: &DesktopWindow, key: CursorKey) -> Option<HostCursor> {
        if let Some(made) = self.made.get(&key) {
            return Some(made.clone());
        }
        let picture = self.pictures.remove(&key)?;
        let made = window.make_cursor(picture)?;
        self.made.insert(key, made.clone());
        Some(made)
    }
}

impl CursorImages for WindowCursors {
    fn build(&mut self, key: CursorKey, bits: &dereth_render::cursor::IconBits) -> bool {
        match cursor_picture(bits) {
            Ok(picture) => {
                self.pictures.insert(key, picture);
                true
            }
            Err(e) => {
                tracing::warn!("cursor ({}, {}, {:#010X}): {e}", key.1, key.2, key.0 .0);
                false
            }
        }
    }

    fn install(&mut self, key: CursorKey) -> Option<CursorInstall> {
        let icon = self.made.contains_key(&key) || self.pictures.contains_key(&key);
        let window = self.window.as_ref().and_then(Weak::upgrade);
        let mut took = false;
        if let Some(window) = &window {
            if let Some(cursor) = self.cursor(window, key) {
                window.set_cursor(&cursor);
                took = true;
            }
        }
        Some(CursorInstall {
            window: window.is_some(),
            icon,
            took,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_primitives::DataId;

    /// A 16x16 opaque cursor with its hotspot at `hot`.
    fn bits(hot: (u32, u32)) -> dereth_render::cursor::IconBits {
        dereth_render::cursor::build(16, 16, &[[0x10, 0x20, 0x30, 0xFF]; 256], hot.0, hot.1)
            .expect("16x16 fits")
    }

    const DEFAULT: CursorKey = (DataId(0x0600_4D6E), 0, 0);

    /// Without a window a cursor is built and kept, and installing it reports that there is no
    /// window to put it on: what a headless run counts.
    #[test]
    fn without_a_window_a_cursor_is_built_and_not_installed() {
        let mut images = desktop_cursor_images(None);
        assert!(images.build(DEFAULT, &bits((0, 0))));
        assert_eq!(
            images.install(DEFAULT),
            Some(CursorInstall {
                window: false,
                icon: true,
                took: false,
            })
        );
        let other = (DataId(0x0600_4D72), 14, 14);
        assert_eq!(
            images.install(other),
            Some(CursorInstall {
                window: false,
                icon: false,
                took: false,
            }),
            "nothing was built for it"
        );
    }

    /// A handle no window on this thread has finds no window.
    #[test]
    fn a_handle_of_no_open_window_finds_none() {
        assert!(cursor_window(0x1234).is_none());
    }

    /// A cursor is built at either hotspot the client pushes, and at one past the 32x32 image,
    /// which is held to its edge rather than refused (`picture_hotspot`).
    #[test]
    fn a_hotspot_is_carried_and_one_outside_the_image_is_held_to_its_edge() {
        let mut images = WindowCursors::new(None);
        for (x, y) in [(0_i32, 0_i32), (14, 14), (40, 3)] {
            let key = (DataId(0x0600_4D72), x, y);
            let hot = (x.unsigned_abs(), y.unsigned_abs());
            assert!(images.build(key, &bits(hot)), "{hot:?}");
        }
        assert_eq!(images.pictures.len(), 3);
    }
}
