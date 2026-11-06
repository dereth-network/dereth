//! What every element does with the pointer and the keyboard: the messages it sends when pressed,
//! released, moved over and rolled onto; moving and resizing with the pointer; taking and giving
//! up the keyboard; its own cursor; its place moved and resized.
//!
//! The kinds ([`crate::types`]) build on these; the manager ([`crate::manager`]) and the regions
//! ([`crate::region`]) decide which elements hear what.

use crate::manager::{global, Cursor};
use crate::system::{Handle, Ui};
use crate::token::{attr, msg, Token};
use crate::types::resizebar::Border;
use crate::Rect;

impl Ui {
    /// An element's root: the top-level element it is in (itself, for one).
    // D2-11290
    #[must_use]
    pub fn root_of(&self, h: Handle) -> Handle {
        let mut at = h;
        while let Some(p) = self.node(at).and_then(|n| n.parent) {
            at = p;
        }
        at
    }

    /// What an element says, to whoever listens: the panels (every message) and the elements
    /// listening to its name.
    // D2-11324
    pub fn broadcast(&mut self, h: Handle, what: Token, param: u32) {
        let Some(n) = self.node(h) else { return };
        let element = n.id;
        self.outbox.push(crate::system::Message {
            from: h,
            element,
            what,
            param,
        });
        self.deliver(h, element, what, param);
    }

    /// A press on a plain element: "pressed on the topmost" when it is the topmost, "pressed"
    /// in any case; a press on a root's own child stops there unless it passes presses through.
    // D2-11345
    pub(crate) fn element_mouse_down(
        &mut self,
        h: Handle,
        _x: i32,
        _y: i32,
        b: u32,
        handled: &mut bool,
    ) {
        if self.node(h).is_some_and(|n| n.over_top) {
            self.broadcast(h, msg::MOUSE_PRESS_TOP, b);
        }
        self.broadcast(h, msg::MOUSE_PRESS, b);
        let Some(n) = self.node(h) else { return };
        if let Some(p) = n.parent {
            if self.node(p).is_some_and(|pn| pn.parent.is_none())
                && !n.bool_attr(attr::MOUSE_EVENT_PASSTHROUGH)
            {
                *handled = true;
            }
        }
    }

    /// A release on a plain element: "released on the topmost" when it is the topmost, and
    /// "released".
    // D2-11292
    pub(crate) fn element_mouse_up(
        &mut self,
        h: Handle,
        _x: i32,
        _y: i32,
        b: u32,
        _handled: &mut bool,
    ) {
        if self.node(h).is_some_and(|n| n.over_top) {
            self.broadcast(h, msg::MOUSE_RELEASE_TOP, b);
        }
        self.broadcast(h, msg::MOUSE_RELEASE, b);
    }

    /// The pointer moved on a plain element: "moved on the topmost" when it is, and "moved".
    // D2-11291
    pub(crate) fn element_mouse_move(&mut self, h: Handle, _x: i32, _y: i32, _handled: &mut bool) {
        if self.node(h).is_some_and(|n| n.over_top) {
            self.broadcast(h, msg::MOUSE_MOVE_TOP, 0);
        }
        self.broadcast(h, msg::MOUSE_MOVE, 0);
    }

    /// The pointer came over the element or left it.
    // D2-11304, D2-11179
    pub(crate) fn element_mouse_over(&mut self, h: Handle, on: bool, _handled: &mut bool) {
        if let Some(n) = self.node_mut(h) {
            n.over = on;
        }
        self.broadcast(h, msg::MOUSE_OVER, u32::from(on));
    }

    /// The element became or stopped being the topmost under the pointer; becoming it shows the
    /// element's cursor, when it has one.
    // D2-11305, D2-11180
    pub(crate) fn element_mouse_over_top(&mut self, h: Handle, on: bool, _handled: &mut bool) {
        if let Some(n) = self.node_mut(h) {
            n.over_top = on;
        }
        if on {
            if let Some(c) = self.node(h).and_then(|n| n.cursor) {
                if c.picture != 0 {
                    self.set_cursor(c, false);
                }
            }
        }
        self.broadcast(h, msg::MOUSE_OVER_TOP, u32::from(on));
    }

