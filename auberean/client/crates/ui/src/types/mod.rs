//! The element kinds: what each kind does in place of a plain element's behavior when the pointer,
//! the keyboard, the clock, a state or an attribute reaches it.
//!
//! Every element is a plain element ([`crate::input`] and [`crate::system`] hold what all of them
//! do); a kind replaces some of that behavior and usually calls the plain one first. The functions
//! here choose by the element's kind, as the client's element classes override their base class.

pub(crate) mod button;
pub(crate) mod dragbar;
pub(crate) mod field;
pub mod listbox;
pub mod listboxmsize;
pub mod menu;
pub mod meter;
pub mod panel;
pub mod resizebar;
pub mod scrollbar;
pub mod text;
pub mod tree;
pub mod viewport;

use crate::system::{Handle, Ui};
use crate::token::{kind, Token};

/// A pointer button or wheel step, as the interface receives it.
pub mod button_id {
    /// The left button.
    pub const LEFT: u32 = 0x4000_0004;
    /// The right button.
    pub const RIGHT: u32 = 0x4000_0005;
    /// The middle button.
    pub const MIDDLE: u32 = 0x4000_0006;
    /// The left, right and middle buttons pressed twice quickly.
    pub const DOUBLE_LEFT: u32 = 0x4000_0007;
    pub const DOUBLE_RIGHT: u32 = 0x4000_0008;
    pub const DOUBLE_MIDDLE: u32 = 0x4000_0009;
    /// The wheel turned up (away from the player) or down, one step.
    pub const WHEEL_UP: u32 = 0x4000_00B8;
    pub const WHEEL_DOWN: u32 = 0x4000_00B9;
}

/// The interface keys that reach the active element.
pub mod key_id {
    pub const LEFT: u32 = 0x4000_0013;
    pub const RIGHT: u32 = 0x4000_0014;
    pub const UP: u32 = 0x4000_0015;
    pub const DOWN: u32 = 0x4000_0016;
    /// The key that moves to the next element taking the keyboard.
    pub const NEXT: u32 = 0x4000_0017;
    /// The key that uses the active element (presses the active button).
    pub const USE: u32 = 0x4000_0018;
    pub const PAGE_UP: u32 = 0x4000_0019;
    pub const PAGE_DOWN: u32 = 0x4000_001A;
    pub const HOME: u32 = 0x4000_001B;
    pub const END: u32 = 0x4000_001C;
    pub const ESCAPE: u32 = 0x4000_001E;
}

impl Ui {
    fn kind_of(&self, h: Handle) -> Token {
        self.node(h).map_or(Token(0), |n| n.kind)
    }

    /// A press on an element under the pointer.
    pub(crate) fn mouse_down(&mut self, h: Handle, x: i32, y: i32, b: u32, handled: &mut bool) {
        match self.kind_of(h) {
            k if k == kind::BUTTON => self.button_mouse_down(h, x, y, b, handled),
            k if k == kind::DRAGBAR => self.dragbar_mouse_down(h, x, y, b, handled),
            k if k == kind::RESIZEBAR => self.resizebar_mouse_down(h, x, y, b, handled),
            k if k == kind::SCROLLBAR => self.scrollbar_mouse_down(h, x, y, b, handled),
            k if k == kind::LISTBOX || k == kind::LISTBOX_MULTI_SIZE => {
                self.listbox_mouse_down(h, x, y, b, handled);
            }
            k if k == kind::TREE => self.tree_mouse_down(h, x, y, b, handled),
            k if k == kind::MENU => self.menu_mouse_down(h, x, y, b, handled),
            k if k == kind::TEXT => self.text_mouse_down(h, x, y, b, handled),
            k if k == kind::PANEL_PAGE => self.panel_page_mouse_down(h, x, y, b, handled),
            _ => self.element_mouse_down(h, x, y, b, handled),
        }
    }

    /// A release of a button pressed on an element.
    pub(crate) fn mouse_up(&mut self, h: Handle, x: i32, y: i32, b: u32, handled: &mut bool) {
        match self.kind_of(h) {
            k if k == kind::BUTTON => self.button_mouse_up(h, x, y, b, handled),
            k if k == kind::DRAGBAR => self.dragbar_mouse_up(h, x, y, b, handled),
            k if k == kind::RESIZEBAR => self.resizebar_mouse_up(h, x, y, b, handled),
            k if k == kind::SCROLLBAR => self.scrollbar_mouse_up(h, x, y, b, handled),
            k if k == kind::TEXT => self.text_mouse_up(h, x, y, b, handled),
            _ => self.element_mouse_up(h, x, y, b, handled),
        }
    }

    /// The pointer moved over an element (coordinates in its parent's).
    pub(crate) fn mouse_move(&mut self, h: Handle, x: i32, y: i32, handled: &mut bool) {
        self.element_mouse_move(h, x, y, handled);
    }

