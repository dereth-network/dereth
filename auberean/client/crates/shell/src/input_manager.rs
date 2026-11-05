//! What stands between the devices and what hears them: whether the window is the active one, and
//! whether the pointer is held for mouse look.
//!
//! - While the window is not the active one, nothing hears the devices: a press or a release, the
//!   wheel, mouse motion or the pointer. The window going inactive lets go of everything held, so
//!   nothing stays held. Where the pointer went is heard when the window is active again.
//! - While the pointer is held nothing hears where it is, and taking hold of it puts it at the
//!   middle of the display.

use crate::input::Event;

/// The window's state as it decides what hears the devices.
#[derive(Clone, Debug)]
pub struct InputManager {
    active: bool,
    pointer_held: bool,
    /// Where the pointer went while the window was not active.
    missed: Option<(f32, f32)>,
}

impl Default for InputManager {
    // D2-04856
    fn default() -> Self {
        Self {
            active: true,
            pointer_held: false,
            missed: None,
        }
    }
}

impl InputManager {
    /// Whether an event goes on to the interface and the world's controls. The window's own events
    /// always do.
    // D2-04865
    pub fn admit(&mut self, e: &Event) -> bool {
        match *e {
            Event::Focus(on) => {
                self.active = on;
                true
            }
            Event::Pointer { x, y } if !self.active => {
                self.missed = Some((x, y));
                false
            }
            Event::Pointer { .. } => !self.pointer_held,
            Event::Key { .. }
            | Event::Button { .. }
            | Event::Wheel { .. }
            | Event::Motion { .. } => self.active,
            _ => true,
        }
    }

    /// The pointer's place to hear now that the window is active again, if it moved meanwhile.
    pub fn take_missed(&mut self) -> Option<(f32, f32)> {
        if self.active {
            self.missed.take()
        } else {
            None
        }
    }

    /// Whether the pointer is held.
    #[must_use]
    pub const fn pointer_held(&self) -> bool {
        self.pointer_held
    }

    /// Hold the pointer for mouse look, or give it back. Whether that changes anything, and where
    /// the pointer now is when it was taken hold of: the middle of a display of `size` pixels, to
    /// the whole pixel below.
    // D2-04870
    pub fn set_mouse_look_mode(
        &mut self,
        on: bool,
        size: (u32, u32),
    ) -> Option<Option<(f32, f32)>> {
        if on == self.pointer_held {
            return None;
        }
        self.pointer_held = on;
        #[allow(clippy::cast_precision_loss)] // a display's size
        Some(on.then_some(((size.0 >> 1) as f32, (size.1 >> 1) as f32)))
    }
}
