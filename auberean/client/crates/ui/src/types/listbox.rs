//! List boxes: rows laid out on a sheet in a grid of cells the first row's size, a view onto the
//! sheet that scrolls (at once, or smoothly over the clock), one row selected, the list's scroll
//! bars and arrow buttons kept in step with the view.
//!
//! The sheet is `cols` by `rows` cells; a vertical list fills down its columns, a horizontal one
//! across its rows. The view's place on the sheet is the `x_view` and `y_view` attributes; a goal
//! view (`goal_x_view`, `goal_y_view`) is reached at once, or over `smooth_movement_duration`
//! seconds for a smooth list. Rows outside the view lie outside the list and are clipped by it.

use crate::manager::global;
use crate::system::{Handle, ListState, Ui, Value};
use crate::token::{attr, msg, state, Token};
use crate::types::button_id;

/// The state the list kinds keep between events, one block per kind (an element uses its own).
#[derive(Clone, Debug, Default)]
pub struct ListKinds {
    pub list: ListBoxState,
    pub scroll: crate::types::scrollbar::ScrollBarState,
    pub meter: crate::types::meter::MeterState,
    pub tree: crate::types::tree::TreeState,
    pub menu: crate::types::menu::MenuState,
}

/// What a list box keeps: its sheet, its view, its selection and its smooth scrolling.
#[derive(Clone, Debug, Default)]
pub struct ListBoxState {
    pub paper_w: i32,
    pub paper_h: i32,
    pub cols: i32,
    pub rows: i32,
    pub item_w: i32,
    pub item_h: i32,
    /// The view's place on the sheet, as last applied.
    pub x_view: i32,
    pub y_view: i32,
    pub anim_start: f64,
    pub anim_end: f64,
    pub anim_start_x: i32,
    pub anim_start_y: i32,
    pub anim_end_x: i32,
    pub anim_end_y: i32,
    pub bits: u32,
    pub selected: Option<Handle>,
    /// The row a dragged item is over, and the state it had.
    pub drag_over: Option<Handle>,
    pub drag_over_state: Token,
    /// A list of rows of their own sizes: each column's width and each row's height.
    pub widths: Vec<i32>,
    pub heights: Vec<i32>,
}

/// The flags a list keeps from its attributes and its own doings.
pub(crate) mod bits {
    pub const HORIZONTAL: u32 = 0x1;
    pub const SMOOTH: u32 = 0x2;
    pub const CLICK_SELECT: u32 = 0x4;
    pub const DRAG_SELECT: u32 = 0x8;
    pub const DRAG_ROLLOVER: u32 = 0x10;
    pub const ALLOW_UPDATING: u32 = 0x20;
    pub const ALLOW_CONTROL_UPDATING: u32 = 0x40;
    pub const ITEM_STATE_CHANGE: u32 = 0x80;
    pub const STATIC_BACKGROUND: u32 = 0x100;
    pub const LARGE_ITEM_SCROLL: u32 = 0x200;
    pub const UNEVEN_AT_END: u32 = 0x400;
    /// The list's scroll bar is a horizontal one.
    pub const HORIZONTAL_SCROLLBAR: u32 = 0x800;
    pub const ANIMATING: u32 = 0x8000;
    pub const DRAG_SELECTING: u32 = 0x1_0000;
    /// The view is being set from the scroll bar (which needs no telling).
    pub const FROM_SCROLLBAR: u32 = 0x2_0000;
    pub const RESTARTED: u32 = 0x4_0000;
    /// A view set from outside stops a smooth scroll (off while the scroll sets it).
    pub const VIEW_STOPS_ANIMATION: u32 = 0x8_0000;
    pub const ROLLOVER: u32 = 0x10_0000;
    /// Several rows change at once: the layout waits.
    pub const MULTI_OP: u32 = 0x20_0000;
}

/// A goal view that is no goal.
pub(crate) const NO_GOAL: i32 = -1_000_000;

pub(crate) mod name {
    use crate::Token;
    pub const X_VIEW: Token = Token::of("x_view");
    pub const Y_VIEW: Token = Token::of("y_view");
    pub const GOAL_X_VIEW: Token = Token::of("goal_x_view");
    pub const GOAL_Y_VIEW: Token = Token::of("goal_y_view");
    pub const HORIZONTAL: Token = Token::of("horizontal");
    pub const SMOOTH_MOVEMENT: Token = Token::of("smooth_movement");
    pub const SMOOTH_MOVEMENT_DURATION: Token = Token::of("smooth_movement_duration");
    pub const CLICK_SELECT: Token = Token::of("click_select");
    pub const DRAG_SELECT: Token = Token::of("drag_select");
    pub const DRAG_ROLLOVER: Token = Token::of("drag_rollover");
    pub const ALLOW_CONTROL_UPDATING: Token = Token::of("allow_control_updating");
    pub const STATIC_BACKGROUND: Token = Token::of("static_background");
    pub const ALLOW_LARGEITEM_SCROLL: Token = Token::of("allow_largeitem_scroll");
    pub const ALLOW_UNEVEN_AT_END: Token = Token::of("allow_uneven_at_end");
    pub const UP_BUTTON: Token = Token::of("up_button");
    pub const DOWN_BUTTON: Token = Token::of("down_button");
    pub const HORIZONTAL_SCROLLBAR: Token = Token::of("horizontal_scrollbar");
    pub const UNIQUE_MSG: Token = Token::of("unique_msg");
    /// A list's messages.
    pub const NEW_VIEW: Token = Token::of("new_view");
    /// What a scroll bar says to the list it serves.
    pub const WIDGET_PRESS: Token = Token::of("widget_press");
    pub const WIDGET_RELEASE: Token = Token::of("widget_release");
    pub const V_LINE_DEC: Token = Token::of("vertical_line_decrement");
    pub const V_LINE_INC: Token = Token::of("vertical_line_increment");
    pub const V_PAGE_DEC: Token = Token::of("vertical_page_decrement");
    pub const V_PAGE_INC: Token = Token::of("vertical_page_increment");
    pub const H_LINE_DEC: Token = Token::of("horizontal_line_decrement");
    pub const H_LINE_INC: Token = Token::of("horizontal_line_increment");
    pub const H_PAGE_DEC: Token = Token::of("horizontal_page_decrement");
    pub const H_PAGE_INC: Token = Token::of("horizontal_page_increment");
}

/// The list's bits that follow its yes-or-no attributes.
const BOOL_BITS: [(Token, u32); 11] = [
    (name::HORIZONTAL, bits::HORIZONTAL),
    (name::SMOOTH_MOVEMENT, bits::SMOOTH),
    (name::CLICK_SELECT, bits::CLICK_SELECT),
    (name::DRAG_SELECT, bits::DRAG_SELECT),
    (name::DRAG_ROLLOVER, bits::DRAG_ROLLOVER),
    (attr::ALLOW_UPDATING, bits::ALLOW_UPDATING),
    (name::ALLOW_CONTROL_UPDATING, bits::ALLOW_CONTROL_UPDATING),
    (attr::SELECTED_ITEM_STATE_CHANGE, bits::ITEM_STATE_CHANGE),
    (name::STATIC_BACKGROUND, bits::STATIC_BACKGROUND),
    (name::ALLOW_LARGEITEM_SCROLL, bits::LARGE_ITEM_SCROLL),
    (name::ALLOW_UNEVEN_AT_END, bits::UNEVEN_AT_END),
];

/// What a list (or tree) hears from its vertical and its horizontal scroll bar.
pub(crate) const SCROLLBAR_MESSAGES_V: [Token; 6] = [
    msg::NEW_POSITION,
    name::WIDGET_RELEASE,
    name::V_LINE_INC,
    name::V_LINE_DEC,
    name::V_PAGE_INC,
    name::V_PAGE_DEC,
];
pub(crate) const SCROLLBAR_MESSAGES_H: [Token; 6] = [
    msg::NEW_POSITION,
    name::WIDGET_RELEASE,
    name::H_LINE_INC,
    name::H_LINE_DEC,
    name::H_PAGE_INC,
    name::H_PAGE_DEC,
];

