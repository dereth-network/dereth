//! Dragging items: an element held down with the left button and moved away (or held a while)
//! is dragged out of its container as a copy that follows the pointer; let go, the copy is
//! offered to the element under the pointer, and the container hears whether it was taken.
//!
//! The copy carries the original's attributes; once dropped it is kept, hidden, until the next
//! drag, for the listeners that read their mail after the drop ([`Ui::dropped`]).
//!
//! A container may refuse to let an item out (`disallow_drag_out`, or its own check) and an
//! element may refuse to take one in (not a `container`, `disallow_drag_in`, or its own check);
//! an element that is not `dragable` passes the drag to its parent.

use crate::system::{Handle, Ui};
use crate::token::{attr, Token};

/// A check a panel gives a container: the item and the container; whether it may go out or come
/// in.
pub type DragCheck = fn(&Ui, Handle, Handle) -> bool;

/// How long the left button must be held, in seconds, or how far (more than this squared, in
/// either direction) the pointer must move, before a press becomes a drag.
const DRAG_DELAY: f64 = 0.3;
const DRAG_DISTANCE_SQUARED: u32 = 15;

/// What an element says when something not dragable is dragged.
const NOT_DRAGABLE: Token = Token::of("not_dragable");
/// What a container says when an item starts going out, and when one comes in or comes back
/// (the parameter: 1 taken, 0 not).
const START_DRAG_ITEM: Token = Token::of("start_drag_item");
const STOP_DRAG_ITEM: Token = Token::of("stop_drag_item");

impl Ui {
    /// The item being dragged.
    #[must_use]
    pub fn dragged(&self) -> Option<Handle> {
        self.mgr.drag.filter(|h| self.node(*h).is_some())
    }

    /// The container an item is being dragged out of.
    #[must_use]
    pub fn drag_owner(&self) -> Option<Handle> {
        self.mgr.drag_owner.filter(|h| self.node(*h).is_some())
    }

    /// The item last dropped (the copy that went with the pointer, with the original's
    /// attributes), until the next drag starts: what a container's `stop_drag_item` is about.
    #[must_use]
    pub fn dropped(&self) -> Option<Handle> {
        self.mgr.dropped.filter(|h| self.node(*h).is_some())
    }

    /// Start dragging `h` now, as a panel does for an element it drags itself (the money bag's
    /// coins, an item picked up from the world): when the left button has been held long enough
    /// or moved far enough, as for any drag. Returns whether a drag started.
    pub fn start_drag(&mut self, h: Handle) -> bool {
        self.start_drag_and_drop(h, 0, 0)
    }

    /// Give a container its checks: whether an item may be dragged out of it, and in.
    pub fn set_drag_checks(&mut self, h: Handle, out: Option<DragCheck>, into: Option<DragCheck>) {
        if let Some(n) = self.node_mut(h) {
            n.drag_out_check = out;
            n.drop_in_check = into;
        }
    }

    /// While nothing is dragged or resized and the pointer is over an element, see whether a
    /// press there has become a drag (the item keeping where it stands from the press).
    // D2-11201
    pub(crate) fn check_drag_and_drop(&mut self) {
        if self.mgr.drag.is_some() || self.mgr.resizing.is_some() {
            return;
        }
        let Some(top) = self.top() else { return };
        let Some((x0, y0)) = self.screen_origin(top) else {
            return;
        };
        let (dx, dy) = (x0 - self.mgr.press_x, y0 - self.mgr.press_y);
        self.start_drag_and_drop(top, dx, dy);
    }

    /// A press on `h` becomes a drag once the left button has been held long enough or the
    /// pointer moved far enough from where it went down: an element that is not dragable says
    /// so and passes the drag to its parent; a dragable one is dragged out of its parent, when
    /// the parent lets it go. The copy is shown in a picture of the item's own size, put at the
    /// pointer moved by `(dx, dy)` held within that size. Returns whether a drag started.
    // D2-11211
    pub(crate) fn start_drag_and_drop(&mut self, h: Handle, dx: i32, dy: i32) -> bool {
        if self.node(h).is_none() || !self.mgr.left_down {
            return false;
        }
        let (px, py) = (self.mgr.press_x, self.mgr.press_y);
        let (w, hh) = self.display();
        if px < 0 || py < 0 || px > w || py > hh {
            return false;
        }
        let held = self.mgr.now - self.mgr.last_press;
        let (x, y) = self.pointer();
        let (mx, my) = (x - px, y - py);
        let far =
            |d: i32| d.unsigned_abs().saturating_mul(d.unsigned_abs()) > DRAG_DISTANCE_SQUARED;
        if !(held >= DRAG_DELAY || far(mx) || far(my)) {
            return false;
        }
        let Some(n) = self.node(h) else { return false };
        let parent = n.parent;
        let size = (n.rect.width().max(0), n.rect.height().max(0));
        if !n.bool_attr(attr::DRAGABLE) {
            self.broadcast(h, NOT_DRAGABLE, 0);
            return match parent {
                Some(p) => self.start_drag_and_drop(p, dx, dy),
                None => false,
            };
        }
        let Some(container) = parent else {
            return false;
        };
        let mut item = h;
        if !self.drag_item(container, &mut item) {
            return false;
        }
        // The copy of the last drop goes now that another drag starts.
        if let Some(old) = self.mgr.dropped.take() {
            self.remove(old);
        }
        // The copy follows the pointer in a picture of the item's size, whatever size the copy
        // takes on its own (placed against the screen, a copy whose edges follow its parent's
        // grows with a screen larger than its layout's).
        self.mgr.drag = Some(item);
        self.mgr.drag_owner = Some(container);
        self.mgr.drag_dx = dx.clamp(0, size.0);
        self.mgr.drag_dy = dy.clamp(0, size.1);
        self.mgr.drag_size = size;
        self.drag_follows_pointer();
        true
    }

