//! One frame of the player's input, as the interface sees it: the pointer, its buttons, the wheel,
//! the keys pressed and the characters typed.

/// A key, by its virtual-key number.
pub mod vk {
    pub const BACK: usize = 0x08;
    pub const TAB: usize = 0x09;
    pub const ENTER: usize = 0x0D;
    pub const ESCAPE: usize = 0x1B;
    pub const SPACE: usize = 0x20;
    pub const END: usize = 0x23;
    pub const HOME: usize = 0x24;
    pub const LEFT: usize = 0x25;
    pub const UP: usize = 0x26;
    pub const RIGHT: usize = 0x27;
    pub const DOWN: usize = 0x28;
    pub const DELETE: usize = 0x2E;
    pub const SLASH: usize = 0xBF;
}

/// The input of one frame.
#[derive(Debug, Clone, Default)]
pub struct InputFrame {
    /// The pointer, in screen pixels.
    pub mouse: (f32, f32),
    /// Buttons held: left, right, middle.
    pub down: [bool; 3],
    /// Buttons that went down this frame.
    pub pressed: [bool; 3],
    /// Buttons that came up this frame.
    pub released: [bool; 3],
    /// Wheel notches this frame, up positive.
    pub wheel: f32,
    /// Keys that went down this frame (virtual keys), with auto-repeat.
    pub keys: Vec<usize>,
    /// Characters typed this frame.
    pub chars: Vec<char>,
    /// Shift and control held.
    pub shift: bool,
    pub ctrl: bool,
    /// Set by a widget that took this frame's press: the world does not see it.
    pub captured: bool,
    /// Set by a text box that has the keyboard.
    pub text_focus: bool,
    /// Where a drag began and whether one is in progress.
    pub drag_origin: Option<(f32, f32)>,
    /// The game's actions this frame that belong to the interface (a shortcut key, a window's
    /// key, logging out), as the action map numbers them.
    pub actions: Vec<u32>,
    /// The spell keys the game raised this frame.
    pub magic: Vec<dereth_client_contract::view::MagicNotice>,
    /// Whether the player is off the ground, which refuses a log-off.
    pub airborne: bool,
    /// This frame's left press is the second of a double-click.
    pub double: bool,
    /// The last left press: when (milliseconds) and where, for telling a double-click.
    pub last_left: Option<(u32, (f32, f32))>,
    /// The windows drawn over what is being drawn now: the pointer over one of them is not over
    /// anything under it, so a press lands only on the window on top.
    pub occluders: Vec<crate::draw::Rect>,
    /// The control a press began on that keeps the pointer while the left button stays down,
    /// wherever it goes (a scrollbar's thumb being dragged).
    pub held: Option<u64>,
    /// The caret and the selection of the line that has the keyboard.
    pub edit: crate::ui::edit::EditState,
    /// The clipboard's text, read for this frame's paste key.
    pub paste: Option<String>,
    /// Text copied, waiting for the host to put it on the clipboard.
    pub copied: Option<String>,
}

impl InputFrame {
    /// Forget this frame's edges, keeping what is held.
    pub fn next_frame(&mut self) {
        self.pressed = [false; 3];
        self.released = [false; 3];
        self.wheel = 0.0;
        self.keys.clear();
        self.chars.clear();
        self.captured = false;
        // A text box that still has the keyboard says so again next frame; with none, the next
        // line to take it starts afresh.
        if !self.text_focus {
            self.edit.release();
        }
        self.text_focus = false;
        self.paste = None;
        self.actions.clear();
        self.magic.clear();
        self.double = false;
    }

    /// Note a left press at `time_ms`: the second within half a second and a few pixels of the
    /// first is a double-click.
    pub fn note_left_press(&mut self, time_ms: u32) {
        let here = self.mouse;
        self.double = self.last_left.is_some_and(|(t, (x, y))| {
            time_ms.wrapping_sub(t) <= 500 && (x - here.0).abs() <= 6.0 && (y - here.1).abs() <= 6.0
        });
        self.last_left = if self.double {
            None
        } else {
            Some((time_ms, here))
        };
    }

    /// A double-click this frame inside `r`, taken.
    pub fn double_clicked(&mut self, r: &crate::draw::Rect) -> bool {
        if self.double && self.pressed[0] && self.hover(r) {
            self.pressed[0] = false;
            self.double = false;
            self.captured = true;
            true
        } else {
            false
        }
    }

    /// Whether `key` went down this frame, taking it so nothing else sees it.
    pub fn take_key(&mut self, key: usize) -> bool {
        if let Some(i) = self.keys.iter().position(|&k| k == key) {
            self.keys.remove(i);
            true
        } else {
            false
        }
    }

    /// A left press this frame inside `r`, taken.
    pub fn clicked(&mut self, r: &crate::draw::Rect) -> bool {
        if self.pressed[0] && self.hover(r) {
            self.pressed[0] = false;
            self.captured = true;
            true
        } else {
            false
        }
    }

    /// A right press this frame inside `r`, taken.
    pub fn right_clicked(&mut self, r: &crate::draw::Rect) -> bool {
        if self.pressed[1] && self.hover(r) {
            self.pressed[1] = false;
            self.captured = true;
            true
        } else {
            false
        }
    }

    /// Whether the pointer is in `r`, and not over a window drawn over it.
    #[must_use]
    pub fn hover(&self, r: &crate::draw::Rect) -> bool {
        let (x, y) = self.mouse;
        r.contains(x, y) && !self.occluders.iter().any(|o| o.contains(x, y))
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental Horizon interface)
    use super::*;
    use crate::draw::Rect;

    #[test]
    fn a_press_under_a_window_drawn_over_it_lands_on_nothing_below() {
        let under = Rect::new(0.0, 0.0, 100.0, 100.0);
        let mut input = InputFrame {
            mouse: (50.0, 50.0),
            pressed: [true, true, false],
            occluders: vec![Rect::new(40.0, 40.0, 100.0, 100.0)],
            ..InputFrame::default()
        };
        assert!(!input.hover(&under));
        assert!(!input.clicked(&under));
        assert!(!input.right_clicked(&under));
        input.occluders.clear();
        assert!(
            input.clicked(&under),
            "with nothing over it the press is its"
        );
    }

    #[test]
    fn the_keyboard_goes_back_to_the_game_the_frame_no_text_box_holds_it() {
        let mut input = InputFrame {
            text_focus: true,
            ..InputFrame::default()
        };
        input.next_frame();
        assert!(!input.text_focus);
    }
}