/// A list's scrolls by line and page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Scroll {
    VPageDec,
    VPageInc,
    HPageDec,
    HPageInc,
    VLineDec,
    VLineInc,
    HLineDec,
    HLineInc,
}

impl Scroll {
    /// The scroll a scroll bar's message asks for.
    pub(crate) fn of_message(what: Token) -> Option<Self> {
        Some(match what {
            w if w == name::V_PAGE_INC => Self::VPageInc,
            w if w == name::V_PAGE_DEC => Self::VPageDec,
            w if w == name::V_LINE_INC => Self::VLineInc,
            w if w == name::V_LINE_DEC => Self::VLineDec,
            w if w == name::H_PAGE_INC => Self::HPageInc,
            w if w == name::H_PAGE_DEC => Self::HPageDec,
            w if w == name::H_LINE_INC => Self::HLineInc,
            w if w == name::H_LINE_DEC => Self::HLineDec,
            _ => return None,
        })
    }
}

impl Ui {
    fn lb(&self, h: Handle) -> Option<&ListBoxState> {
        self.node(h).map(|n| &n.lists.list)
    }

    fn lb_mut(&mut self, h: Handle) -> Option<&mut ListBoxState> {
        self.node_mut(h).map(|n| &mut n.lists.list)
    }

    /// Whether one of a list's flags is on.
    // D2-11508
    pub(crate) fn lb_test(&self, h: Handle, bit: u32) -> bool {
        self.lb(h).is_some_and(|l| l.bits & bit != 0)
    }

    pub(crate) fn lb_set_bit(&mut self, h: Handle, bit: u32, on: bool) {
        if let Some(l) = self.lb_mut(h) {
            if on {
                l.bits |= bit;
            } else {
                l.bits &= !bit;
            }
        }
    }

    pub(crate) fn int_or(&self, h: Handle, a: Token, or: i32) -> i32 {
        self.node(h).and_then(|n| n.int_attr(a)).unwrap_or(or)
    }

    pub(crate) fn float_or(&self, h: Handle, a: Token, or: f32) -> f32 {
        self.node(h).and_then(|n| n.float_attr(a)).unwrap_or(or)
    }

    /// Set an attribute only when the element's records left it unset: what an element's kind
    /// starts with, which its records override.
    pub(crate) fn default_attr(&mut self, h: Handle, a: Token, v: Value) {
        if self.node(h).is_some_and(|n| n.attr(a).is_none()) {
            self.set_attr(h, a, v);
        }
    }

    /// The element a named attribute points to: in the element's own window when it asks for
    /// unique messages, else the element of that name wherever it is, made and not yet on the
    /// screen included (a window's tree finds its scroll bar while the window is being made).
    // D2-11261
    pub(crate) fn named_element(&self, h: Handle, a: Token) -> Option<Handle> {
        let id = self.node(h)?.id_attr(a)?;
        if id == 0 {
            return None;
        }
        if self.node(h)?.bool_attr(name::UNIQUE_MSG) {
            let root = self.root_of(h);
            if self.node(root)?.id == Token(id) {
                return Some(root);
            }
            return self.child_recursive(root, Token(id));
        }
        self.element(Token(id))
    }

    /// A list's rows, in order.
    #[must_use]
    pub fn list_items(&self, list: Handle) -> Vec<Handle> {
        self.node(list)
            .map(|n| n.list.items.clone())
            .unwrap_or_default()
    }

    /// The selected row's place in the list.
    // D2-01752
    #[must_use]
    pub fn list_selected_index(&self, list: Handle) -> Option<usize> {
        let sel = self.lb(list)?.selected?;
        self.list_items(list).iter().position(|i| *i == sel)
    }

    /// A row's place in the list.
    // D2-02497
    #[must_use]
    pub fn list_index_of(&self, list: Handle, item: Handle) -> Option<usize> {
        self.list_items(list).iter().position(|i| *i == item)
    }

    /// The selected row.
    #[must_use]
    pub fn list_selected_item(&self, list: Handle) -> Option<Handle> {
        self.lb(list).and_then(|l| l.selected)
    }

    fn sync_selected_index(&mut self, list: Handle) {
        let i = self.list_selected_index(list);
        if let Some(n) = self.node_mut(list) {
            n.list.selected = i;
        }
    }

    /// Whether an element is a list box whose rows keep their own sizes.
    pub(crate) fn is_msize(&self, h: Handle) -> bool {
        self.node(h)
            .is_some_and(|n| n.kind == crate::token::kind::LISTBOX_MULTI_SIZE)
    }

    // -----------------------------------------------------------------------------------------
    // Making
    // -----------------------------------------------------------------------------------------

    /// A new list: updating its sheet and its controls on, a smooth scroll of a quarter second,
    /// the view at the sheet's top left, one column, no goal view; its flags from its attributes;
    /// listening to its arrow buttons and scroll bars; the clock heard; then laid out.
    // D2-11575, D2-11576, D2-11580
    pub(crate) fn listbox_created(&mut self, h: Handle) {
        self.default_attr(h, attr::ALLOW_UPDATING, Value::Bool(true));
        self.default_attr(h, name::ALLOW_CONTROL_UPDATING, Value::Bool(true));
        self.default_attr(h, name::SMOOTH_MOVEMENT_DURATION, Value::Float(0.25));
        self.default_attr(h, name::X_VIEW, Value::Int(0));
        self.default_attr(h, name::Y_VIEW, Value::Int(0));
        self.default_attr(h, attr::MAX_COLUMNS, Value::Int(1));
        let mut b = bits::VIEW_STOPS_ANIMATION;
        for (a, bit) in BOOL_BITS {
            if self.node(h).is_some_and(|n| n.bool_attr(a)) {
                b |= bit;
            }
        }
        if let Some(l) = self.lb_mut(h) {
            l.bits = b;
        }
        self.set_attr(h, name::GOAL_X_VIEW, Value::Int(NO_GOAL));
        self.set_attr(h, name::GOAL_Y_VIEW, Value::Int(NO_GOAL));
        for a in [
            name::UP_BUTTON,
            name::DOWN_BUTTON,
            attr::LEFT_BUTTON,
            attr::RIGHT_BUTTON,
            attr::VERTICAL_SCROLLBAR,
            name::HORIZONTAL_SCROLLBAR,
        ] {
            if let Some(id) = self.node(h).and_then(|n| n.id_attr(a)) {
                self.listbox_id_changed(h, a, id);
            }
        }
        self.register_for_global_message(h, global::TIME);
        self.listbox_view_changed(h);
        self.listbox_update_and_draw(h);
    }

    /// After a yes-or-no attribute changed: a list turned horizontal is laid out again; each
    /// flag follows its attribute.
    // D2-11510
    pub(crate) fn listbox_bool_changed(&mut self, h: Handle, a: Token, v: bool) {
        if a == name::HORIZONTAL {
            self.listbox_update_layout(h);
        }
        if let Some((_, bit)) = BOOL_BITS.iter().find(|(t, _)| *t == a) {
            self.lb_set_bit(h, *bit, v);
        }
    }

    /// After an attribute naming another element changed: arrow buttons are listened to for
    /// their hot clicks, scroll bars for their positions, releases, lines and pages.
    // D2-11564
    pub(crate) fn listbox_id_changed(&mut self, h: Handle, a: Token, id: u32) {
        let mut changed = false;
        if a == name::UP_BUTTON
            || a == name::DOWN_BUTTON
            || a == attr::LEFT_BUTTON
            || a == attr::RIGHT_BUTTON
        {
            self.register_for_message(h, Token(id), msg::HOT_CLICK);
            changed = true;
        }
        if a == name::HORIZONTAL_SCROLLBAR {
            self.lb_set_bit(h, bits::HORIZONTAL_SCROLLBAR, true);
            for m in SCROLLBAR_MESSAGES_H {
                self.register_for_message(h, Token(id), m);
            }
            changed = true;
        }
        if a == attr::VERTICAL_SCROLLBAR {
            self.lb_set_bit(h, bits::HORIZONTAL_SCROLLBAR, false);
            for m in SCROLLBAR_MESSAGES_V {
                self.register_for_message(h, Token(id), m);
            }
            self.listbox_update_controls(h);
            return;
        }
        if changed {
            self.listbox_update_controls(h);
        }
    }

