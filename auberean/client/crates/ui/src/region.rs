//! Regions: every element is a rectangle inside its parent's, shown or not, that tracks the
//! pointer. Moving the pointer tells each region whether it is over it and which one is topmost
//! under it; a press reaches every region the pointer is over, children first, until one takes
//! it; a release reaches the regions the button went down on.

use crate::system::{Handle, Ui};
use crate::Rect;

impl Ui {
    /// Whether an element and every element above it are shown.
    // D2-01859
    #[must_use]
    pub fn is_visible(&self, h: Handle) -> bool {
        self.shown(h)
    }

    /// The pointer at `(x, y)` in the parent's coordinates, inside the parent when
    /// `parent_contains`: the children first (front to back) until one takes the move, then this
    /// region. The first region holding the pointer that shows tooltips becomes `tip`; the first
    /// that sees the pointer becomes `top`. A region holding the pointer gets the move (unless one
    /// was taken), and is told when the pointer comes over or leaves it and when it becomes or
    /// stops being the topmost.
    // D2-11156
    #[allow(clippy::too_many_arguments)] // The original's own: the point, the parent's answer, three results.
    pub(crate) fn notify_mouse_move(
        &mut self,
        h: Handle,
        x: i32,
        y: i32,
        parent_contains: bool,
        top: &mut Option<Handle>,
        tip: &mut Option<Handle>,
        handled: &mut bool,
    ) {
        let Some(n) = self.node(h) else { return };
        let r = n.rect;
        let contains = parent_contains && n.visible && r.contains(x, y);
        let kids = n.children.clone();
        for c in kids {
            if *handled {
                break;
            }
            self.notify_mouse_move(c, x - r.x0, y - r.y0, contains, top, tip, handled);
        }
        if tip.is_none() && contains && self.is_tooltip_on(h) {
            *tip = Some(h);
        }
        let Some(n) = self.node(h) else { return };
        let over = contains && n.mouse_visible;
        let mut is_top = false;
        if top.is_none() && over {
            *top = Some(h);
            is_top = true;
        }
        if over && !*handled {
            self.mouse_move(h, x, y, handled);
        }
        if self.node(h).is_some_and(|n| n.over != over) && !*handled {
            self.mouse_over(h, over, handled);
        }
        if self.node(h).is_some_and(|n| n.over_top != is_top) && !*handled {
            self.mouse_over_top(h, is_top, handled);
        }
    }

    /// A button pressed at `(x, y)`: the children first until one takes it, then this region.
    /// A region that already holds this button (a press it never saw released) gets a release
    /// if the pointer has left it; otherwise a region the pointer is over gets the press and
    /// holds the button. The wheel never holds.
    // D2-11182
    pub(crate) fn notify_mouse_down(
        &mut self,
        h: Handle,
        x: i32,
        y: i32,
        button: u32,
        handled: &mut bool,
    ) {
        let Some(n) = self.node(h) else { return };
        let kids = n.children.clone();
        for c in kids {
            if *handled {
                break;
            }
            self.notify_mouse_down(c, x, y, button, handled);
        }
        let Some(n) = self.node(h) else { return };
        let wheel = button == crate::types::button_id::WHEEL_UP
            || button == crate::types::button_id::WHEEL_DOWN;
        if !wheel && n.holds(button) {
            if n.over {
                return;
            }
            if !*handled {
                self.mouse_up(h, x, y, button, handled);
            }
            self.set_held(h, button, false);
            return;
        }
        if n.over {
            if !*handled {
                self.mouse_down(h, x, y, button, handled);
            }
            self.set_held(h, button, true);
        }
    }

    /// A button let go at `(x, y)`: the children first until one takes it, then this region if
    /// it holds the button.
    // D2-11183
    pub(crate) fn notify_mouse_up(
        &mut self,
        h: Handle,
        x: i32,
        y: i32,
        button: u32,
        handled: &mut bool,
    ) {
        let Some(n) = self.node(h) else { return };
        let kids = n.children.clone();
        for c in kids {
            if *handled {
                break;
            }
            self.notify_mouse_up(c, x, y, button, handled);
        }
        if self.node(h).is_some_and(|n| n.holds(button)) {
            if !*handled {
                self.mouse_up(h, x, y, button, handled);
            }
            self.set_held(h, button, false);
        }
    }

    fn set_held(&mut self, h: Handle, button: u32, on: bool) {
        if let Some(n) = self.node_mut(h) {
            n.buttons.retain(|b| *b != button);
            if on {
                n.buttons.push(button);
            }
        }
    }

    /// The part of an element that shows on the screen: its rectangle cut by every element above
    /// it.
    // D2-11170
    #[must_use]
    pub fn screen_clip_rect(&self, h: Handle) -> Option<Rect> {
        let mut r = self.screen_rect(h)?;
        let mut p = self.node(h)?.parent;
        while let Some(ph) = p {
            r = r.intersect(self.screen_rect(ph)?)?;
            p = self.node(ph)?.parent;
        }
        Some(r)
    }

    /// The top-level element whose picture an element is drawn into, and where that picture
    /// stands on the screen.
    // D2-11157, D2-11158, D2-11159
    #[must_use]
    pub fn object_of(&self, h: Handle) -> Option<(Handle, i32, i32)> {
        self.node(h)?;
        let root = self.root_of(h);
        let r = self.node(root)?.rect;
        Some((root, r.x0, r.y0))
    }

    /// Where on the screen the top-level element an element is drawn with stands: its left and
    /// top.
    // D2-11164, D2-11165
    #[must_use]
    pub fn object_origin(&self, h: Handle) -> Option<(i32, i32)> {
        self.node(h)?;
        self.screen_origin(self.root_of(h))
    }

    /// An element's left and top on the screen.
    // D2-11160, D2-11161
    #[must_use]
    pub fn screen_origin(&self, h: Handle) -> Option<(i32, i32)> {
        self.screen_rect(h).map(|r| (r.x0, r.y0))
    }
}
