//! Drag bars: the strip a window is moved by. Held down with the left button while topmost, it
//! moves its parent with the pointer until let go.

use crate::system::{Handle, Ui};
use crate::token::{attr, state, Token};
use crate::types::button_id;

impl Ui {
    /// A new drag bar follows its flags (`allow_updating` on).
    // D2-11490, D2-11491
    pub(crate) fn dragbar_created(&mut self, h: Handle) {
        self.set_attr(h, attr::ALLOW_UPDATING, crate::Value::Bool(true));
    }

    /// The left button down on the topmost drag bar of a parent that is neither moving nor
    /// resizing starts moving the parent, unless the bar is disabled.
    // D2-11484
    pub(crate) fn dragbar_mouse_down(
        &mut self,
        h: Handle,
        x: i32,
        y: i32,
        b: u32,
        handled: &mut bool,
    ) {
        self.element_mouse_down(h, x, y, b, handled);
        let Some(n) = self.node(h) else { return };
        if n.moving || n.resizing || !n.over_top || b != button_id::LEFT {
            return;
        }
        let Some(p) = n.parent else { return };
        if self.node(p).is_some_and(|pn| pn.moving || pn.resizing) {
            return;
        }
        if n.bool_attr(attr::DISABLED) {
            return;
        }
        if let Some(n) = self.node_mut(h) {
            n.held = true;
        }
        self.start_movement(p);
        self.set_state(h, state::PRESSED);
        self.flush_pending();
    }

    /// The left button let go after it started a move: the bar back to normal, the move ends.
    // D2-11485
    pub(crate) fn dragbar_mouse_up(
        &mut self,
        h: Handle,
        x: i32,
        y: i32,
        b: u32,
        handled: &mut bool,
    ) {
        self.element_mouse_up(h, x, y, b, handled);
        if !self.node(h).is_some_and(|n| n.held) || b != button_id::LEFT {
            return;
        }
        self.set_state(h, state::NORMAL);
        self.flush_pending();
        if let Some(n) = self.node_mut(h) {
            n.held = false;
        }
        if let Some(p) = self.node(h).and_then(|n| n.parent) {
            self.stop_movement(p);
        }
    }

    /// A disabled bar that hides when disabled is hidden; otherwise it is shown.
    // D2-11486
    pub(crate) fn dragbar_update_layout(&mut self, h: Handle) {
        let Some(n) = self.node(h) else { return };
        if !n.bool_attr(attr::ALLOW_UPDATING) {
            return;
        }
        let hide = n.bool_attr(attr::DISABLED) && n.bool_attr(attr::HIDE_WHEN_DISABLED);
        let shown = self.is_visible(h);
        if !hide {
            if shown {
                self.set_visible(h, true);
            }
        } else if shown {
            self.set_visible(h, false);
        }
    }

    // D2-11489
    pub(crate) fn dragbar_bool_changed(&mut self, h: Handle, a: Token) {
        if a == attr::DISABLED || a == attr::HIDE_WHEN_DISABLED {
            self.dragbar_update_layout(h);
        }
    }
}
