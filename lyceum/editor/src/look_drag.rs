//! The right button's drag that turns the camera. While it lasts the window hides the pointer and
//! holds it, reporting the mouse's movement instead, so the turn never stops at the window's or
//! the screen's edge; when the button comes up the pointer shows again where the drag began. A
//! press over the panels begins no turn and holds nothing.
//!
//! The hold is the desktop host's, as the client's camera drags use it: on Windows the pointer is
//! hidden and put back in the middle of the window whenever it moves, and on Wayland it is locked
//! where it is. A window that cannot hold it leaves the pointer shown, and the turn follows the
//! pointer as before.

use dereth_client_shell::pointer::HostPointer;

/// The camera's turn by the right button, and the window's hold on the pointer for it.
pub struct LookDrag {
    host: Box<dyn HostPointer>,
    /// Where the drag began, in the window's pixels, while it lasts.
    from: Option<(f32, f32)>,
    /// Whether the window holds the pointer for it.
    held: bool,
}

impl std::fmt::Debug for LookDrag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LookDrag")
            .field("from", &self.from)
            .field("held", &self.held)
            .finish_non_exhaustive()
    }
}

impl LookDrag {
    #[must_use]
    pub fn new(host: Box<dyn HostPointer>) -> Self {
        Self {
            host,
            from: None,
            held: false,
        }
    }

    /// The right button pressed or let go with the pointer at `at`, `over` the panels or not: a
    /// press over the world view begins a turn, the pointer held where it is; a release ends one,
    /// the pointer shown where it began. Answers where a turn begun goes on from.
    pub fn right_button(
        &mut self,
        pressed: bool,
        over: bool,
        at: (f32, f32),
    ) -> Option<(f32, f32)> {
        self.end();
        if pressed && !over {
            self.from = Some(at);
            self.held = self.host.capture((f64::from(at.0), f64::from(at.1)));
        }
        self.from
    }

    /// End a turn, if one goes on: a pointer held for it is shown again where it began.
    pub fn end(&mut self) {
        if let Some(at) = self.from.take() {
            if std::mem::take(&mut self.held) {
                self.host.release((f64::from(at.0), f64::from(at.1)));
            }
        }
    }

    /// Whether the window holds the pointer for a turn.
    #[must_use]
    pub fn holds(&self) -> bool {
        self.held
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the world editor's camera)
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    /// A window's hold that says what it was asked, and holds when `can`.
    struct Asked {
        can: bool,
        asked: Rc<RefCell<Vec<String>>>,
    }

    impl HostPointer for Asked {
        fn capture(&mut self, at: (f64, f64)) -> bool {
            self.asked.borrow_mut().push(format!("hold {at:?}"));
            self.can
        }

        fn release(&mut self, at: (f64, f64)) {
            self.asked.borrow_mut().push(format!("show {at:?}"));
        }
    }

    fn drag(can: bool) -> (LookDrag, Rc<RefCell<Vec<String>>>) {
        let asked = Rc::new(RefCell::new(Vec::new()));
        let host = Asked {
            can,
            asked: Rc::clone(&asked),
        };
        (LookDrag::new(Box::new(host)), asked)
    }

    #[test]
    fn a_right_drag_over_the_world_view_holds_the_pointer_and_shows_it_where_it_began() {
        let (mut d, asked) = drag(true);
        assert_eq!(
            d.right_button(true, false, (300.0, 200.0)),
            Some((300.0, 200.0))
        );
        assert!(d.holds());
        assert_eq!(d.right_button(false, false, (310.0, 260.0)), None);
        assert!(!d.holds());
        assert_eq!(
            *asked.borrow(),
            ["hold (300.0, 200.0)", "show (300.0, 200.0)"]
        );
    }

    #[test]
    fn a_right_drag_begun_over_the_panels_turns_nothing_and_holds_nothing() {
        let (mut d, asked) = drag(true);
        assert_eq!(d.right_button(true, true, (1500.0, 200.0)), None);
        assert_eq!(d.right_button(false, false, (900.0, 300.0)), None);
        assert!(asked.borrow().is_empty());
    }

    #[test]
    fn a_window_that_cannot_hold_the_pointer_still_turns_and_shows_nothing_back() {
        let (mut d, asked) = drag(false);
        assert_eq!(
            d.right_button(true, false, (300.0, 200.0)),
            Some((300.0, 200.0))
        );
        assert!(!d.holds());
        d.end();
        assert_eq!(*asked.borrow(), ["hold (300.0, 200.0)"]);
    }

    #[test]
    fn a_turn_ended_another_way_shows_the_pointer_once() {
        let (mut d, asked) = drag(true);
        d.right_button(true, false, (40.0, 50.0));
        d.end();
        d.end();
        assert_eq!(d.right_button(false, false, (60.0, 70.0)), None);
        assert_eq!(*asked.borrow(), ["hold (40.0, 50.0)", "show (40.0, 50.0)"]);
    }
}