    /// After a whole-number attribute was set: a view is first kept on the sheet (and set again
    /// when that moved it); a new column count lays the sheet out; a new view (unless a smooth
    /// scroll set it) stops a smooth scroll and is applied; a goal view starts a smooth scroll,
    /// or is applied at once and forgotten.
    // D2-11563
    pub(crate) fn listbox_int_set(&mut self, h: Handle, a: Token, v: i32, changed: bool) {
        let is_view = a == name::X_VIEW || a == name::Y_VIEW;
        let is_goal = a == name::GOAL_X_VIEW || a == name::GOAL_Y_VIEW;
        if is_view || (is_goal && v != NO_GOAL) {
            let horizontal = a == name::X_VIEW || a == name::GOAL_X_VIEW;
            let kept = self.constrain_view_value(h, horizontal, v);
            if kept != v {
                self.set_attr(h, a, Value::Int(kept));
                return;
            }
        }
        if changed {
            if a == attr::MAX_COLUMNS {
                self.listbox_update_layout(h);
            }
            if is_view {
                if self.lb_test(h, bits::ANIMATING) && self.lb_test(h, bits::VIEW_STOPS_ANIMATION) {
                    self.listbox_stop_animation(h);
                }
                self.listbox_view_changed(h);
            }
        }
        if is_goal && v != NO_GOAL {
            if self.lb_test(h, bits::SMOOTH) {
                self.listbox_start_animation(h);
                return;
            }
            let view = if a == name::GOAL_X_VIEW {
                name::X_VIEW
            } else {
                name::Y_VIEW
            };
            self.set_attr(h, view, Value::Int(v));
            self.set_attr(h, a, Value::Int(NO_GOAL));
        }
    }

    // -----------------------------------------------------------------------------------------
    // The view
    // -----------------------------------------------------------------------------------------

    /// The view at the sheet's top left.
    // D2-11498
    pub fn list_home(&mut self, h: Handle) {
        self.set_attr(h, name::X_VIEW, Value::Int(0));
        self.set_attr(h, name::Y_VIEW, Value::Int(0));
    }

    /// The view as far along the sheet as it goes.
    // D2-11499
    pub fn list_end(&mut self, h: Handle) {
        let (w, hh) = self.lb(h).map_or((0, 0), |l| (l.paper_w, l.paper_h));
        self.set_attr(h, name::X_VIEW, Value::Int(w));
        self.set_attr(h, name::Y_VIEW, Value::Int(hh));
    }

    /// Scroll a list to a row: at once, or smoothly (a panel's own scrolling).
    pub fn list_scroll_to_row(&mut self, list: Handle, row: i32, at_once: bool) {
        self.listbox_scroll_to_row(list, row, at_once);
    }

    /// The topmost row in view (a panel keeping its place across a refill).
    #[must_use]
    pub fn list_top_row(&self, list: Handle) -> i32 {
        self.listbox_top_row(list)
    }

    /// Scroll a list a page down (`on`) or up, as its own page keys do (a panel's page buttons).
    pub fn list_page(&mut self, list: Handle, on: bool) {
        let s = if on {
            Scroll::VPageInc
        } else {
            Scroll::VPageDec
        };
        self.listbox_scroll(list, s);
    }

    /// The row at a point on the screen, by its cell on the sheet (a panel asking which row is
    /// first in view).
    #[must_use]
    pub fn list_index_at(&self, list: Handle, x: i32, y: i32) -> Option<usize> {
        self.listbox_index_at(list, x, y)
    }

    /// Scroll to a row: at once, or as a goal.
    // D2-11500
    pub(crate) fn listbox_scroll_to_row(&mut self, h: Handle, row: i32, at_once: bool) {
        if self.is_msize(h) {
            return self.msize_scroll_to_row(h, row, at_once);
        }
        let ih = self.lb(h).map_or(0, |l| l.item_h);
        let a = if at_once {
            name::Y_VIEW
        } else {
            name::GOAL_Y_VIEW
        };
        self.set_attr(h, a, Value::Int(ih * row));
    }

    /// Scroll to a column: at once, or as a goal.
    // D2-11501
    pub(crate) fn listbox_scroll_to_col(&mut self, h: Handle, col: i32, at_once: bool) {
        if self.is_msize(h) {
            return self.msize_scroll_to_col(h, col, at_once);
        }
        let iw = self.lb(h).map_or(0, |l| l.item_w);
        let a = if at_once {
            name::X_VIEW
        } else {
            name::GOAL_X_VIEW
        };
        self.set_attr(h, a, Value::Int(iw * col));
    }

    /// The leftmost column in view.
    // D2-11502
    pub(crate) fn listbox_left_col(&self, h: Handle) -> i32 {
        self.lb(h)
            .map_or(0, |l| if l.item_w < 1 { 0 } else { l.x_view / l.item_w })
    }

    /// The topmost row in view.
    // D2-11503
    pub(crate) fn listbox_top_row(&self, h: Handle) -> i32 {
        self.lb(h)
            .map_or(0, |l| if l.item_h < 1 { 0 } else { l.y_view / l.item_h })
    }

    /// Keep the view's attributes on the sheet; returns whether the vertical one moved.
    // D2-11504
    fn listbox_keep_view_in_bounds(&mut self, h: Handle) -> bool {
        let x = self.int_or(h, name::X_VIEW, 0);
        let kx = self.constrain_view_value(h, true, x);
        if kx != x {
            self.set_attr(h, name::X_VIEW, Value::Int(kx));
        }
        let y = self.int_or(h, name::Y_VIEW, 0);
        let ky = self.constrain_view_value(h, false, y);
        if ky != y {
            self.set_attr(h, name::Y_VIEW, Value::Int(ky));
            return true;
        }
        false
    }

    /// A view value kept on the sheet: not below 0, not past the sheet's length, and (unless the
    /// list scrolls smoothly) on a cell's edge, to the nearest.
    // D2-11530
    pub(crate) fn constrain_view_value(&self, h: Handle, horizontal: bool, v: i32) -> i32 {
        if self.is_msize(h) {
            return self.msize_constrain_view_value(h, horizontal, v);
        }
        let Some(l) = self.lb(h) else { return v };
        let (size, max) = if horizontal {
            (l.item_w, l.paper_w)
        } else {
            (l.item_h, l.paper_h)
        };
        if size == 0 || v < 0 {
            return 0;
        }
        let mut v = v.min(max);
        if l.bits & bits::SMOOTH == 0 {
            v = ((size / 2 + v) / size) * size;
        }
        v
    }

    /// The vertical view's share of the way down the sheet's hidden part (0 when all shows).
    // D2-11531
    pub(crate) fn listbox_percentage_v(&self, h: Handle) -> f32 {
        let Some(n) = self.node(h) else { return 0.0 };
        let l = &n.lists.list;
        let height = n.rect.height();
        if l.paper_h <= height {
            return 0.0;
        }
        #[allow(clippy::cast_precision_loss)]
        let p = l.y_view as f32 / (l.paper_h - height) as f32;
        p.clamp(0.0, 1.0)
    }

    /// The horizontal view's share of the way along the sheet's hidden part.
    // D2-11532
    pub(crate) fn listbox_percentage_h(&self, h: Handle) -> f32 {
        let Some(n) = self.node(h) else { return 0.0 };
        let l = &n.lists.list;
        let width = n.rect.width();
        if l.paper_w <= width {
            return 0.0;
        }
        #[allow(clippy::cast_precision_loss)]
        let p = l.x_view as f32 / (l.paper_w - width) as f32;
        p.clamp(0.0, 1.0)
    }

    /// The view's attributes applied: when they moved, the sheet is laid out again, the controls
    /// follow, and the list says its view changed.
    // D2-11559
    fn listbox_view_changed(&mut self, h: Handle) {
        let x = self.int_or(h, name::X_VIEW, 0);
        let y = self.int_or(h, name::Y_VIEW, 0);
        let Some(l) = self.lb_mut(h) else { return };
        let moved = l.x_view != x || l.y_view != y;
        l.x_view = x;
        l.y_view = y;
        if moved {
            self.listbox_update_and_draw(h);
            self.broadcast(h, name::NEW_VIEW, 0);
        }
    }

