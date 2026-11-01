//! Buttons: a look per state (normal, rolled over, pressed, highlighted, ghosted), a click when
//! released on, a highlight that a boolean button turns on and off, and a hot button's repeated
//! clicks while held.

use crate::manager::global;
use crate::system::{Handle, Ui, Value};
use crate::token::{attr, msg, state, Token};
use crate::types::button_id;

/// What a button keeps between events.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ButtonState {
    /// The left button went down on it and has not come up.
    pub pressed: bool,
    /// Its hot clicks are under way, and when the next is due.
    pub hot: bool,
    pub next_hot: f64,
}

impl Ui {
    /// A new button's hot-click intervals, where its records give none: 0.3 seconds to the first
    /// repeat, 0.1 between them (the original sets them before the records' values apply). A
    /// ghosted, highlighted or rollover flag its records set brought its state up to date as it
    /// was applied (the button was not ready to hear it then), so a button made in its plain
    /// state shows its normal one at once, not at the pointer's first touch.
    // D2-11467, D2-11468, D2-11480
    pub(crate) fn button_created(&mut self, h: Handle) {
        self.default_attr(h, attr::HOT_CLICK_FIRST_INTERVAL, Value::Float(0.3));
        self.default_attr(h, attr::HOT_CLICK_INTERVAL, Value::Float(0.1));
        let flagged = self.node(h).is_some_and(|n| {
            [attr::GHOSTED, attr::HIGHLIGHTED, attr::ROLLOVER]
                .iter()
                .any(|a| n.attr(*a).is_some())
        });
        if flagged {
            self.update_button(h);
        }
    }

    /// The clock: a held hot button clicks again when its next click is due (the next one an
    /// interval after it); while the pointer is off it, the due time waits.
    // D2-11469
    fn button_global_loop(&mut self, h: Handle) {
        let now = self.now();
        let Some(n) = self.node(h) else { return };
        if !n.button.hot {
            return;
        }
        if !n.over_top {
            if n.button.next_hot < now {
                if let Some(n) = self.node_mut(h) {
                    n.button.next_hot = now;
                }
            }
            return;
        }
        if n.button.next_hot <= now {
            self.broadcast(h, msg::HOT_CLICK, 0);
            let interval = self
                .node(h)
                .and_then(|n| n.float_attr(attr::HOT_CLICK_INTERVAL))
                .unwrap_or(0.0);
            if let Some(n) = self.node_mut(h) {
                n.button.next_hot += f64::from(interval);
            }
        }
    }

    // D2-11470
    pub(crate) fn button_listen(&mut self, h: Handle, what: u32, param: u32) {
        self.element_listen(h, what, param);
        if what == global::TIME {
            self.button_global_loop(h);
        }
    }

    /// A button's state from its flags and the pointer: ghosted, else (a radio button already
    /// pressed stays so) pressed or rolled over while the pointer is on it or it has the keyboard,
    /// else highlighted or normal.
    // D2-11471
    pub fn update_button(&mut self, h: Handle) {
        let Some(n) = self.node(h) else { return };
        let s = if n.bool_attr(attr::GHOSTED) {
            state::GHOSTED
        } else {
            let lit = n.bool_attr(attr::HIGHLIGHTED);
            if n.bool_attr(attr::RADIO_BUTTON) && n.state == state::NORMAL_PRESSED {
                return;
            }
            if (n.over_top || n.active) && n.button.pressed {
                if lit {
                    state::HIGHLIGHT_PRESSED
                } else {
                    state::NORMAL_PRESSED
                }
            } else if (n.over_top || n.active) && n.bool_attr(attr::ROLLOVER) {
                if lit {
                    state::HIGHLIGHT_ROLLOVER
                } else {
                    state::NORMAL_ROLLOVER
                }
            } else if lit {
                state::HIGHLIGHT
            } else {
                state::NORMAL
            }
        };
        if n.state != s {
            self.set_state(h, s);
            self.flush_pending();
        }
    }

    /// Entering a highlighted state turns the highlight on.
    // D2-11472
    pub(crate) fn button_set_state(&mut self, h: Handle, s: Token) -> bool {
        if (s == state::HIGHLIGHT
            || s == state::HIGHLIGHT_ROLLOVER
            || s == state::HIGHLIGHT_PRESSED)
            && !self.node(h).is_some_and(|n| n.bool_attr(attr::HIGHLIGHTED))
        {
            self.set_attr(h, attr::HIGHLIGHTED, Value::Bool(true));
        }
        self.set_state_base(h, s)
    }

    /// A ghosted button never takes the keyboard.
    // D2-11473
    pub(crate) fn button_is_activatable(&self, h: Handle) -> bool {
        if self.node(h).is_some_and(|n| n.bool_attr(attr::GHOSTED)) {
            return false;
        }
        self.element_is_activatable(h)
    }

    // D2-11474
    pub(crate) fn button_activate(&mut self, h: Handle) -> bool {
        if !self.element_activate(h) {
            return false;
        }
        self.update_button(h);
        true
    }

    // D2-11475
    pub(crate) fn button_deactivate(&mut self, h: Handle) -> bool {
        if !self.element_deactivate(h) {
            return false;
        }
        self.update_button(h);
        true
    }

    /// The use key pressed on the active button: as a press of the left button on it.
    // D2-11476
    pub(crate) fn button_start_use_active(&mut self, h: Handle) -> bool {
        if let Some(n) = self.node_mut(h) {
            n.button.pressed = true;
        }
        self.broadcast(h, msg::MOUSE_PRESS, 0);
        if self.node(h).is_some_and(|n| n.bool_attr(attr::HOT_BUTTON)) {
            self.broadcast(h, msg::HOT_CLICK, 0);
            let first = self
                .node(h)
                .and_then(|n| n.float_attr(attr::HOT_CLICK_FIRST_INTERVAL))
                .unwrap_or(0.0);
            let now = self.now();
            if let Some(n) = self.node_mut(h) {
                n.button.hot = true;
                n.button.next_hot = f64::from(first) + now;
            }
        }
        self.update_button(h);
        true
    }

