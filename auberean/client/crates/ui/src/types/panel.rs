//! Panels and their page tabs: a panel holds tabs (`panel_page` elements), each showing its own
//! page while open. Opening one tab closes the others; a tab rolls over under the pointer and
//! opens when pressed or used.

use crate::system::{Handle, Ui};
use crate::token::{kind, state, Token};
use crate::types::button_id;

/// A tab's states and what it says.
pub mod page {
    use crate::Token;
    pub const OPEN: Token = Token::of("open");
    pub const CLOSED: Token = Token::of("closed");
    pub const ROLLOVER: Token = Token::of("rollover");
    /// A tab opened, and closed.
    pub const OPENED: Token = Token::of("opened");
    pub const CLOSED_MSG: Token = Token::of("closed");
    /// The attribute naming the tab's page: a top-level element of the tab's layout.
    pub const PAGE: Token = Token::of("page");
}

impl Ui {
    /// The panel's tabs: its children made from tab templates.
    fn panel_pages(&self, h: Handle) -> Vec<Handle> {
        self.node(h)
            .map(|n| {
                n.children
                    .iter()
                    .copied()
                    .filter(|c| self.node(*c).is_some_and(|cn| cn.kind == kind::PANEL_PAGE))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// A panel made: it listens to each of its tabs opening and closing.
    // D2-11643, D2-11641, D2-11646
    pub(crate) fn panel_initialized(&mut self, h: Handle) {
        for p in self.panel_pages(h) {
            let Some(id) = self.node(p).map(|n| n.id) else {
                continue;
            };
            self.register_for_message(h, id, page::OPENED);
            self.register_for_message(h, id, page::CLOSED_MSG);
        }
        self.element_initialized(h);
    }

    /// A panel going away stops listening to its tabs and closes them.
    // D2-11640, D2-11645
    pub(crate) fn panel_removing(&mut self, h: Handle) {
        for p in self.panel_pages(h) {
            let Some(id) = self.node(p).map(|n| n.id) else {
                continue;
            };
            self.unregister_for_message(h, id, page::OPENED);
            self.unregister_for_message(h, id, page::CLOSED_MSG);
            self.set_state(p, page::CLOSED);
        }
    }

    /// One of the panel's tabs opened: every other open tab closes.
    // D2-11644
    pub(crate) fn panel_hear(&mut self, h: Handle, from: Handle, what: Token) {
        if what != page::OPENED {
            return;
        }
        for p in self.panel_pages(h) {
            if p != from && self.node(p).is_some_and(|n| n.state == page::OPEN) {
                self.set_state(p, page::CLOSED);
                self.flush_pending();
            }
        }
    }

    /// A tab made: its page (the top-level element its `page` names, from the tab's layout) is
    /// made beside it under the same parent, shown only while the tab is open; a tab with no
    /// parent has no page.
    // D2-11648, D2-11647, D2-11656
    pub(crate) fn panel_page_initialized(&mut self, h: Handle) {
        let Some(n) = self.node(h) else { return };
        let name = n.id_attr(page::PAGE).map(Token);
        let layout = self.library().get(n.refdesc).layout;
        let parent = n.parent;
        let open = n.state == page::OPEN;
        let made = name.and_then(|name| self.create(layout, name));
        match (made, parent) {
            (Some(pg), Some(p)) => {
                self.set_parent(pg, p);
                self.set_visible(pg, open);
                if let Some(n) = self.node_mut(h) {
                    n.page = Some(pg);
                }
            }
            (Some(pg), None) => self.remove(pg),
            _ => {}
        }
        self.element_initialized(h);
    }

    /// A tab's state: its page shows only while open; opening and closing are announced.
    // D2-11649
    pub(crate) fn panel_page_set_state(&mut self, h: Handle, s: Token) -> bool {
        let r = self.set_state_base(h, s);
        let Some(pg) = self.node(h).and_then(|n| n.page) else {
            return r;
        };
        if r {
            self.set_visible(pg, s == page::OPEN);
            if s == page::OPEN {
                self.broadcast(h, page::OPENED, 0);
            } else if s == page::CLOSED {
                self.broadcast(h, page::CLOSED_MSG, 0);
            }
        }
        r
    }

    /// The left button down on a tab opens it (unless it is moving or resizing).
    // D2-11650
    pub(crate) fn panel_page_mouse_down(
        &mut self,
        h: Handle,
        x: i32,
        y: i32,
        b: u32,
        handled: &mut bool,
    ) {
        self.element_mouse_down(h, x, y, b, handled);
        let Some(n) = self.node(h) else { return };
        if !n.moving && !n.resizing && b == button_id::LEFT {
            self.panel_page_start_use_active(h);
        }
    }

    /// A closed tab rolls over under the pointer, and back when the pointer leaves.
    // D2-11651
    pub(crate) fn panel_page_mouse_over_top(&mut self, h: Handle, on: bool, handled: &mut bool) {
        self.element_mouse_over_top(h, on, handled);
        let s = self.node(h).map_or(Token(0), |n| n.state);
        if !on {
            if s == page::ROLLOVER {
                self.set_state(h, page::CLOSED);
                self.flush_pending();
            }
        } else if s == page::CLOSED {
            self.set_state(h, page::ROLLOVER);
            self.flush_pending();
        }
    }

    /// A closed tab taking the keyboard rolls over.
    // D2-11652
    pub(crate) fn panel_page_activate(&mut self, h: Handle) -> bool {
        if !self.element_activate(h) {
            return false;
        }
        if self.node(h).is_some_and(|n| n.state == page::CLOSED) {
            self.set_state(h, page::ROLLOVER);
            self.flush_pending();
        }
        true
    }

    /// A rolled-over tab giving up the keyboard closes again.
    // D2-11653
    pub(crate) fn panel_page_deactivate(&mut self, h: Handle) -> bool {
        if !self.element_deactivate(h) {
            return false;
        }
        if self.node(h).is_some_and(|n| n.state == page::ROLLOVER) {
            self.set_state(h, page::CLOSED);
            self.flush_pending();
        }
        true
    }

    /// Using a tab opens it, unless it is open already or ghosted.
    // D2-11654
    pub fn panel_page_start_use_active(&mut self, h: Handle) -> bool {
        let s = self.node(h).map_or(Token(0), |n| n.state);
        if s != page::OPEN && s != state::GHOSTED {
            self.set_state(h, page::OPEN);
            self.flush_pending();
        }
        true
    }
}