    /// A goal from a scroll by line or page: from the view (or the goal under way), `delta` on,
    /// no further than `limit` (not past it going on, not before it going back).
    // D2-11511
    pub(crate) fn listbox_new_goal(
        &mut self,
        h: Handle,
        from: i32,
        delta: i32,
        on: bool,
        limit: i32,
        goal: Token,
    ) {
        let mut cur = from;
        if self.lb_test(h, bits::ANIMATING) {
            cur = self.int_or(h, goal, cur);
        }
        let sum = cur + delta;
        let v = if on { sum.min(limit) } else { sum.max(limit) };
        self.set_attr(h, goal, Value::Int(v));
    }

    /// A scroll by line or page, as a goal: a page back is the view's length less a row, a page
    /// on likewise; never before the sheet's start or past its last full view.
    // D2-11541, D2-11542, D2-11543, D2-11544, D2-11545, D2-11546, D2-11547, D2-11548
    pub(crate) fn listbox_scroll(&mut self, h: Handle, s: Scroll) {
        if self.is_msize(h) {
            return self.msize_scroll(h, s);
        }
        let Some(n) = self.node(h) else { return };
        let l = &n.lists.list;
        let (w, hh) = (n.rect.width(), n.rect.height());
        let animating = l.bits & bits::ANIMATING != 0;
        let (goal, view, step, page, paper) = match s {
            Scroll::VPageDec | Scroll::VPageInc | Scroll::VLineDec | Scroll::VLineInc => {
                (name::GOAL_Y_VIEW, l.y_view, l.item_h, hh, l.paper_h)
            }
            _ => (name::GOAL_X_VIEW, l.x_view, l.item_w, w, l.paper_w),
        };
        let mut v = view;
        if animating {
            v = self.int_or(h, goal, v);
        }
        let last = paper - page;
        let v = match s {
            Scroll::VPageDec | Scroll::HPageDec => (v + step - page).max(0),
            Scroll::VPageInc | Scroll::HPageInc => (v + page - step).min(last),
            Scroll::VLineDec | Scroll::HLineDec => (v - step).max(0),
            Scroll::VLineInc | Scroll::HLineInc => (v + step).min(last),
        };
        self.set_attr(h, goal, Value::Int(v));
    }

    /// Show a row: scroll (at once, or as a goal) so its cell is inside the view.
    // D2-11525, D2-11509
    pub(crate) fn listbox_scroll_to_show(&mut self, h: Handle, index: usize, at_once: bool) {
        if self.is_msize(h) {
            return self.msize_scroll_to_show(h, index, at_once);
        }
        let Some(n) = self.node(h) else { return };
        let (w, hh) = (n.rect.width(), n.rect.height());
        let l = &n.lists.list;
        let (iw, ih, xv, yv) = (l.item_w, l.item_h, l.x_view, l.y_view);
        let i = i32::try_from(index).unwrap_or(-1);
        let (Some(col), Some(row)) = (self.listbox_col(h, i), self.listbox_row(h, i)) else {
            return;
        };
        if iw * col < xv {
            self.listbox_scroll_to_col(h, col, at_once);
        } else if 0 < iw && self.listbox_left_col(h) + w / iw <= col {
            self.listbox_scroll_to_col(h, col - w / iw + 1, at_once);
        }
        if ih * row < yv {
            self.listbox_scroll_to_row(h, row, at_once);
        } else if 0 < ih && self.listbox_top_row(h) + hh / ih <= row {
            self.listbox_scroll_to_row(h, row - hh / ih + 1, at_once);
        }
    }

    /// A row's column in the grid (by its place in the list).
    // D2-11523
    pub(crate) fn listbox_col(&self, h: Handle, i: i32) -> Option<i32> {
        let l = self.lb(h)?;
        if l.rows < 1 || l.cols < 1 {
            return Some(0);
        }
        if i == -1 || self.list_items(h).is_empty() {
            return None;
        }
        Some(if l.bits & bits::HORIZONTAL != 0 {
            i % l.cols
        } else {
            i / l.rows
        })
    }

    /// A row's row in the grid (by its place in the list).
    // D2-11524, D2-02705
    pub(crate) fn listbox_row(&self, h: Handle, i: i32) -> Option<i32> {
        let l = self.lb(h)?;
        if l.rows < 1 || l.cols < 1 {
            return Some(0);
        }
        if i == -1 || self.list_items(h).is_empty() {
            return None;
        }
        Some(if l.bits & bits::HORIZONTAL != 0 {
            i / l.cols
        } else {
            i % l.rows
        })
    }

    // -----------------------------------------------------------------------------------------
    // Smooth scrolling
    // -----------------------------------------------------------------------------------------

    /// Start (or restart) a smooth scroll toward the goal view: from the view now, over the
    /// list's smooth-movement time; a scroll already under way restarts as if half done, from
    /// where it would have been, so the motion stays smooth.
    // D2-11513
    fn listbox_start_animation(&mut self, h: Handle) {
        let duration = self.float_or(h, name::SMOOTH_MOVEMENT_DURATION, 0.0);
        if duration == 0.0 {
            return;
        }
        let now = self.now();
        let gx = self.int_or(h, name::GOAL_X_VIEW, NO_GOAL);
        let gy = self.int_or(h, name::GOAL_Y_VIEW, NO_GOAL);
        let Some(l) = self.lb_mut(h) else { return };
        l.anim_end_x = if gx == NO_GOAL { l.x_view } else { gx };
        l.anim_end_y = if gy == NO_GOAL { l.y_view } else { gy };
        if l.bits & bits::ANIMATING != 0 {
            l.bits |= bits::RESTARTED;
            l.anim_start_x = l.x_view * 2 - l.anim_end_x;
            l.anim_start_y = l.y_view * 2 - l.anim_end_y;
            l.anim_start = now - f64::from(duration) * 0.5;
        } else {
            l.anim_start = now;
            l.anim_start_x = l.x_view;
            l.anim_start_y = l.y_view;
            l.bits = (l.bits & !bits::RESTARTED) | bits::ANIMATING;
        }
        l.anim_end = f64::from(duration) + l.anim_start;
    }

    /// Stop a smooth scroll: no goal any more.
    // D2-11514
    fn listbox_stop_animation(&mut self, h: Handle) {
        self.set_attr(h, name::GOAL_X_VIEW, Value::Int(NO_GOAL));
        self.set_attr(h, name::GOAL_Y_VIEW, Value::Int(NO_GOAL));
        self.lb_set_bit(h, bits::ANIMATING, false);
    }

    /// The clock: a smooth scroll moves the view its share of the way from start to goal; done,
    /// it forgets the goal.
    // D2-11539
    fn listbox_global_loop(&mut self, h: Handle) {
        if !self.lb_test(h, bits::ANIMATING) {
            return;
        }
        let now = self.now();
        let Some(l) = self.lb(h) else { return };
        let span = l.anim_end - l.anim_start;
        let mut t = if span > 0.0 {
            (now - l.anim_start) / span
        } else {
            1.0
        };
        let (sx, sy, ex, ey) = (l.anim_start_x, l.anim_start_y, l.anim_end_x, l.anim_end_y);
        if t >= 1.0 {
            t = 1.0;
            self.set_attr(h, name::GOAL_X_VIEW, Value::Int(NO_GOAL));
            self.set_attr(h, name::GOAL_Y_VIEW, Value::Int(NO_GOAL));
            self.lb_set_bit(h, bits::ANIMATING, false);
        }
        self.lb_set_bit(h, bits::VIEW_STOPS_ANIMATION, false);
        let x = sx + dereth_primitives::num::to_i32_f64(f64::from(ex - sx) * t);
        let y = sy + dereth_primitives::num::to_i32_f64(f64::from(ey - sy) * t);
        self.set_attr(h, name::X_VIEW, Value::Int(x));
        self.set_attr(h, name::Y_VIEW, Value::Int(y));
        self.lb_set_bit(h, bits::VIEW_STOPS_ANIMATION, true);
    }