    /// The use key let go: a click while the button still has the keyboard, and a boolean
    /// button's highlight turned over.
    // D2-11477
    pub(crate) fn button_end_use_active(&mut self, h: Handle) -> bool {
        let Some(n) = self.node_mut(h) else {
            return false;
        };
        n.button.pressed = false;
        n.button.hot = false;
        let what = if n.active {
            msg::BUTTON_RELEASE_ON
        } else {
            msg::BUTTON_RELEASE_OFF
        };
        self.broadcast(h, what, 0);
        if self
            .node(h)
            .is_some_and(|n| n.bool_attr(attr::BOOLEAN_BUTTON))
        {
            let lit = self.node(h).is_some_and(|n| n.bool_attr(attr::HIGHLIGHTED));
            self.set_attr(h, attr::HIGHLIGHTED, Value::Bool(!lit));
        }
        self.update_button(h);
        true
    }

    /// After a yes-or-no attribute changed: ghosting changes whether it takes the keyboard and
    /// its look; the highlight and roll-over change its look; the highlight is announced.
    // D2-11479
    pub(crate) fn button_bool_changed(&mut self, h: Handle, a: Token, v: bool) {
        if a == attr::GHOSTED {
            self.update_activatable(h);
            self.update_button(h);
        }
        if a == attr::HIGHLIGHTED || a == attr::ROLLOVER {
            self.update_button(h);
        }
        if a == attr::HIGHLIGHTED {
            self.broadcast(h, msg::HIGHLIGHT_STATE_CHANGE, u32::from(v));
        }
    }

    /// A press: the plain element's; then, unless the element is moving or resizing, the press
    /// stops here (unless the button passes presses through), the left button shows it pressed,
    /// and an unghosted button (or one that speaks when ghosted) says it was pressed and starts
    /// a hot button's clicks.
    // D2-11481
    pub(crate) fn button_mouse_down(
        &mut self,
        h: Handle,
        x: i32,
        y: i32,
        b: u32,
        handled: &mut bool,
    ) {
        self.element_mouse_down(h, x, y, b, handled);
        let Some(n) = self.node(h) else { return };
        if n.moving || n.resizing {
            return;
        }
        if !n.bool_attr(attr::MOUSE_EVENT_PASSTHROUGH) {
            *handled = true;
        }
        let left = b == button_id::LEFT || b == button_id::DOUBLE_LEFT;
        if left {
            if let Some(n) = self.node_mut(h) {
                n.button.pressed = true;
            }
            self.update_button(h);
        }
        let Some(n) = self.node(h) else { return };
        if !n.bool_attr(attr::GHOSTED) || n.bool_attr(attr::BROADCAST_WHEN_GHOSTED) {
            self.broadcast(h, msg::BUTTON_PRESS, b);
            if left && self.node(h).is_some_and(|n| n.bool_attr(attr::HOT_BUTTON)) {
                self.broadcast(h, msg::HOT_CLICK, b);
                let first = self
                    .node(h)
                    .and_then(|n| n.float_attr(attr::HOT_CLICK_FIRST_INTERVAL))
                    .unwrap_or(0.0);
                let now = self.now();
                if let Some(n) = self.node_mut(h) {
                    n.button.hot = true;
                    n.button.next_hot = f64::from(first) + now;
                }
            }
        }
    }

    /// A release: the plain element's; the release stops here unless passed through; for the
    /// left button the press ends, and an unghosted boolean button released on itself (or while
    /// it has the keyboard) turns its highlight over; an unghosted button (or one that speaks
    /// when ghosted) says whether it was released on itself (a click) or off it.
    // D2-11482
    pub(crate) fn button_mouse_up(
        &mut self,
        h: Handle,
        x: i32,
        y: i32,
        b: u32,
        handled: &mut bool,
    ) {
        self.element_mouse_up(h, x, y, b, handled);
        let Some(n) = self.node(h) else { return };
        if !n.bool_attr(attr::MOUSE_EVENT_PASSTHROUGH) {
            *handled = true;
        }
        let ghosted = n.bool_attr(attr::GHOSTED);
        let speaks = !ghosted || n.bool_attr(attr::BROADCAST_WHEN_GHOSTED);
        if b == button_id::LEFT || b == button_id::DOUBLE_LEFT {
            let (over, active) = (n.over, n.active);
            if let Some(n) = self.node_mut(h) {
                n.button.pressed = false;
                n.button.hot = false;
            }
            if (over || active)
                && !ghosted
                && self
                    .node(h)
                    .is_some_and(|n| n.bool_attr(attr::BOOLEAN_BUTTON))
            {
                let lit = self.node(h).is_some_and(|n| n.bool_attr(attr::HIGHLIGHTED));
                self.set_attr(h, attr::HIGHLIGHTED, Value::Bool(!lit));
            }
            self.update_button(h);
        }
        if speaks {
            let what = if self.node(h).is_some_and(|n| n.over) {
                msg::BUTTON_RELEASE_ON
            } else {
                msg::BUTTON_RELEASE_OFF
            };
            self.broadcast(h, what, b);
        }
    }

    // D2-11483
    pub(crate) fn button_mouse_over_top(&mut self, h: Handle, on: bool, handled: &mut bool) {
        self.element_mouse_over_top(h, on, handled);
        self.update_button(h);
    }
}