    /// The pointer came over an element or left it.
    pub(crate) fn mouse_over(&mut self, h: Handle, on: bool, handled: &mut bool) {
        self.element_mouse_over(h, on, handled);
    }

    /// An element became, or stopped being, the topmost under the pointer.
    pub(crate) fn mouse_over_top(&mut self, h: Handle, on: bool, handled: &mut bool) {
        match self.kind_of(h) {
            k if k == kind::BUTTON => self.button_mouse_over_top(h, on, handled),
            k if k == kind::FIELD => self.field_mouse_over_top(h, on, handled),
            k if k == kind::PANEL_PAGE => self.panel_page_mouse_over_top(h, on, handled),
            k if k == kind::LISTBOX || k == kind::LISTBOX_MULTI_SIZE => {
                self.listbox_mouse_over_top(h, on, handled);
            }
            k if k == kind::MENU => self.menu_mouse_over_top(h, on, handled),
            _ => self.element_mouse_over_top(h, on, handled),
        }
    }

    /// A message to an element as a listener: the pointer moving while it moves or resizes, the
    /// clock.
    pub(crate) fn listen(&mut self, h: Handle, what: u32, param: u32) {
        match self.kind_of(h) {
            k if k == kind::BUTTON => self.button_listen(h, what, param),
            k if k == kind::SCROLLBAR => self.scrollbar_listen(h, what, param),
            k if k == kind::LISTBOX || k == kind::LISTBOX_MULTI_SIZE => {
                self.listbox_listen(h, what, param);
            }
            k if k == kind::METER => self.meter_listen(h, what, param),
            k if k == kind::MENU => self.menu_listen(h, what, param),
            k if k == kind::TEXT => self.text_listen(h, what, param),
            _ => self.element_listen(h, what, param),
        }
    }

    /// The state change of an element's kind; returns whether the state exists.
    pub(crate) fn kind_set_state(&mut self, h: Handle, s: Token) -> bool {
        match self.kind_of(h) {
            k if k == kind::BUTTON => self.button_set_state(h, s),
            k if k == kind::DRAGBAR => {
                // D2-11488
                let r = self.set_state_base(h, s);
                self.dragbar_update_layout(h);
                r
            }
            k if k == kind::RESIZEBAR => {
                // D2-11421
                let r = self.set_state_base(h, s);
                self.resizebar_update_layout(h);
                r
            }
            k if k == kind::PANEL_PAGE => self.panel_page_set_state(h, s),
            k if k == kind::LISTBOX || k == kind::LISTBOX_MULTI_SIZE => {
                self.listbox_set_state(h, s)
            }
            k if k == kind::SCROLLBAR => self.scrollbar_set_state(h, s),
            k if k == kind::METER => self.meter_set_state(h, s),
            k if k == kind::TREE => self.tree_set_state(h, s),
            _ => self.set_state_base(h, s),
        }
    }

    /// An element is told it is made: its kind finishes (a panel listens to its tabs, a tab makes
    /// its page).
    pub(crate) fn initialized(&mut self, h: Handle) {
        match self.kind_of(h) {
            k if k == kind::PANEL => self.panel_initialized(h),
            k if k == kind::PANEL_PAGE => self.panel_page_initialized(h),
            _ => self.element_initialized(h),
        }
    }

    /// A plain element made.
    // D2-11289
    pub(crate) fn element_initialized(&mut self, h: Handle) {
        if let Some(n) = self.node_mut(h) {
            n.initialized = true;
        }
    }

    /// Whether an element has been told it is made.
    // D2-11288
    #[must_use]
    pub fn is_initialized(&self, h: Handle) -> bool {
        self.node(h).is_some_and(|n| n.initialized)
    }

    /// What an element's kind does after one of its yes-or-no attributes changed.
    pub(crate) fn kind_bool_changed(&mut self, h: Handle, a: Token, v: bool) {
        match self.kind_of(h) {
            k if k == kind::BUTTON => self.button_bool_changed(h, a, v),
            k if k == kind::DRAGBAR => self.dragbar_bool_changed(h, a),
            k if k == kind::RESIZEBAR => self.resizebar_bool_changed(h, a),
            k if k == kind::LISTBOX || k == kind::LISTBOX_MULTI_SIZE => {
                self.listbox_bool_changed(h, a, v);
            }
            k if k == kind::SCROLLBAR => self.scrollbar_bool_changed(h, a, v),
            k if k == kind::TREE => self.tree_bool_changed(h, a),
            k if k == kind::MENU => self.menu_bool_changed(h, a, v),
            _ => {}
        }
    }

    /// Whether an element can take the keyboard.
    pub(crate) fn is_activatable(&self, h: Handle) -> bool {
        match self.kind_of(h) {
            k if k == kind::BUTTON => self.button_is_activatable(h),
            _ => self.element_is_activatable(h),
        }
    }