    // -----------------------------------------------------------------------------------------
    // Laying out
    // -----------------------------------------------------------------------------------------

    /// Lay the rows out on the sheet: `max_columns` columns (at least 1) and as many rows as the
    /// rows need, each cell the first row's size; a vertical list fills down its columns, a
    /// horizontal one across its rows; each row is put inside the list (when it is not yet) at
    /// its cell less the view.
    // D2-11512
    pub(crate) fn listbox_update_layout(&mut self, h: Handle) {
        if self.is_msize(h) {
            return self.msize_update_layout(h);
        }
        if !self.lb_test(h, bits::ALLOW_UPDATING) {
            return;
        }
        let items = self.list_items(h);
        let horizontal = self.lb_test(h, bits::HORIZONTAL);
        // An emptied list keeps the size of its cells, and one cell of sheet.
        let (mut iw, mut ih) = self.lb(h).map_or((0, 0), |l| (l.item_w, l.item_h));
        let (mut cols, mut rows) = (1, 1);
        if !items.is_empty() {
            let maxc = self.int_or(h, attr::MAX_COLUMNS, 1).max(1);
            self.listbox_adopt_items(h, &items);
            if let Some(f) = self.node(items[0]) {
                (iw, ih) = (f.rect.width(), f.rect.height());
            }
            let count = i32::try_from(items.len()).unwrap_or(i32::MAX);
            cols = maxc;
            rows = ((count + maxc - 1) / maxc).max(1);
            let (xv, yv) = self.lb(h).map_or((0, 0), |l| (l.x_view, l.y_view));
            let (mut col, mut row) = (0, 0);
            for it in &items {
                self.place_at(*it, iw * col - xv, ih * row - yv);
                if horizontal {
                    if col == cols - 1 {
                        col = 0;
                        row += 1;
                    } else {
                        col += 1;
                    }
                } else if row == rows - 1 {
                    row = 0;
                    col += 1;
                } else {
                    row += 1;
                }
            }
        }
        if let Some(l) = self.lb_mut(h) {
            l.cols = cols;
            l.rows = rows;
            l.item_w = iw;
            l.item_h = ih;
            l.paper_w = iw * cols;
            l.paper_h = ih * rows;
        }
    }

    /// Each row inside the list (when it is not yet) and placed by its own edges there.
    pub(crate) fn listbox_adopt_items(&mut self, h: Handle, items: &[Handle]) {
        for it in items {
            if self.node(*it).and_then(|n| n.parent) != Some(h) {
                self.set_parent(*it, h);
            }
            self.place(*it);
        }
    }

    /// Put an element at a place inside its parent, keeping its size, and its children where
    /// their edges put them.
    pub(crate) fn place_at(&mut self, h: Handle, x: i32, y: i32) {
        if let Some(n) = self.node_mut(h) {
            let (w, hh) = (n.rect.width(), n.rect.height());
            n.rect = crate::Rect::sized(x, y, w, hh);
            n.sized = true;
        }
        self.place_children(h);
    }

    /// The list's controls in step with its view: its scroll bar (unless the scroll bar moved the
    /// view) sized to the share of the sheet in view, at the view's place, disabled when all of
    /// the sheet shows; its arrow buttons ghosted at the sheet's ends.
    // D2-11549
    pub(crate) fn listbox_update_controls(&mut self, h: Handle) {
        if !self.lb_test(h, bits::ALLOW_CONTROL_UPDATING) {
            return;
        }
        let Some(n) = self.node(h) else { return };
        let l = &n.lists.list;
        let horizontal_bar = l.bits & bits::HORIZONTAL_SCROLLBAR != 0;
        let (mut paper, view) = if horizontal_bar {
            (l.paper_w, n.rect.width())
        } else {
            (l.paper_h, n.rect.height())
        };
        let (xv, yv) = (l.x_view, l.y_view);
        let from_bar = l.bits & bits::FROM_SCROLLBAR != 0;
        if !from_bar {
            if let Some(sb) = self.listbox_scrollbar(h) {
                let all_shows = paper <= view;
                let whole = if all_shows { view } else { paper };
                self.set_attr(sb, attr::ALLOW_UPDATING, Value::Bool(false));
                self.set_attr(sb, attr::DISABLED, Value::Bool(false));
                #[allow(clippy::cast_precision_loss)]
                let share = if paper > 0 {
                    view as f32 / paper as f32
                } else {
                    1.0
                };
                let (size, pos, at) = if horizontal_bar {
                    (
                        attr::X_WIDGET_SIZE,
                        attr::X_POSITION,
                        self.listbox_percentage_h(h),
                    )
                } else {
                    (
                        attr::Y_WIDGET_SIZE,
                        attr::Y_POSITION,
                        self.listbox_percentage_v(h),
                    )
                };
                self.set_attr(sb, size, Value::Float(share.min(1.0)));
                self.set_attr(sb, pos, Value::Float(at));
                self.set_attr(sb, attr::ALLOW_UPDATING, Value::Bool(true));
                self.lay_out_scrollbar(sb);
                if all_shows {
                    self.set_attr(sb, attr::DISABLED, Value::Bool(true));
                }
                paper = whole;
            }
        }
        self.listbox_ghost_button(h, name::UP_BUTTON, yv == 0);
        self.listbox_ghost_button(h, name::DOWN_BUTTON, paper <= yv + view);
        self.listbox_ghost_button(h, attr::LEFT_BUTTON, xv == 0);
        self.listbox_ghost_button(h, attr::RIGHT_BUTTON, paper <= xv + view);
    }

    /// Ghost (or not) the arrow button an attribute names.
    // D2-11505
    pub(crate) fn listbox_ghost_button(&mut self, h: Handle, a: Token, ghost: bool) {
        if let Some(b) = self.named_element(h, a) {
            self.set_attr(b, attr::GHOSTED, Value::Bool(ghost));
        }
    }

    /// The list's scroll bar: the vertical one it names, else the horizontal one.
    // D2-11506
    pub(crate) fn listbox_scrollbar(&self, h: Handle) -> Option<Handle> {
        let n = self.node(h)?;
        let a = if n.id_attr(attr::VERTICAL_SCROLLBAR).is_some() {
            attr::VERTICAL_SCROLLBAR
        } else if n.id_attr(name::HORIZONTAL_SCROLLBAR).is_some() {
            name::HORIZONTAL_SCROLLBAR
        } else {
            return None;
        };
        self.named_element(h, a)
    }

    /// A list's rows laid out again and its controls brought in step with its view, for a row whose
    /// height changed. Nothing else under the list's window is touched.
    // D2-11550
    pub fn list_update_and_draw(&mut self, list: Handle) {
        self.listbox_update_and_draw(list);
    }

    /// Lay out, keep the view on the sheet, update the controls.
    // D2-11550
    pub(crate) fn listbox_update_and_draw(&mut self, h: Handle) {
        self.listbox_update_layout(h);
        self.listbox_keep_view_in_bounds(h);
        self.listbox_update_controls(h);
    }

    /// A list going away stops selecting by drag and showing a dragged item, and stops listening
    /// to its arrows and scroll bars.
    // D2-11536, D2-11561, D2-11587
    pub(crate) fn listbox_deleting(&mut self, h: Handle) {
        self.listbox_end_drag_select(h);
        self.listbox_end_rollover(h);
        for a in [
            name::UP_BUTTON,
            name::DOWN_BUTTON,
            attr::LEFT_BUTTON,
            attr::RIGHT_BUTTON,
        ] {
            if let Some(id) = self.node(h).and_then(|n| n.id_attr(a)) {
                self.unregister_for_message(h, Token(id), msg::HOT_CLICK);
            }
        }
        self.listbox_unregister_scrollbar(h, name::HORIZONTAL_SCROLLBAR);
        self.listbox_unregister_scrollbar(h, attr::VERTICAL_SCROLLBAR);
    }

