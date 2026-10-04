//! Mouse position, reference-counted capture, mouse-look and the 0.2 s idle re-tick.
//!
//! This covers the client's mouse button event, the mouse-mode switch, the per-frame device read,
//! the cursor sync and the `MouseLookBehavior` option.

use dereth_primitives::LocalTime;

/// Where the cursor goes when
/// mouse-look ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MouseLookBehavior {
    #[default]
    Center = 0,
    Remember = 1,
    GotoXy = 2,
}

/// The mouse action selected by per-frame input processing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseFrameAction {
    /// Nothing: the pointer did not move and either we are not in mouse-look or the idle window
    /// has not elapsed.
    None,
    /// Notify mouse movement with **absolute client coordinates**.
    Move { x: i32, y: i32 },
    /// Notify mouse-look with the delta, then recenter the cursor.
    Look { dx: i32, dy: i32 },
    /// The 0.2 s idle re-tick: mouse-look with a zero delta. This keeps
    /// mouse-turning applying while the mouse is held still, and it is not decorative.
    LookIdle,
}

/// The idle test is `last_input_event + 0.2 s < current_time`.
pub const MOUSE_LOOK_IDLE_SECONDS: f64 = 0.2;

/// Mouse position, capture ownership and mouse-look state.
#[derive(Debug)]
pub struct MouseState {
    /// The recorded mouse position. **Authoritative** — in mouse-look the OS cursor
    /// is warped to the screen centre every frame, so `GetCursorPos()` is meaningless and the UI
    /// hit-tests against this.
    pub pos: (i32, i32),
    /// The previous frame's position.
    prev_pos: (i32, i32),
    /// Capture references shared across **buttons and mouse-look**.
    capture_count: u32,
    /// True while the window holds the OS capture.
    pub captured: bool,
    /// Whether mouse-look is currently active.
    pub in_mouse_look: bool,
    /// The requested mouse-look state, which survives a focus loss.
    pub want_mouse_look: bool,
    pub behavior: MouseLookBehavior,
    /// The cursor position saved before entering mouse-look.
    pub pos_before_mouse_look: (i32, i32),
    /// Time of the last input event, used by the idle re-tick.
    pub last_input_event: LocalTime,
    /// Pointer movement accumulated by actions 2 and 3.
    pub non_mouse_pointer_movement: (i32, i32),
    pub display_size: (i32, i32),
}

impl Default for MouseState {
    fn default() -> Self {
        Self {
            pos: (0, 0),
            prev_pos: (0, 0),
            capture_count: 0,
            captured: false,
            in_mouse_look: false,
            want_mouse_look: false,
            behavior: MouseLookBehavior::Center,
            pos_before_mouse_look: (0, 0),
            last_input_event: LocalTime(0.0),
            non_mouse_pointer_movement: (0, 0),
            display_size: (1024, 768),
        }
    }
}

impl MouseState {
    #[must_use]
    pub const fn capture_count(&self) -> u32 {
        self.capture_count
    }

    /// On a press, increment the capture count and call `SetCapture` when it reaches 1.
    pub fn add_capture(&mut self) {
        self.capture_count += 1;
        if self.capture_count == 1 {
            self.captured = true;
        }
    }

    /// If the capture count is non-zero, decrement it and call `ReleaseCapture` when it reaches 0.
    pub fn release_capture(&mut self) {
        if self.capture_count != 0 {
            self.capture_count -= 1;
            if self.capture_count == 0 {
                self.captured = false;
            }
        }
    }

    /// `WM_CANCELMODE`: force-release: zero the capture count and call `ReleaseCapture`.
    pub fn cancel_capture(&mut self) {
        self.capture_count = 0;
        self.captured = false;
    }

    /// The mouse-mode switch, the entering half. Returns true when
    /// the caller should hide the cursor.
    pub fn enter_mouse_look(&mut self, active: bool) -> bool {
        if self.behavior == MouseLookBehavior::Remember {
            self.pos_before_mouse_look = self.pos;
        }
        self.add_capture();
        self.in_mouse_look = true;
        active
    }

    /// The leaving half.
    pub fn leave_mouse_look(&mut self) {
        match self.behavior {
            MouseLookBehavior::Center => {
                self.pos = (self.display_size.0 / 2, self.display_size.1 / 2);
            }
            MouseLookBehavior::Remember | MouseLookBehavior::GotoXy => {
                self.pos = self.pos_before_mouse_look;
            }
        }
        self.release_capture();
        self.in_mouse_look = false;
    }