    /// Give an element the keyboard as its kind takes it (a text element says it was activated);
    /// returns whether it took it.
    pub fn activate_element(&mut self, h: Handle) -> bool {
        match self.kind_of(h) {
            k if k == kind::BUTTON => self.button_activate(h),
            k if k == kind::PANEL_PAGE => self.panel_page_activate(h),
            k if k == kind::TEXT => self.text_activate(h),
            _ => self.element_activate(h),
        }
    }

    /// Take the keyboard from an element as its kind gives it up (a text element says it was
    /// deactivated); returns whether it had a manager to tell.
    pub fn deactivate_element(&mut self, h: Handle) -> bool {
        match self.kind_of(h) {
            k if k == kind::BUTTON => self.button_deactivate(h),
            k if k == kind::PANEL_PAGE => self.panel_page_deactivate(h),
            k if k == kind::TEXT => self.text_deactivate(h),
            _ => self.element_deactivate(h),
        }
    }

    /// An interface key pressed (`down`) or let go while an element has the keyboard; returns
    /// whether the element used it.
    pub(crate) fn element_key(&mut self, h: Handle, key: u32, down: bool) -> bool {
        match (self.kind_of(h), key, down) {
            (k, key_id::USE, true) if k == kind::BUTTON => self.button_start_use_active(h),
            // A text element moves its caret for the key but never uses it up: the interface's
            // listeners hear the key too (the chat window's history keys).
            (k, key, down) if k == kind::TEXT => {
                self.text_action(h, key, down);
                false
            }
            (k, key_id::USE, false) if k == kind::BUTTON => self.button_end_use_active(h),
            (k, key_id::USE, true) if k == kind::PANEL_PAGE => self.panel_page_start_use_active(h),
            (k, key_id::UP | key_id::DOWN | key_id::LEFT | key_id::RIGHT, true)
                if k == kind::LISTBOX || k == kind::LISTBOX_MULTI_SIZE =>
            {
                self.listbox_key(h, key)
            }
            _ => false,
        }
    }

    /// What an element's kind does after an attribute other than a yes-or-no one was set
    /// (`changed` when its value is new): a list's views and goals, a scroll bar's positions and
    /// sizes, a meter's position and goal, a tree's views, the elements a kind listens to.
    pub(crate) fn kind_value_set(&mut self, h: Handle, a: Token, v: &crate::Value, changed: bool) {
        use crate::Value;
        let list = |k: Token| k == kind::LISTBOX || k == kind::LISTBOX_MULTI_SIZE;
        match (self.kind_of(h), v) {
            (k, Value::Int(i)) if list(k) => self.listbox_int_set(h, a, *i, changed),
            (k, Value::Id(id)) if changed && list(k) => self.listbox_id_changed(h, a, *id),
            (k, Value::Float(f)) if k == kind::SCROLLBAR => {
                self.scrollbar_float_set(h, a, *f, changed);
            }
            (k, Value::Int(_)) if changed && k == kind::SCROLLBAR => {
                self.scrollbar_int_changed(h, a);
            }
            (k, Value::Id(id)) if changed && k == kind::SCROLLBAR => {
                self.scrollbar_id_changed(h, a, *id);
            }
            (k, Value::Float(f)) if k == kind::METER => self.meter_float_set(h, a, *f, changed),
            (k, Value::Int(i)) if k == kind::TREE => self.tree_int_set(h, a, *i, changed),
            (k, Value::Id(id)) if changed && k == kind::TREE => self.tree_id_changed(h, a, *id),
            _ => {}
        }
    }

    /// What the list kinds do when they are made.
    pub(crate) fn list_kind_created(&mut self, h: Handle, k: Token) {
        match k {
            k if k == kind::LISTBOX => self.listbox_created(h),
            k if k == kind::LISTBOX_MULTI_SIZE => self.msize_created(h),
            k if k == kind::METER => self.meter_created(h),
            k if k == kind::TREE => self.tree_created(h),
            k if k == kind::MENU => self.menu_created(h),
            _ => {}
        }
    }

    /// What an element does as it goes away (a menu takes its popup with it; a text element lets
    /// go of what its strings wait for).
    pub(crate) fn kind_deleting(&mut self, h: Handle) {
        self.cleanup_string_downloads(h);
        match self.kind_of(h) {
            k if k == kind::MENU => self.menu_deleting(h),
            k if k == kind::LISTBOX || k == kind::LISTBOX_MULTI_SIZE => self.listbox_deleting(h),
            k if k == kind::SCROLLBAR => self.scrollbar_deleting(h),
            k if k == kind::TREE => self.tree_deleting(h),
            k if k == kind::METER => self.meter_deleting(h),
            _ => {}
        }
    }

    /// What an element does after it was resized.
    pub(crate) fn kind_resized(&mut self, h: Handle) {
        match self.kind_of(h) {
            k if k == kind::LISTBOX || k == kind::LISTBOX_MULTI_SIZE => self.listbox_resized(h),
            k if k == kind::SCROLLBAR => self.lay_out_scrollbar(h),
            // D2-11629
            k if k == kind::METER => self.lay_out_meter(h),
            _ => {}
        }
    }
}