    /// Stop hearing a scroll bar the list names.
    // D2-11507
    fn listbox_unregister_scrollbar(&mut self, h: Handle, which: Token) {
        let Some(id) = self.node(h).and_then(|n| n.id_attr(which)) else {
            return;
        };
        let messages = if which == name::HORIZONTAL_SCROLLBAR {
            SCROLLBAR_MESSAGES_H
        } else {
            SCROLLBAR_MESSAGES_V
        };
        for m in messages {
            self.unregister_for_message(h, Token(id), m);
        }
    }

    /// Resized: laid out again.
    // D2-11551
    pub(crate) fn listbox_resized(&mut self, h: Handle) {
        self.listbox_update_and_draw(h);
    }

    /// A new state: the list's rows are its children again (vertical lists by column then row,
    /// horizontal ones by row then column, of where they stand), then laid out.
    // D2-11562
    pub(crate) fn listbox_set_state(&mut self, h: Handle, s: Token) -> bool {
        let r = self.set_state_base(h, s);
        let horizontal = self.lb_test(h, bits::HORIZONTAL);
        let kids = self.node(h).map(|n| n.children.clone()).unwrap_or_default();
        let mut items: Vec<Handle> = Vec::new();
        for k in kids {
            let Some(kr) = self.node(k).map(|n| n.rect) else {
                continue;
            };
            let at = items.iter().position(|i| {
                let Some(ir) = self.node(*i).map(|n| n.rect) else {
                    return false;
                };
                if horizontal {
                    kr.y0 < ir.y0 || (kr.y0 == ir.y0 && kr.x0 < ir.x0)
                } else {
                    kr.x0 < ir.x0 || (kr.x0 == ir.x0 && kr.y0 < ir.y0)
                }
            });
            match at {
                Some(at) => items.insert(at, k),
                None => items.push(k),
            }
        }
        if let Some(n) = self.node_mut(h) {
            n.list.items = items;
        }
        self.sync_selected_index(h);
        if self.node(h).is_some_and(|n| n.ready) {
            self.listbox_update_and_draw(h);
        }
        r
    }

    // -----------------------------------------------------------------------------------------
    // Rows
    // -----------------------------------------------------------------------------------------

    /// Add a row at the end (it is put inside the list when the list is laid out).
    // D2-11571
    pub fn list_add(&mut self, list: Handle, item: Handle) {
        if let Some(n) = self.node_mut(list) {
            n.list.items.push(item);
        }
        if !self.lb_test(list, bits::MULTI_OP) {
            self.listbox_update_and_draw(list);
        }
    }

    /// Insert a row at `index` (past the end of the list, no row is added).
    // D2-11572, D2-11224
    pub fn list_insert(&mut self, list: Handle, item: Handle, index: usize) {
        if let Some(n) = self.node_mut(list) {
            if index <= n.list.items.len() {
                n.list.items.insert(index, item);
            }
        }
        self.sync_selected_index(list);
        if !self.lb_test(list, bits::MULTI_OP) {
            self.listbox_update_and_draw(list);
        }
    }

    /// Insert a row before another row of the list, at the end with no other named (and not at
    /// all when the one named is not one of its rows).
    // D2-11519
    pub fn list_insert_before(&mut self, list: Handle, item: Handle, before: Option<Handle>) {
        let at = match before {
            Some(b) => self.list_index_of(list, b).unwrap_or(usize::MAX),
            None => self.node(list).map_or(0, |n| n.list.items.len()),
        };
        self.list_insert(list, item, at);
    }

    /// Add a row as part of a change of several rows: the list waits to lay itself out until
    /// [`Ui::list_changes_done`].
    // D2-11520
    pub fn list_multi_add(&mut self, list: Handle, item: Handle) {
        self.lb_set_bit(list, bits::MULTI_OP, true);
        self.list_add(list, item);
    }

    /// Insert a row as part of a change of several rows.
    // D2-11521, D2-11522
    pub fn list_multi_insert(&mut self, list: Handle, item: Handle, index: usize) {
        self.lb_set_bit(list, bits::MULTI_OP, true);
        self.list_insert(list, item, index);
    }

    /// Take a row out as part of a change of several rows.
    // D2-11555, D2-11574
    pub fn list_multi_remove(&mut self, list: Handle, index: usize) -> Option<Handle> {
        self.lb_set_bit(list, bits::MULTI_OP, true);
        self.list_remove(list, index)
    }

    /// A change of several rows done: laid out now.
    // D2-11556
    pub fn list_changes_done(&mut self, list: Handle) {
        self.lb_set_bit(list, bits::MULTI_OP, false);
        self.listbox_update_and_draw(list);
    }

    /// Take the row at `index` out of the list (keeping the element, which stands nowhere until
    /// it is put somewhere): no longer selected or under a dragged item.
    // D2-11554
    pub fn list_remove(&mut self, list: Handle, index: usize) -> Option<Handle> {
        let item = self.list_items(list).get(index).copied()?;
        if let Some(n) = self.node_mut(list) {
            n.list.items.remove(index);
            let l = &mut n.lists.list;
            if l.selected == Some(item) {
                l.selected = None;
            }
            if l.drag_over == Some(item) {
                l.drag_over = None;
            }
            n.children.retain(|c| *c != item);
        }
        if let Some(n) = self.node_mut(item).filter(|n| n.parent == Some(list)) {
            n.parent = None;
        }
        self.sync_selected_index(list);
        if !self.lb_test(list, bits::MULTI_OP) {
            self.listbox_update_and_draw(list);
        }
        Some(item)
    }

    /// Remove a row and the element with it.
    // D2-11573
    pub fn list_delete(&mut self, list: Handle, item: Handle) -> bool {
        let Some(i) = self.list_index_of(list, item) else {
            return false;
        };
        if let Some(r) = self.list_remove(list, i) {
            self.remove(r);
            return true;
        }
        false
    }

    /// Put a row in place of the row at `index`.
    // D2-11579
    pub fn list_set_item(&mut self, list: Handle, item: Handle, index: usize) {
        let multi = self.lb_test(list, bits::MULTI_OP);
        if let Some(old) = self.list_items(list).get(index).copied() {
            self.list_delete(list, old);
        }
        self.list_insert(list, item, index);
        self.lb_set_bit(list, bits::MULTI_OP, multi);
        self.listbox_update_and_draw(list);
    }

    /// Empty a list box: every row removed, nothing selected, the view at the top left.
    // D2-11557
    pub fn list_flush(&mut self, list: Handle) {
        for i in self.list_items(list) {
            self.remove(i);
        }
        if let Some(n) = self.node_mut(list) {
            n.list = ListState::default();
            n.lists.list.selected = None;
            n.lists.list.drag_over = None;
        }
        self.set_attr(list, name::X_VIEW, Value::Int(0));
        self.set_attr(list, name::Y_VIEW, Value::Int(0));
        self.listbox_update_and_draw(list);
    }

    /// The row at `index`.
    // D2-11515
    #[must_use]
    pub fn list_item(&self, list: Handle, index: usize) -> Option<Handle> {
        self.list_items(list).get(index).copied()
    }

    // -----------------------------------------------------------------------------------------
    // Selection
    // -----------------------------------------------------------------------------------------

    /// Select a row (none when `item` is not one of the list's): the row selected before takes
    /// the list's normal row state and the new one its selected state (when the list changes its
    /// rows' states); with `notify`, the list says so (with the row's place); then laid out.
    // D2-11516
    pub fn list_select_item(&mut self, list: Handle, item: Option<Handle>, notify: bool) {
        let before = self.lb(list).and_then(|l| l.selected);
        if before == item {
            return;
        }
        let index = item.and_then(|i| self.list_index_of(list, i));
        let now = index.and(item);
        if let Some(l) = self.lb_mut(list) {
            l.selected = now;
        }
        self.sync_selected_index(list);
        if self.lb_test(list, bits::ITEM_STATE_CHANGE) {
            let (normal, selected) = self.node(list).map_or((None, None), |n| {
                (
                    n.id_attr(attr::ITEM_NORMAL_STATE),
                    n.id_attr(attr::ITEM_SELECTED_STATE),
                )
            });
            if let Some(b) = before {
                self.set_state(b, Token(normal.unwrap_or(0)));
            }
            if let Some(a) = now {
                self.set_state(a, Token(selected.unwrap_or(0)));
            }
            self.flush_pending();
        }
        if notify {
            let p = index.map_or(u32::MAX, |i| u32::try_from(i).unwrap_or(u32::MAX));
            self.broadcast(list, msg::NEW_SELECTION, p);
        }
        self.listbox_update_layout(list);
    }