    /// The per-frame read, step 3 -- compare this frame's position with the previous frame's.
    ///
    /// On a `Look` the caller must recentre: set the mouse position to `(w/2, h/2)` and `SetCursorPos` to the
    /// same point in screen space. [`recentre`] does the client-side half.
    ///
    /// [`recentre`]: MouseState::recentre
    pub fn frame(&mut self, now: LocalTime) -> MouseFrameAction {
        let moved = self.pos != self.prev_pos;
        if !moved {
            if self.in_mouse_look && self.last_input_event.0 + MOUSE_LOOK_IDLE_SECONDS < now.0 {
                return MouseFrameAction::LookIdle;
            }
            return MouseFrameAction::None;
        }
        self.last_input_event = now;
        let action = if self.in_mouse_look {
            MouseFrameAction::Look {
                dx: self.pos.0 - self.prev_pos.0,
                dy: self.pos.1 - self.prev_pos.1,
            }
        } else {
            MouseFrameAction::Move {
                x: self.pos.0,
                y: self.pos.1,
            }
        };
        self.prev_pos = self.pos;
        action
    }

    /// The recentre that follows a `Look`.
    pub fn recentre(&mut self) {
        self.pos = (self.display_size.0 / 2, self.display_size.1 / 2);
        self.prev_pos = self.pos;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// the counter spans buttons *and*
    /// mouse-look, and reaches zero exactly once.
    #[test]
    fn capture_is_reference_counted_across_buttons_and_mouse_look() {
        let mut m = MouseState::default();
        m.add_capture(); // left down
        assert!(m.captured);
        m.add_capture(); // right down
        m.enter_mouse_look(true); // and mouse-look takes a reference too
        assert_eq!(m.capture_count(), 3);
        m.release_capture();
        m.release_capture();
        assert!(m.captured, "still held by mouse-look");
        m.leave_mouse_look();
        assert_eq!(m.capture_count(), 0);
        assert!(!m.captured);
        // A release with the counter already at zero must not underflow.
        m.release_capture();
        assert_eq!(m.capture_count(), 0);
    }

    /// the idle tick fires only in mouse-look
    /// and only once 0.2 s has passed since the last input event.
    #[test]
    fn the_idle_retick_needs_mouse_look_and_two_tenths() {
        let mut m = MouseState {
            last_input_event: LocalTime(1.0),
            ..MouseState::default()
        };
        assert_eq!(
            m.frame(LocalTime(1.5)),
            MouseFrameAction::None,
            "not in mouse-look"
        );
        m.enter_mouse_look(true);
        assert_eq!(
            m.frame(LocalTime(1.1)),
            MouseFrameAction::None,
            "0.1 s is not enough"
        );
        assert_eq!(m.frame(LocalTime(1.21)), MouseFrameAction::LookIdle);
    }

    /// a move outside mouse-look reports absolute client
    /// coordinates; inside it reports the delta and then recentres.
    #[test]
    fn move_versus_look() {
        let mut m = MouseState {
            display_size: (800, 600),
            ..MouseState::default()
        };
        m.pos = (10, 20);
        assert_eq!(
            m.frame(LocalTime(1.0)),
            MouseFrameAction::Move { x: 10, y: 20 }
        );
        m.enter_mouse_look(true);
        m.pos = (13, 25);
        assert_eq!(
            m.frame(LocalTime(1.1)),
            MouseFrameAction::Look { dx: 3, dy: 5 }
        );
        m.recentre();
        assert_eq!(m.pos, (400, 300));
    }

    /// Mouse-look behavior settings select held and toggled modes.
    #[test]
    fn mouse_look_exit_honours_the_behaviour() {
        let mut m = MouseState {
            display_size: (800, 600),
            ..MouseState::default()
        };
        m.pos = (7, 9);
        m.enter_mouse_look(true);
        m.leave_mouse_look();
        assert_eq!(m.pos, (400, 300));

        let mut m = MouseState {
            behavior: MouseLookBehavior::Remember,
            display_size: (800, 600),
            ..MouseState::default()
        };
        m.pos = (7, 9);
        m.enter_mouse_look(true);
        m.pos = (400, 300);
        m.leave_mouse_look();
        assert_eq!(m.pos, (7, 9));
    }
}
