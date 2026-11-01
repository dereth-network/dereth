//! Fields: plain elements, which as containers show whether they would take the item dragged over
//! them.

use crate::system::{Handle, Ui};
use crate::token::{attr, state};

impl Ui {
    /// The pointer over a container while an item is dragged: a container that shows it takes
    /// the accepting or refusing state (remembering the state it had); leaving puts the
    /// remembered state back.
    // D2-11494
    pub(crate) fn field_mouse_over_top(&mut self, h: Handle, on: bool, handled: &mut bool) {
        self.element_mouse_over_top(h, on, handled);
        if !on {
            if let Some(saved) = self.node(h).and_then(|n| n.saved_state) {
                self.set_state(h, saved);
                self.flush_pending();
                if let Some(n) = self.node_mut(h) {
                    n.saved_state = None;
                }
            }
            return;
        }
        let Some(dragged) = self.dragged() else {
            return;
        };
        let Some(n) = self.node(h) else { return };
        if !n.bool_attr(attr::CONTAINER) || !n.bool_attr(attr::DRAG_ROLLOVER) {
            return;
        }
        let refuses = n.bool_attr(attr::DISALLOW_DRAG_IN);
        let s = if self.would_catch(h, dragged) && !refuses {
            state::DRAG_ROLLOVER_ACCEPT
        } else {
            state::DRAG_ROLLOVER_REJECT
        };
        let now = n.state;
        if s != now {
            if n.saved_state.is_none() {
                if let Some(n) = self.node_mut(h) {
                    n.saved_state = Some(now);
                }
            }
            self.set_state(h, s);
            self.flush_pending();
        }
    }

    /// A dropped item caught: the remembered state back first.
    // D2-11495
    pub(crate) fn field_catch_dropped_item(&mut self, h: Handle, item: Handle) -> bool {
        if let Some(saved) = self.node(h).and_then(|n| n.saved_state) {
            self.set_state(h, saved);
            self.flush_pending();
            if let Some(n) = self.node_mut(h) {
                n.saved_state = None;
            }
        }
        self.element_catch_dropped_item(h, item)
    }
}