    /// A global message to a plain element: the pointer moving while the element resizes or
    /// moves with it; the clock, which runs its transitions and its media on.
    // D2-11390
    pub(crate) fn element_listen(&mut self, h: Handle, what: u32, _param: u32) {
        if what == global::TIME {
            self.update_transitions(h);
            self.update_media(h);
            return;
        }
        if what == global::MOUSE_MOVE {
            let Some(n) = self.node(h) else { return };
            if n.resizing {
                self.mouse_resize_element(h);
            } else if n.moving {
                self.mouse_move_element(h);
            }
        }
    }

    /// An element hears what another element it listens to said.
    pub(crate) fn hear(&mut self, h: Handle, from: Handle, what: Token, param: u32) {
        use crate::token::kind;
        match self.node(h).map(|n| n.kind) {
            Some(k) if k == kind::SCROLLBAR => self.scrollbar_hear(h, from, what, param),
            Some(k) if k == kind::LISTBOX || k == kind::LISTBOX_MULTI_SIZE => {
                self.listbox_hear(h, from, what, param);
            }
            Some(k) if k == kind::TREE => self.tree_hear(h, from, what, param),
            Some(k) if k == kind::MENU => self.menu_hear(h, from, what),
            Some(k) if k == kind::PANEL => self.panel_hear(h, from, what),
            Some(k) if k == kind::TEXT => self.text_hear(h, from, what, param),
            _ => {}
        }
    }

    // -----------------------------------------------------------------------------------------
    // Moving and resizing with the pointer
    // -----------------------------------------------------------------------------------------

    /// Start moving with the pointer (unless already moving or resizing): the pointer's place
    /// remembered, and the pointer's moves heard.
    // D2-11293
    pub fn start_movement(&mut self, h: Handle) {
        let (x, y) = self.pointer();
        let Some(n) = self.node_mut(h) else { return };
        if n.resizing || n.moving {
            return;
        }
        n.mouse_offset = (x, y);
        n.moving = true;
        self.register_for_global_message(h, global::MOUSE_MOVE);
    }

    // D2-11294
    pub fn stop_movement(&mut self, h: Handle) {
        if let Some(n) = self.node_mut(h) {
            n.moving = false;
        }
        self.unregister_for_global_message(h, global::MOUSE_MOVE);
    }

    /// Move by as much as the pointer moved since last time; an element kept on the screen
    /// stops at its edges.
    // D2-11346
    fn mouse_move_element(&mut self, h: Handle) {
        let (px, py) = self.pointer();
        let Some(n) = self.node(h) else { return };
        let (ox, oy) = n.mouse_offset;
        let r = n.rect;
        let mut x = r.x0 + (px - ox);
        let mut y = r.y0 + (py - oy);
        if n.bool_attr(attr::DISPLAY_CONSTRAINED) {
            let (dw, dh) = self.display();
            x = x.max(0).min(dw - 1 + (r.x0 - r.x1));
            y = y.max(0).min(dh - 1 + (r.y0 - r.y1));
        }
        self.move_to(h, x, y);
        if let Some(n) = self.node_mut(h) {
            n.mouse_offset = (px, py);
        }
    }

    /// Start resizing by a border with the pointer (unless already moving or resizing); with a
    /// resize line, a line shows the new size until the resize ends.
    // D2-11362
    pub fn start_resizing(&mut self, h: Handle, border: Border) {
        let (x, y) = self.pointer();
        let Some(n) = self.node_mut(h) else { return };
        if n.resizing || n.moving {
            return;
        }
        n.mouse_offset = (x, y);
        if border == Border::None {
            return;
        }
        n.border = border;
        n.resizing = true;
        n.resize_line = n.bool_attr(attr::RESIZE_LINE).then_some(n.rect);
        self.register_for_global_message(h, global::MOUSE_MOVE);
        self.mgr.resizing = Some(h);
        self.touch();
    }

