//! Resize bars: the edges and corners a window is resized by. Held down with the left button
//! while topmost, they resize their parent by the edges they name until let go.

use crate::system::{Handle, Ui};
use crate::token::{attr, state, Token};
use crate::types::button_id;

/// Which edge or corner of an element is being moved, clockwise from the upper left.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Border {
    #[default]
    None,
    UpperLeft,
    Top,
    UpperRight,
    Right,
    LowerRight,
    Bottom,
    LowerLeft,
    Left,
}

mod name {
    use crate::Token;
    pub const BORDER_RIGHT: Token = Token::of("border_right");
    pub const BORDER_LEFT: Token = Token::of("border_left");
    pub const BORDER_TOP: Token = Token::of("border_top");
    pub const BORDER_BOTTOM: Token = Token::of("border_bottom");
}

impl Ui {
    /// A new resize bar follows its flags (`allow_updating` on).
    // D2-11423, D2-11424, D2-11425
    pub(crate) fn resizebar_created(&mut self, h: Handle) {
        self.set_attr(h, attr::ALLOW_UPDATING, crate::Value::Bool(true));
    }

    /// The left button down on the topmost resize bar of a parent that is neither moving nor
    /// resizing starts resizing the parent by the edges the bar names (right before left, top
    /// before bottom: a corner when two meet), unless the bar is disabled.
    // D2-11417
    pub(crate) fn resizebar_mouse_down(
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
        let (r, l, t, bo) = (
            n.bool_attr(name::BORDER_RIGHT),
            n.bool_attr(name::BORDER_LEFT),
            n.bool_attr(name::BORDER_TOP),
            n.bool_attr(name::BORDER_BOTTOM),
        );
        let border = match (r, l, t, bo) {
            (true, _, true, _) => Border::UpperRight,
            (true, _, false, true) => Border::LowerRight,
            (true, _, false, false) => Border::Right,
            (false, true, true, _) => Border::UpperLeft,
            (false, true, false, true) => Border::LowerLeft,
            (false, true, false, false) => Border::Left,
            (false, false, true, _) => Border::Top,
            (false, false, false, true) => Border::Bottom,
            (false, false, false, false) => return,
        };
        if let Some(n) = self.node_mut(h) {
            n.held = true;
        }
        self.start_resizing(p, border);
        self.set_state(h, state::PRESSED);
        self.flush_pending();
    }

    /// The left button let go after it started a resize: the bar back to normal, the resize ends.
    // D2-11418
    pub(crate) fn resizebar_mouse_up(
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
            self.stop_resizing(p);
        }
    }

    /// A disabled bar that hides when disabled is hidden; any other bar not shown is shown.
    // D2-11419
    pub(crate) fn resizebar_update_layout(&mut self, h: Handle) {
        let Some(n) = self.node(h) else { return };
        if !n.bool_attr(attr::ALLOW_UPDATING) {
            return;
        }
        let hide = n.bool_attr(attr::DISABLED) && n.bool_attr(attr::HIDE_WHEN_DISABLED);
        let shown = self.is_visible(h);
        if !hide {
            if shown {
                return;
            }
        } else if shown {
            self.set_visible(h, false);
            return;
        }
        self.set_visible(h, true);
    }

    // D2-11422
    pub(crate) fn resizebar_bool_changed(&mut self, h: Handle, a: Token) {
        if a == attr::DISABLED || a == attr::HIDE_WHEN_DISABLED {
            self.resizebar_update_layout(h);
        }
    }
}