    /// Select the row at `index` (none past the end) and scroll it into view, without saying so.
    pub fn list_select(&mut self, list: Handle, index: usize) {
        self.list_select_index(list, index, false);
    }

    /// Select the row at `index` (none past the end) and scroll it into view; with `notify`, the
    /// list says so.
    // D2-11517
    pub fn list_select_index(&mut self, list: Handle, index: usize, notify: bool) {
        match self.list_item(list, index) {
            Some(item) => {
                self.list_select_item(list, Some(item), notify);
                self.listbox_scroll_to_show(list, index, true);
            }
            None => self.list_select_item(list, None, notify),
        }
    }

    /// Select the row named `id` (with `notify`, the list says so) and select it again by its
    /// place, which scrolls it into view.
    // D2-11552
    pub fn list_select_by_name(&mut self, list: Handle, id: Token, notify: bool) {
        let items = self.list_items(list);
        let Some(i) = items
            .iter()
            .position(|it| self.node(*it).is_some_and(|n| n.id == id))
        else {
            self.list_select_item(list, None, notify);
            return;
        };
        self.list_select_item(list, Some(items[i]), notify);
        self.list_select_index(list, i, true);
    }

    /// The row under the point `(x, y)` on the screen, by its cell on the sheet.
    // D2-11518
    pub(crate) fn listbox_index_at(&self, h: Handle, x: i32, y: i32) -> Option<usize> {
        if self.is_msize(h) {
            return self.msize_index_at(h, x, y);
        }
        let r = self.screen_rect(h)?;
        if !r.contains(x, y) {
            return None;
        }
        let l = self.lb(h)?;
        if l.item_w <= 0 || l.item_h <= 0 {
            return None;
        }
        let px = l.x_view + x - r.x0;
        let py = l.y_view + y - r.y0;
        if px >= l.cols * l.item_w || py >= l.rows * l.item_h {
            return None;
        }
        let (c, rw) = (px / l.item_w, py / l.item_h);
        let i = if l.bits & bits::HORIZONTAL != 0 {
            l.cols * rw + c
        } else {
            l.rows * c + rw
        };
        let i = usize::try_from(i).ok()?;
        (i < self.list_items(h).len()).then_some(i)
    }

    /// The row under the pointer.
    // D2-11553
    pub fn listbox_item_under_mouse(&self, h: Handle) -> Option<Handle> {
        let (x, y) = self.pointer();
        let i = self.listbox_index_at(h, x, y)?;
        self.list_item(h, i)
    }

    /// A press that selects: the row under the pointer, when the list selects by clicks; a list
    /// that selects by dragging starts doing so.
    // D2-11560
    fn listbox_handle_selection(&mut self, h: Handle) {
        if !self.lb_test(h, bits::CLICK_SELECT) {
            return;
        }
        if let Some(item) = self.listbox_item_under_mouse(h) {
            self.list_select_item(h, Some(item), true);
            if self.lb_test(h, bits::DRAG_SELECT) {
                self.listbox_start_drag_select(h);
            }
        }
    }

    /// Selecting by drag starts: the list listens for its top-level element's pointer moves
    /// (which the manager never sends, so the row pressed stays the one selected) and for the
    /// release.
    // D2-11526
    fn listbox_start_drag_select(&mut self, h: Handle) {
        if self.lb_test(h, bits::DRAG_SELECTING) {
            return;
        }
        self.lb_set_bit(h, bits::DRAG_SELECTING, true);
        let root = self.root_of(h);
        self.register_for_object_message(h, root, global::MOUSE_MOVE);
        self.register_for_global_message(h, global::MOUSE_UP);
    }

    // D2-11527
    fn listbox_end_drag_select(&mut self, h: Handle) {
        if !self.lb_test(h, bits::DRAG_SELECTING) {
            return;
        }
        self.lb_set_bit(h, bits::DRAG_SELECTING, false);
        let root = self.root_of(h);
        self.unregister_for_object_message(h, root, global::MOUSE_MOVE);
        self.unregister_for_global_message(h, global::MOUSE_UP);
    }

    /// While an item is dragged over a container list: hear the pointer's moves and the release.
    // D2-11528
    fn listbox_start_rollover(&mut self, h: Handle) {
        if !self.lb_test(h, bits::DRAG_ROLLOVER) || self.lb_test(h, bits::ROLLOVER) {
            return;
        }
        self.lb_set_bit(h, bits::ROLLOVER, true);
        self.register_for_global_message(h, global::MOUSE_MOVE);
        self.register_for_global_message(h, global::MOUSE_UP);
    }

    /// Stop that: the row the item was over gets its state back.
    // D2-11529
    fn listbox_end_rollover(&mut self, h: Handle) {
        if !self.lb_test(h, bits::ROLLOVER) {
            return;
        }
        self.lb_set_bit(h, bits::ROLLOVER, false);
        self.unregister_for_global_message(h, global::MOUSE_MOVE);
        self.unregister_for_global_message(h, global::MOUSE_UP);
        let over = self.lb_mut(h).and_then(|l| l.drag_over.take());
        if let Some(o) = over {
            let s = self.lb(h).map_or(Token(0), |l| l.drag_over_state);
            self.set_state(o, s);
            self.flush_pending();
        }
    }

    /// The row under a dragged item shows whether the list would take it; the row it left gets
    /// its state back.
    // D2-11558
    fn listbox_rollover(&mut self, h: Handle) {
        let Some(dragged) = self.dragged() else {
            self.listbox_end_rollover(h);
            return;
        };
        let refuses = self
            .node(h)
            .is_some_and(|n| n.bool_attr(attr::DISALLOW_DRAG_IN));
        let s = if self.would_catch(h, dragged) && !refuses {
            state::DRAG_ROLLOVER_ACCEPT
        } else {
            state::DRAG_ROLLOVER_REJECT
        };
        let under = self.listbox_item_under_mouse(h);
        let before = self.lb(h).and_then(|l| l.drag_over);
        if before == under {
            return;
        }
        if let Some(b) = before {
            let back = self.lb(h).map_or(Token(0), |l| l.drag_over_state);
            self.set_state(b, back);
            if let Some(l) = self.lb_mut(h) {
                l.drag_over = None;
            }
        }
        if let Some(u) = under {
            let now = self.node(u).map_or(Token(0), |n| n.state);
            if now != s {
                if let Some(l) = self.lb_mut(h) {
                    l.drag_over_state = now;
                    l.drag_over = Some(u);
                }
                self.set_state(u, s);
            }
        }
        self.flush_pending();
    }

    /// Move the selection by `delta` rows (wrapping past either end), and scroll it into view.
    // D2-11540
    fn listbox_start_active(&mut self, h: Handle, delta: i32) -> bool {
        let n = i32::try_from(self.list_items(h).len()).unwrap_or(0);
        let cur = self
            .list_selected_index(h)
            .map_or(-1, |i| i32::try_from(i).unwrap_or(-1));
        let mut i = cur + delta;
        if i < 0 {
            i = n - 1;
        }
        let valid = if n <= i {
            i = 0;
            n != 0
        } else {
            i < n
        };
        if valid {
            let i = usize::try_from(i).unwrap_or(0);
            let item = self.list_item(h, i);
            self.list_select_item(h, item, true);
            self.listbox_scroll_to_show(h, i, true);
        } else {
            self.list_select_item(h, None, true);
        }
        true
    }