    /// Where a resize line shows on the screen, while an element under this top-level element is
    /// being resized with one.
    #[must_use]
    pub fn resize_outline(&self, root: Handle) -> Option<Rect> {
        let h = self.mgr.resizing?;
        let line = self.node(h)?.resize_line?;
        if self.root_of(h) != root {
            return None;
        }
        let parent = self
            .node(h)?
            .parent
            .and_then(|p| self.screen_rect(p))
            .unwrap_or(Rect::new(0, 0, 0, 0));
        Some(line.offset(parent.x0, parent.y0))
    }

    /// Stop resizing; with a resize line, the element takes the line's place and size now.
    // D2-11363
    pub fn stop_resizing(&mut self, h: Handle) {
        let Some(n) = self.node_mut(h) else { return };
        n.resizing = false;
        let line = n.resize_line.take();
        self.unregister_for_global_message(h, global::MOUSE_MOVE);
        self.mgr.resizing = None;
        self.touch();
        if let Some(r) = line {
            self.move_to(h, r.x0, r.y0);
            self.resize_to(h, r.width(), r.height());
            self.move_to(h, r.x0, r.y0);
        }
    }

    /// Resize by as much as the pointer moved since last time, by the border held: each edge it
    /// moves stays within the element's least and greatest width and height, and each edge within
    /// the screen; a corner moves its two edges.
    // D2-11364
    fn mouse_resize_element(&mut self, h: Handle) {
        let (px, py) = self.pointer();
        let Some(n) = self.node(h) else { return };
        if n.border == Border::None {
            return;
        }
        let (dx, dy) = (px - n.mouse_offset.0, py - n.mouse_offset.1);
        let from = n.resize_line.unwrap_or(n.rect);
        let (x0, y0, x1, y1) = (from.x0, from.y0, from.x1, from.y1);
        let (mut l, mut t, mut r, mut b) = (x0, y0, x1, y1);
        let (min_w, max_w) = (n.int_attr(attr::MIN_WIDTH), n.int_attr(attr::MAX_WIDTH));
        let (min_h, max_h) = (n.int_attr(attr::MIN_HEIGHT), n.int_attr(attr::MAX_HEIGHT));
        let border = n.border;
        let line = n.resize_line.is_some();
        let left = |l: &mut i32| {
            *l = x0 + dx;
            if let Some(m) = min_w {
                *l = (*l).min(x1 - m + 1);
            }
            if let Some(m) = max_w {
                *l = (*l).max(x1 - m + 1);
            }
        };
        let right = |r: &mut i32| {
            *r = x1 + dx;
            if let Some(m) = min_w {
                *r = (*r).max(m - 1 + x0);
            }
            if let Some(m) = max_w {
                *r = (*r).min(m - 1 + x0);
            }
        };
        let top = |t: &mut i32| {
            *t = y0 + dy;
            if let Some(m) = min_h {
                *t = (*t).min(y1 - m + 1);
            }
            if let Some(m) = max_h {
                *t = (*t).max(y1 - m + 1);
            }
        };
        let bottom = |b: &mut i32| {
            *b = y1 + dy;
            if let Some(m) = min_h {
                *b = (*b).max(m - 1 + y0);
            }
            if let Some(m) = max_h {
                *b = (*b).min(m - 1 + y0);
            }
        };
        match border {
            Border::UpperLeft => {
                top(&mut t);
                left(&mut l);
            }
            Border::Left => left(&mut l),
            Border::UpperRight => {
                right(&mut r);
                top(&mut t);
            }
            Border::Top => top(&mut t),
            Border::LowerRight => {
                bottom(&mut b);
                right(&mut r);
            }
            Border::Right => right(&mut r),
            Border::LowerLeft => {
                left(&mut l);
                bottom(&mut b);
            }
            Border::Bottom => bottom(&mut b),
            Border::None => {}
        }
        let (dw, dh) = self.display();
        let (l, r) = (l.max(0), r.min(dw - 1));
        let (t, b) = (t.max(0), b.min(dh - 1));
        if line {
            if let Some(n) = self.node_mut(h) {
                n.resize_line = Some(Rect::new(l, t, r, b));
            }
            self.touch();
        } else {
            self.move_to(h, l, t);
            self.resize_to(h, r - l + 1, b - t + 1);
        }
        if let Some(n) = self.node_mut(h) {
            n.mouse_offset = (px, py);
        }
    }