    /// Where the dragged copy is shown: the pointer moved by the copy's offset, in the size of
    /// the item it was copied from.
    #[must_use]
    pub fn drag_area(&self) -> Option<crate::Rect> {
        let d = self.node(self.dragged()?)?;
        let (w, h) = self.mgr.drag_size;
        Some(crate::Rect::sized(d.rect.x0, d.rect.y0, w, h))
    }

    /// The dragged copy follows the pointer.
    // D2-11206
    pub(crate) fn drag_follows_pointer(&mut self) {
        if let Some(d) = self.dragged() {
            let (x, y) = self.pointer();
            let (x, y) = (x + self.mgr.drag_dx, y + self.mgr.drag_dy);
            if let Some(n) = self.node_mut(d) {
                n.rect = crate::Rect::sized(x, y, n.rect.width(), n.rect.height());
                n.sized = true;
            }
        }
    }

    /// End a drag where the pointer is: the element under it is offered the item, and the
    /// container hears whether it was taken.
    // D2-11212
    pub fn stop_drag_and_drop(&mut self) {
        let Some(item) = self.mgr.drag else { return };
        let caught = self.top().is_some_and(|t| self.catch_dropped_item(t, item));
        if let Some(owner) = self.mgr.drag_owner {
            self.drag_and_drop_complete(owner, item, caught);
        }
        self.clear_drag_and_drop();
    }

    /// Drop the drag: the copy goes.
    // D2-11202
    pub(crate) fn clear_drag_and_drop(&mut self) {
        self.mgr.drag_owner = None;
        if let Some(d) = self.mgr.drag.take() {
            // Kept for the listeners that read their mail after the drop, until the next drag.
            if let Some(old) = self.mgr.dropped.replace(d) {
                self.remove(old);
            }
            self.set_visible(d, false);
        }
    }

    /// A container lets `item` be dragged out of it, unless it keeps its items or its check
    /// refuses: the item it keeps is the original; a copy in the same state and with the same
    /// pictures goes with the pointer (`item` becomes the copy), and the container says an item
    /// started going out.
    // D2-11351
    pub(crate) fn drag_item(&mut self, container: Handle, item: &mut Handle) -> bool {
        let Some(n) = self.node(container) else {
            return false;
        };
        if n.bool_attr(attr::DISALLOW_DRAG_OUT) {
            return false;
        }
        if let Some(check) = n.drag_out_check {
            if !check(self, *item, container) {
                return false;
            }
        }
        let original = *item;
        let Some(copy) = self.copy_element(original) else {
            return false;
        };
        if let Some(n) = self.node_mut(container) {
            n.dragged_original = Some(original);
        }
        *item = copy;
        self.initialized(copy);
        let s = self.node(original).map_or(Token(0), |n| n.state);
        self.set_state(copy, s);
        self.flush_pending();
        // The copy carries what the original was given (the item it stands for, where it came
        // from), then its pictures.
        let _ = self.match_attributes(original, copy);
        let _ = self.match_graphics(copy, original);
        self.broadcast(container, START_DRAG_ITEM, 0);
        true
    }

    /// A dropped item offered to an element: a container that takes items in (and whose check
    /// agrees) takes it and says an item came in; anything else passes it to its parent.
    // D2-11352
    pub(crate) fn element_catch_dropped_item(&mut self, h: Handle, item: Handle) -> bool {
        if h == item {
            return false;
        }
        let Some(n) = self.node(h) else { return false };
        if n.bool_attr(attr::CONTAINER) && !n.bool_attr(attr::DISALLOW_DRAG_IN) {
            if let Some(check) = n.drop_in_check {
                if !check(self, item, h) {
                    return false;
                }
            }
            self.broadcast(h, STOP_DRAG_ITEM, 1);
            return true;
        }
        match n.parent {
            Some(p) => self.catch_dropped_item(p, item),
            None => false,
        }
    }

    /// A dropped item offered to an element, as its kind takes it.
    pub(crate) fn catch_dropped_item(&mut self, h: Handle, item: Handle) -> bool {
        if self
            .node(h)
            .is_some_and(|n| n.kind == crate::token::kind::FIELD)
        {
            return self.field_catch_dropped_item(h, item);
        }
        self.element_catch_dropped_item(h, item)
    }

    /// Whether an element would take a dropped item, as its own check says (with none, it would).
    pub(crate) fn would_catch(&self, h: Handle, item: Handle) -> bool {
        self.node(h)
            .and_then(|n| n.drop_in_check)
            .is_none_or(|check| check(self, item, h))
    }

    /// The container an item was dragged out of hears how it went: an item nobody took comes
    /// back (it says so).
    // D2-11309
    pub(crate) fn drag_and_drop_complete(&mut self, owner: Handle, item: Handle, caught: bool) {
        if self.node(item).is_none() {
            return;
        }
        if !caught {
            self.broadcast(owner, STOP_DRAG_ITEM, 0);
        }
    }
}