    /// The arrow keys with the keyboard on the list: up and down move a row (in a horizontal
    /// list, a whole row of columns), left and right a column (in a vertical list, a whole
    /// column of rows).
    // D2-11567, D2-11568, D2-11569, D2-11570
    pub(crate) fn listbox_key(&mut self, h: Handle, key: u32) -> bool {
        use crate::types::key_id;
        let (horizontal, cols, rows) = self.lb(h).map_or((false, 1, 1), |l| {
            (l.bits & bits::HORIZONTAL != 0, l.cols, l.rows)
        });
        let delta = match key {
            key_id::UP if horizontal => -cols,
            key_id::UP => -1,
            key_id::DOWN if horizontal => cols,
            key_id::DOWN => 1,
            key_id::LEFT if horizontal => -1,
            key_id::LEFT => -rows,
            key_id::RIGHT if horizontal => 1,
            key_id::RIGHT => rows,
            _ => return false,
        };
        self.listbox_start_active(h, delta)
    }

    // -----------------------------------------------------------------------------------------
    // The pointer and messages
    // -----------------------------------------------------------------------------------------

    /// A press on the list: the left button (or its double press) selects; the wheel goes to the
    /// list's scroll bar; either stops the press here unless the list passes presses through.
    // D2-11565
    pub(crate) fn listbox_mouse_down(
        &mut self,
        h: Handle,
        x: i32,
        y: i32,
        b: u32,
        handled: &mut bool,
    ) {
        self.element_mouse_down(h, x, y, b, handled);
        if self.node(h).is_some_and(|n| n.moving || n.resizing) {
            return;
        }
        if b == button_id::LEFT || b == button_id::DOUBLE_LEFT {
            self.listbox_handle_selection(h);
        } else if b == button_id::WHEEL_UP || b == button_id::WHEEL_DOWN {
            self.listbox_mouse_wheel(h, x, y, b);
        } else {
            return;
        }
        if !self
            .node(h)
            .is_some_and(|n| n.bool_attr(attr::MOUSE_EVENT_PASSTHROUGH))
        {
            *handled = true;
        }
    }

    /// The wheel over the list goes to its scroll bar, when the pointer is not on the bar already.
    // D2-11533
    fn listbox_mouse_wheel(&mut self, h: Handle, x: i32, y: i32, b: u32) {
        if let Some(sb) = self.listbox_scrollbar(h) {
            if !self.node(sb).is_some_and(|n| n.over) {
                let mut handled = false;
                self.mouse_down(sb, x, y, b, &mut handled);
            }
        }
    }

    /// Topmost under the pointer while an item is dragged: a container list starts showing which
    /// row would take it.
    // D2-11537
    pub(crate) fn listbox_mouse_over_top(&mut self, h: Handle, on: bool, handled: &mut bool) {
        self.element_mouse_over_top(h, on, handled);
        if on
            && !self.lb_test(h, bits::ROLLOVER)
            && self.dragged().is_some()
            && self.node(h).is_some_and(|n| n.bool_attr(attr::CONTAINER))
        {
            self.listbox_start_rollover(h);
        }
    }

    /// The manager's messages: a release ends selecting by drag and showing a dragged item; a
    /// move selects (while dragging) and shows the dragged item; the clock scrolls smoothly.
    // D2-11577, D2-11578
    pub(crate) fn listbox_listen(&mut self, h: Handle, what: u32, param: u32) {
        self.element_listen(h, what, param);
        if what == global::MOUSE_UP {
            self.listbox_end_drag_select(h);
            self.listbox_end_rollover(h);
        } else if what == global::MOUSE_MOVE {
            self.listbox_global_mouse_move(h);
        } else if what == global::TIME {
            self.listbox_global_loop(h);
        }
    }

    /// The pointer moved: while selecting by drag, the row under it is selected; while an item is
    /// dragged over the list, the row under it shows whether it would take the item.
    // D2-11566
    fn listbox_global_mouse_move(&mut self, h: Handle) {
        if self.lb_test(h, bits::DRAG_SELECTING) {
            if let Some(item) = self.listbox_item_under_mouse(h) {
                if Some(item) != self.list_selected_item(h) {
                    self.list_select_item(h, Some(item), true);
                }
            }
        }
        if self.lb_test(h, bits::ROLLOVER) {
            self.listbox_rollover(h);
        }
    }

    /// What the list's arrow buttons and scroll bars say: a hot click of an arrow scrolls a line
    /// (with `unique_msg`, only an arrow in the list's own window); the scroll bar's new position
    /// moves the view to that share of the sheet's hidden part, its release puts the view on a
    /// cell's edge, its lines and pages scroll.
    // D2-11538
    pub(crate) fn listbox_hear(&mut self, h: Handle, from: Handle, what: Token, param: u32) {
        let unique = self.node(h).is_some_and(|n| n.bool_attr(name::UNIQUE_MSG));
        let sender = self.node(from).map_or(Token(0), |n| n.id);
        if what == msg::HOT_CLICK && (!unique || self.root_of(from) == self.root_of(h)) {
            for (a, s) in [
                (name::UP_BUTTON, Scroll::VLineDec),
                (name::DOWN_BUTTON, Scroll::VLineInc),
                (attr::LEFT_BUTTON, Scroll::HLineDec),
                (attr::RIGHT_BUTTON, Scroll::HLineInc),
            ] {
                if self.node(h).and_then(|n| n.id_attr(a)) == Some(sender.raw()) {
                    self.listbox_scroll(h, s);
                }
            }
        }
        if unique && param != self.root_of(h).index() {
            return;
        }
        self.listbox_scrollbar_message(h, sender, attr::VERTICAL_SCROLLBAR, what, from);
        self.listbox_scrollbar_message(h, sender, name::HORIZONTAL_SCROLLBAR, what, from);
    }

    /// One of the list's scroll bars said something.
    // D2-11534
    fn listbox_scrollbar_message(
        &mut self,
        h: Handle,
        sender: Token,
        which: Token,
        what: Token,
        bar: Handle,
    ) {
        if self.node(h).and_then(|n| n.id_attr(which)) != Some(sender.raw()) {
            return;
        }
        let vertical = which == attr::VERTICAL_SCROLLBAR;
        if what == msg::NEW_POSITION {
            let (pos_attr, view_attr) = if vertical {
                (attr::Y_POSITION, name::Y_VIEW)
            } else {
                (attr::X_POSITION, name::X_VIEW)
            };
            let p = self.float_or(bar, pos_attr, 0.0);
            let Some(n) = self.node(h) else { return };
            let l = &n.lists.list;
            let hidden = if vertical {
                l.paper_h - n.rect.height()
            } else {
                l.paper_w - n.rect.width()
            };
            self.lb_set_bit(h, bits::FROM_SCROLLBAR, true);
            #[allow(clippy::cast_precision_loss)]
            let v = dereth_primitives::num::to_i32(p * hidden as f32);
            self.set_attr(h, view_attr, Value::Int(v));
            self.lb_set_bit(h, bits::FROM_SCROLLBAR, false);
            return;
        }
        if what == name::WIDGET_RELEASE {
            self.listbox_widget_release(h);
            return;
        }
        if let Some(s) = Scroll::of_message(what) {
            self.listbox_scroll(h, s);
        }
    }

    /// The scroll bar's widget let go: the view goes (as a goal) to the nearest cell's edge,
    /// unless the list scrolls by less than a cell or may end uneven at its end.
    // D2-11535
    fn listbox_widget_release(&mut self, h: Handle) {
        let Some(n) = self.node(h) else { return };
        let (w, hh) = (n.rect.width(), n.rect.height());
        let l = &n.lists.list;
        if l.item_w == 0 || l.item_h == 0 || l.bits & bits::LARGE_ITEM_SCROLL != 0 {
            return;
        }
        let uneven = l.bits & bits::UNEVEN_AT_END != 0;
        let gx = ((l.item_w / 2 + l.x_view) / l.item_w) * l.item_w;
        let gy = ((l.item_h / 2 + l.y_view) / l.item_h) * l.item_h;
        let (xv, yv, pw, ph) = (l.x_view, l.y_view, l.paper_w, l.paper_h);
        if gx != xv && (xv < pw - w || !uneven) {
            self.set_attr(h, name::GOAL_X_VIEW, Value::Int(gx));
        }
        if gy != yv && (yv < ph - hh || !uneven) {
            self.set_attr(h, name::GOAL_Y_VIEW, Value::Int(gy));
        }
    }
}