    // -----------------------------------------------------------------------------------------
    // Places
    // -----------------------------------------------------------------------------------------

    /// Move an element (a window the player drags, a place the game remembers): a root stays
    /// wholly on the screen. An element whose place is kept, or that announces its moves, says
    /// its place changed.
    // D2-11356, D2-11174
    pub fn move_to(&mut self, h: Handle, x: i32, y: i32) {
        let Some(n) = self.node(h) else { return };
        let (mut x, mut y) = (x, y);
        if n.parent.is_none() {
            let (dw, dh) = self.display();
            let r = n.rect;
            x = x.max(0).min(dw - 1 + (r.x0 - r.x1));
            y = y.max(0).min(dh - 1 + (r.y0 - r.y1));
        }
        if self.announces_place(h) {
            self.broadcast(h, msg::LOCATION_CHANGED, 0);
        }
        if self
            .node(h)
            .is_some_and(|n| n.sized && (n.rect.x0, n.rect.y0) == (x, y))
        {
            return;
        }
        if let Some(n) = self.node_mut(h) {
            let (w, hh) = (n.rect.width(), n.rect.height());
            n.rect = Rect::sized(x, y, w, hh);
            n.sized = true;
        }
    }

    /// Put a top-level element's picture at a place of the screen, with no word of its place and no
    /// keeping it on the screen: the words the game sets over the world run off the edges.
    // D2-10403
    pub fn move_picture(&mut self, h: Handle, x: i32, y: i32) {
        if self
            .node(h)
            .is_some_and(|n| n.sized && (n.rect.x0, n.rect.y0) == (x, y))
        {
            return;
        }
        if let Some(n) = self.node_mut(h) {
            let (w, hh) = (n.rect.width(), n.rect.height());
            n.rect = Rect::sized(x, y, w, hh);
            n.sized = true;
        }
    }

    /// Resize an element within its least and greatest width and height; its children follow
    /// their edges.
    // D2-11393, D2-11175
    pub fn resize_to(&mut self, h: Handle, w: i32, hh: i32) {
        let Some(n) = self.node(h) else { return };
        let (w, hh) = n.clamp_size(w, hh);
        if self.announces_place(h) {
            self.broadcast(h, msg::LOCATION_CHANGED, 0);
        }
        if let Some(n) = self.node_mut(h) {
            n.rect = Rect::sized(n.rect.x0, n.rect.y0, w, hh);
            n.sized = true;
        }
        let kids = self.node(h).map(|n| n.children.clone()).unwrap_or_default();
        for k in kids {
            self.place_tree(k);
        }
        self.kind_resized(h);
        if self.text_state(h).is_some() {
            self.text_resized(h);
        }
    }

    fn announces_place(&self, h: Handle) -> bool {
        self.node(h)
            .is_some_and(|n| n.bool_attr(attr::SAVE_LOC) || n.bool_attr(attr::BROADCAST_LOC_CHANGE))
    }

    // -----------------------------------------------------------------------------------------
    // The keyboard
    // -----------------------------------------------------------------------------------------

    /// Whether a plain element can take the keyboard: its `activatable` flag.
    // D2-11354
    pub(crate) fn element_is_activatable(&self, h: Handle) -> bool {
        self.node(h).is_some_and(|n| n.bool_attr(attr::ACTIVATABLE))
    }

    /// Its place in the order elements take the keyboard.
    // D2-11355
    pub(crate) fn activate_order(&self, h: Handle) -> i32 {
        self.node(h)
            .and_then(|n| n.int_attr(attr::ACTIVATE_ORDER))
            .unwrap_or(0)
    }

    /// Take the keyboard, when the element is shown.
    // D2-11316
    pub(crate) fn element_activate(&mut self, h: Handle) -> bool {
        if !self.is_visible(h) {
            return false;
        }
        if let Some(n) = self.node_mut(h) {
            n.active = true;
        }
        self.activation_alert(h, true);
        true
    }

    // D2-11317
    pub(crate) fn element_deactivate(&mut self, h: Handle) -> bool {
        if let Some(n) = self.node_mut(h) {
            n.active = false;
        }
        self.activation_alert(h, false);
        true
    }

    /// Counted among those that take the keyboard, or not, as its flags say now.
    // D2-11314
    pub(crate) fn update_activatable(&mut self, h: Handle) {
        self.unregister_activatable(h);
        if self.is_activatable(h) {
            self.register_activatable(h);
        }
        let active = self.node(h).is_some_and(|n| n.active);
        self.activation_alert(h, active);
    }

    // -----------------------------------------------------------------------------------------
    // Cursors and tooltips
    // -----------------------------------------------------------------------------------------

    /// The element's own cursor; shown at once while the element is topmost.
    // D2-11301
    pub fn set_element_cursor(&mut self, h: Handle, c: Cursor) {
        let Some(n) = self.node_mut(h) else { return };
        n.cursor = Some(c);
        if n.over_top && c.picture != 0 {
            self.set_cursor(c, false);
        }
    }

    /// Keep the element's cursor to put back later.
    // D2-11303
    pub fn save_cursor(&mut self, h: Handle) {
        if let Some(n) = self.node_mut(h) {
            n.saved_cursor = n.cursor;
        }
    }

    /// Put the kept cursor back (the default cursor when none was kept, while topmost).
    // D2-11302
    pub fn restore_saved_cursor(&mut self, h: Handle) {
        let saved = self
            .node(h)
            .and_then(|n| n.saved_cursor)
            .unwrap_or_default();
        self.set_element_cursor(h, saved);
        if self.node(h).is_some_and(|n| n.over_top) && saved.picture == 0 {
            self.use_default_cursor();
        }
    }

    /// Whether the element has a cursor of its own.
    // D2-11322
    pub(crate) fn has_cursor(&self, h: Handle) -> bool {
        self.node(h)
            .and_then(|n| n.cursor)
            .is_some_and(|c| c.picture != 0)
    }

    /// Whether the element shows a tooltip.
    // D2-11353
    pub(crate) fn is_tooltip_on(&self, h: Handle) -> bool {
        self.node(h).is_some_and(|n| n.bool_attr(attr::TOOLTIPS_ON))
    }

    /// What a yes-or-no attribute does when it changes: hiding shows or hides the element,
    /// `activatable` counts it among those that take the keyboard, `mouse_invisible` lets the
    /// pointer through; then what its kind does (a panel's are any element's).
    // D2-11383, D2-11642
    pub(crate) fn bool_changed(&mut self, h: Handle, a: Token, v: bool) {
        if a == attr::ACTIVATABLE {
            self.update_activatable(h);
        }
        if a == attr::HIDE {
            self.set_visible(h, !v);
        }
        if a == attr::MOUSE_INVISIBLE {
            self.set_mouse_visible(h, !v);
        }
        self.kind_bool_changed(h, a, v);
    }

    /// Whether the pointer sees the element (a region it does not see lets the pointer through to
    /// what is under it).
    // D2-11155
    pub(crate) fn set_mouse_visible(&mut self, h: Handle, on: bool) {
        if let Some(n) = self.node_mut(h) {
            n.mouse_visible = on;
        }
    }
}
