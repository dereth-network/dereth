//! Scroll bars: a widget along a bar at a position from 0 to 1, dragged by the pointer, moved by
//! the buttons the bar names and by the wheel, paging toward a press beside the widget (or jumping
//! there), snapping to evenly spaced stops when it has them, moving smoothly to a goal position.
//!
//! A bar that serves a list does not move the list itself: it says what happened (a new position,
//! the widget pressed or let go, a line or page back or on) with its window as the message's
//! parameter, and the list listening to it scrolls.

use dereth_primitives::num::to_i32;

use crate::manager::global;
use crate::system::{Handle, Ui, Value};
use crate::token::{attr, msg, state, Token};
use crate::types::button_id;
use crate::types::listbox::name as lname;

/// What a scroll bar keeps between events.
#[derive(Clone, Debug, Default)]
pub struct ScrollBarState {
    /// The widget child, once found.
    pub widget: Option<Handle>,
    pub bits: u32,
    pub anim_start: f64,
    pub anim_end: f64,
    pub anim_start_x: f32,
    pub anim_start_y: f32,
    pub anim_end_x: f32,
    pub anim_end_y: f32,
    /// When a held press beside the widget pages again.
    pub next_page: f64,
    /// Where on the widget the pointer took it.
    pub widget_offset: (i32, i32),
}

/// The flags a scroll bar keeps from its attributes and its own doings.
pub(crate) mod bits {
    pub const HORIZONTAL: u32 = 0x1;
    pub const PROPORTIONAL: u32 = 0x2;
    pub const DISABLED: u32 = 0x4;
    pub const HIDE_WHEN_DISABLED: u32 = 0x8;
    pub const SMOOTH: u32 = 0x10;
    pub const ALLOW_UPDATING: u32 = 0x20;
    pub const MOVE_TO_TOUCHED: u32 = 0x40;
    pub const BUTTONS_INTERACT: u32 = 0x80;
    pub const STOP_LOCATIONS: u32 = 0x100;
    pub const PRESSED: u32 = 0x8000;
    pub const DRAGGING: u32 = 0x1_0000;
    pub const PAGE_LEFT: u32 = 0x2_0000;
    pub const PAGE_RIGHT: u32 = 0x4_0000;
    pub const PAGE_UP: u32 = 0x8_0000;
    pub const PAGE_DOWN: u32 = 0x10_0000;
    pub const HEARS_MOVES: u32 = 0x20_0000;
    pub const ANIMATING: u32 = 0x40_0000;
    /// Paging while held, any way.
    pub const PAGING: u32 = PAGE_LEFT | PAGE_RIGHT | PAGE_UP | PAGE_DOWN;
}

mod name {
    use crate::Token;
    pub const HORIZONTAL: Token = Token::of("horizontal");
    pub const DISABLED: Token = Token::of("disabled");
    pub const HIDE_WHEN_DISABLED: Token = Token::of("hide_when_disabled");
    pub const SMOOTH_MOVEMENT: Token = Token::of("smooth_movement");
    pub const SMOOTH_MOVEMENT_DURATION: Token = Token::of("smooth_movement_duration");
    pub const MOVE_TO_TOUCHED_POSITION: Token = Token::of("move_to_touched_position");
    pub const GOAL_X_POSITION: Token = Token::of("goal_x_position");
    pub const GOAL_Y_POSITION: Token = Token::of("goal_y_position");
    pub const PAGE_FIRST_INTERVAL: Token = Token::of("page_first_interval");
    pub const PAGE_INTERVAL: Token = Token::of("page_interval");
    pub const TOP_FILL: Token = Token::of("top_fill");
    pub const BOTTOM_FILL: Token = Token::of("bottom_fill");
    pub const UP_BUTTON: Token = Token::of("up_button");
    pub const DOWN_BUTTON: Token = Token::of("down_button");
    pub const UNIQUE_MSG: Token = Token::of("unique_msg");
    /// The bar's own messages: pressed and let go.
    pub const MOUSE_PRESS: Token = Token::of("mouse_press");
    pub const MOUSE_RELEASE: Token = Token::of("mouse_release");
}

/// No goal position.
const NO_GOAL: i32 = -1;

/// The bar's flags that follow its yes-or-no attributes (the vertical attribute turns the
/// horizontal flag the other way).
const BOOL_BITS: [(Token, u32); 9] = [
    (name::HORIZONTAL, bits::HORIZONTAL),
    (attr::PROPORTIONAL, bits::PROPORTIONAL),
    (name::DISABLED, bits::DISABLED),
    (name::HIDE_WHEN_DISABLED, bits::HIDE_WHEN_DISABLED),
    (name::SMOOTH_MOVEMENT, bits::SMOOTH),
    (attr::ALLOW_UPDATING, bits::ALLOW_UPDATING),
    (name::MOVE_TO_TOUCHED_POSITION, bits::MOVE_TO_TOUCHED),
    (attr::BUTTONS_INTERACT, bits::BUTTONS_INTERACT),
    (attr::STOP_LOCATIONS, bits::STOP_LOCATIONS),
];

/// The buttons a bar may name, what the bar says to its window for each when its buttons do not
/// move it, and how many stops each moves it back.
const BUTTONS: [(Token, Token, i32); 4] = [
    (name::UP_BUTTON, lname::V_LINE_DEC, 1),
    (name::DOWN_BUTTON, lname::V_LINE_INC, -1),
    (attr::LEFT_BUTTON, lname::H_LINE_DEC, 1),
    (attr::RIGHT_BUTTON, lname::H_LINE_INC, -1),
];

impl Ui {
    fn sb(&self, h: Handle) -> Option<&ScrollBarState> {
        self.node(h).map(|n| &n.lists.scroll)
    }

    fn sb_mut(&mut self, h: Handle) -> Option<&mut ScrollBarState> {
        self.node_mut(h).map(|n| &mut n.lists.scroll)
    }

    fn sb_has(&self, h: Handle, bit: u32) -> bool {
        self.sb(h).is_some_and(|s| s.bits & bit != 0)
    }

    fn sb_set(&mut self, h: Handle, bit: u32, on: bool) {
        if let Some(s) = self.sb_mut(h) {
            if on {
                s.bits |= bit;
            } else {
                s.bits &= !bit;
            }
        }
    }

    /// The attribute of the bar's position along its length.
    fn sb_position_attr(&self, h: Handle) -> Token {
        if self.sb_has(h, bits::HORIZONTAL) {
            attr::X_POSITION
        } else {
            attr::Y_POSITION
        }
    }

    /// The message to the bar's window, with the window as its parameter.
    // D2-11658
    fn broadcast_on_root(&mut self, h: Handle, what: Token) {
        let root = self.root_of(h).index();
        self.broadcast(h, what, root);
    }

    // -----------------------------------------------------------------------------------------
    // Making and attributes
    // -----------------------------------------------------------------------------------------

    /// A new bar: updating on; pages a third of a second after the first and a tenth apart; a
    /// smooth move of a quarter second; at 0 with a widget filling it, the widget at least one
    /// texel; no goal; its flags from its attributes; listening to its buttons; the clock heard.
    // D2-11680, D2-11682, D2-11685
    pub(crate) fn scrollbar_created(&mut self, h: Handle) {
        self.default_attr(h, attr::ALLOW_UPDATING, Value::Bool(true));
        self.default_attr(h, name::PAGE_FIRST_INTERVAL, Value::Float(0.3));
        self.default_attr(h, name::PAGE_INTERVAL, Value::Float(0.1));
        self.default_attr(h, name::SMOOTH_MOVEMENT_DURATION, Value::Float(0.25));
        self.default_attr(h, attr::X_POSITION, Value::Float(0.0));
        self.default_attr(h, attr::Y_POSITION, Value::Float(0.0));
        self.default_attr(h, attr::X_WIDGET_SIZE, Value::Float(1.0));
        self.default_attr(h, attr::Y_WIDGET_SIZE, Value::Float(1.0));
        self.default_attr(h, attr::MIN_WIDGET_WIDTH, Value::Int(1));
        self.default_attr(h, attr::MIN_WIDGET_HEIGHT, Value::Int(1));
        self.default_attr(h, name::GOAL_X_POSITION, Value::Int(NO_GOAL));
        self.default_attr(h, name::GOAL_Y_POSITION, Value::Int(NO_GOAL));
        let mut b = 0;
        for (a, bit) in BOOL_BITS {
            if self.node(h).is_some_and(|n| n.bool_attr(a)) {
                b |= bit;
            }
        }
        if self
            .node(h)
            .is_some_and(|n| matches!(n.attr(attr::VERTICAL), Some(Value::Bool(false))))
        {
            b |= bits::HORIZONTAL;
        }
        if let Some(s) = self.sb_mut(h) {
            s.bits = b;
        }
        let named: Vec<u32> = BUTTONS
            .iter()
            .filter_map(|(a, _, _)| self.node(h).and_then(|n| n.id_attr(*a)))
            .collect();
        for id in named {
            self.register_for_message(h, Token(id), msg::HOT_CLICK);
        }
        self.register_for_global_message(h, global::TIME);
        self.lay_out_scrollbar(h);
    }

    /// After an attribute naming another element changed: a named button is listened to for its
    /// hot clicks.
    // D2-11657
    pub(crate) fn scrollbar_id_changed(&mut self, h: Handle, a: Token, id: u32) {
        if BUTTONS.iter().any(|(t, _, _)| *t == a) {
            self.register_for_message(h, Token(id), msg::HOT_CLICK);
        }
    }

    /// After a yes-or-no attribute changed: the bar is laid out again for its look's flags; each
    /// flag follows its attribute (the vertical attribute turning the horizontal flag the other
    /// way).
    // D2-11671
    pub(crate) fn scrollbar_bool_changed(&mut self, h: Handle, a: Token, v: bool) {
        if a == name::DISABLED
            || a == name::HIDE_WHEN_DISABLED
            || a == attr::PROPORTIONAL
            || a == name::HORIZONTAL
            || a == attr::VERTICAL
        {
            self.lay_out_scrollbar(h);
        }
        if a == attr::VERTICAL {
            self.sb_set(h, bits::HORIZONTAL, !v);
        }
        if let Some((_, bit)) = BOOL_BITS.iter().find(|(t, _)| *t == a) {
            self.sb_set(h, *bit, v);
        }
    }

    /// After a number was set: a new widget size or position lays the bar out; a goal position
    /// starts a smooth move, or is taken at once and forgotten.
    // D2-11672
    pub(crate) fn scrollbar_float_set(&mut self, h: Handle, a: Token, v: f32, changed: bool) {
        if changed
            && (a == attr::X_WIDGET_SIZE
                || a == attr::Y_WIDGET_SIZE
                || a == attr::X_POSITION
                || a == attr::Y_POSITION)
        {
            self.lay_out_scrollbar(h);
        }
        if a == name::GOAL_X_POSITION || a == name::GOAL_Y_POSITION {
            if self.sb_has(h, bits::SMOOTH) {
                self.scrollbar_start_animation(h);
                return;
            }
            let at = if a == name::GOAL_X_POSITION {
                attr::X_POSITION
            } else {
                attr::Y_POSITION
            };
            self.set_attr(h, at, Value::Float(v));
            self.set_attr(h, a, Value::Int(NO_GOAL));
        }
    }

    /// After a whole number changed: a new least widget size lays the bar out.
    // D2-11673
    pub(crate) fn scrollbar_int_changed(&mut self, h: Handle, a: Token) {
        if a == attr::MIN_WIDGET_WIDTH || a == attr::MIN_WIDGET_HEIGHT {
            self.lay_out_scrollbar(h);
        }
    }

    /// A new state, or a new size: laid out again.
    // D2-11670, D2-11664
    pub(crate) fn scrollbar_set_state(&mut self, h: Handle, s: Token) -> bool {
        let r = self.set_state_base(h, s);
        self.lay_out_scrollbar(h);
        r
    }

    // -----------------------------------------------------------------------------------------
    // Laying out
    // -----------------------------------------------------------------------------------------

    /// Show or hide a named button.
    // D2-11659
    fn scrollbar_show_button(&mut self, h: Handle, a: Token, shown: bool) {
        if let Some(b) = self.node(h).and_then(|n| n.id_attr(a)) {
            if let Some(e) = self.find(Token(b)) {
                self.set_visible(e, shown);
            }
        }
    }

    /// A fill child shown or hidden and placed at `(x, y)`, `w` by `hh`.
    // D2-11660
    fn scrollbar_fill(&mut self, h: Handle, fill: Token, shown: bool, at: (i32, i32, i32, i32)) {
        if let Some(f) = self.child(h, fill) {
            self.set_visible(f, shown);
            if let Some(n) = self.node_mut(f) {
                n.rect = crate::Rect::sized(at.0, at.1, at.2, at.3);
                n.sized = true;
            }
            self.place_children(f);
        }
    }

    /// The widget at its place along the bar: a proportional bar's widget as long as its share
    /// (never shorter than its least size); its fills up to and from the widget's middle; the bar
    /// and its buttons hidden when disabled and set to hide then, else shown; the widget hidden
    /// while the bar is disabled and shown.
    // D2-11661
    pub fn lay_out_scrollbar(&mut self, bar: Handle) {
        if self.sb(bar).is_some_and(|s| s.widget.is_none()) {
            let w = self.child(bar, attr::WIDGET);
            if let Some(s) = self.sb_mut(bar) {
                s.widget = w;
            }
        }
        if !self.sb_has(bar, bits::ALLOW_UPDATING) {
            return;
        }
        let Some(n) = self.node(bar) else { return };
        let (w, h) = (n.rect.width(), n.rect.height());
        let b = n.lists.scroll.bits;
        let widget = n.lists.scroll.widget.filter(|w| self.node(*w).is_some());
        let disabled = b & bits::DISABLED != 0;
        if let Some(wg) = widget {
            let Some(wn) = self.node(wg) else { return };
            let (cur_w, cur_h, cur_x, cur_y) =
                (wn.rect.width(), wn.rect.height(), wn.rect.x0, wn.rect.y0);
            let mut update_fills = false;
            let (mut ww, mut wh, mut wx, mut wy) = (cur_w, cur_h, cur_x, cur_y);
            if !disabled {
                if b & bits::PROPORTIONAL != 0 {
                    #[allow(clippy::cast_precision_loss)]
                    let sx = to_i32(self.float_or(bar, attr::X_WIDGET_SIZE, 1.0) * w as f32);
                    #[allow(clippy::cast_precision_loss)]
                    let sy = to_i32(self.float_or(bar, attr::Y_WIDGET_SIZE, 1.0) * h as f32);
                    ww = sx.max(self.int_or(bar, attr::MIN_WIDGET_WIDTH, 1));
                    wh = sy.max(self.int_or(bar, attr::MIN_WIDGET_HEIGHT, 1));
                }
                #[allow(clippy::cast_precision_loss)]
                if b & bits::HORIZONTAL == 0 {
                    wy = to_i32(self.float_or(bar, attr::Y_POSITION, 0.0) * (h - wh) as f32);
                } else {
                    wx = to_i32(self.float_or(bar, attr::X_POSITION, 0.0) * (w - ww) as f32);
                }
                let resized = ww != cur_w || wh != cur_h;
                let moved = wx != cur_x || wy != cur_y;
                if resized || moved {
                    if let Some(wn) = self.node_mut(wg) {
                        wn.rect = crate::Rect::sized(wx, wy, ww, wh);
                        wn.sized = true;
                    }
                    self.place_children(wg);
                    update_fills = true;
                }
            }
            if !update_fills {
                update_fills = b & bits::HIDE_WHEN_DISABLED == 0 && self.is_visible(wg) == disabled;
            }
            if update_fills {
                let shown = !disabled;
                let cx = wx + ww / 2;
                let cy = wy + wh / 2;
                self.scrollbar_fill(bar, attr::LEFT_FILL, shown, (0, 0, cx, h));
                self.scrollbar_fill(bar, attr::RIGHT_FILL, shown, (cx, 0, w - cx, h));
                self.scrollbar_fill(bar, name::TOP_FILL, shown, (0, 0, w, cy));
                self.scrollbar_fill(bar, name::BOTTOM_FILL, shown, (0, cy, w, h - cy));
            }
        }
        let hide = disabled && b & bits::HIDE_WHEN_DISABLED != 0 && self.is_visible(bar);
        if hide {
            self.set_visible(bar, false);
            for (a, _, _) in BUTTONS {
                self.scrollbar_show_button(bar, a, false);
            }
        } else if !self.is_visible(bar) {
            self.set_visible(bar, true);
            for (a, _, _) in BUTTONS {
                self.scrollbar_show_button(bar, a, true);
            }
        }
        if let Some(wg) = widget {
            let hidden = disabled && b & bits::HIDE_WHEN_DISABLED == 0;
            self.set_visible(wg, !hidden);
        }
    }

    // -----------------------------------------------------------------------------------------
    // Moving
    // -----------------------------------------------------------------------------------------

    /// Move a scroll bar to `position` (0 to 1) along its length.
    pub fn scroll_to(&mut self, bar: Handle, position: f32) {
        let which = self.sb_position_attr(bar);
        self.scrollbar_move_to(bar, position, which);
    }

    /// Move a scroll bar `steps` stops back (negative: on), within its ends.
    pub fn scroll_steps(&mut self, bar: Handle, steps: i32) {
        let which = self.sb_position_attr(bar);
        self.scrollbar_move_steps(bar, which, steps);
    }

    /// Hear the pointer's moves (once).
    // D2-11665
    fn scrollbar_hear_moves(&mut self, h: Handle) {
        if !self.sb_has(h, bits::HEARS_MOVES) {
            self.register_for_global_message(h, global::MOUSE_MOVE);
            self.sb_set(h, bits::HEARS_MOVES, true);
        }
    }

    /// Move to `position`: a bar with stops snaps down to the stop the position falls in (the
    /// last stop at 1 or beyond); kept within 0 and 1; then the bar tells its window.
    // D2-11666
    fn scrollbar_move_to(&mut self, h: Handle, position: f32, which: Token) {
        let mut p = position;
        if self.sb_has(h, bits::STOP_LOCATIONS) {
            let stops = self.int_or(h, attr::NUM_STOP_LOCATIONS, 1);
            if stops == 0 || stops == 1 || position >= 1.0 {
                p = 1.0;
            } else {
                #[allow(clippy::cast_precision_loss)]
                let step = 1.0 / (stops - 1) as f32;
                let mut at = 0.0_f32;
                loop {
                    if at <= position && position < step + at {
                        p = at;
                        break;
                    }
                    at += step;
                    if at >= 1.0 {
                        break;
                    }
                }
            }
        }
        let p = p.clamp(0.0, 1.0);
        self.set_attr(h, which, Value::Float(p));
        self.broadcast_on_root(h, msg::NEW_POSITION);
    }

    /// The widget's top-left `pos` along `range` texels: that share of the way.
    // D2-11667
    fn scrollbar_mouse_move_to(&mut self, h: Handle, range: i32, pos: i32, which: Token) {
        let mut p = 0.0;
        if range != 0 {
            #[allow(clippy::cast_precision_loss)]
            let v = pos.max(0).min(range) as f32 / range as f32;
            p = v;
        }
        self.scrollbar_move_to(h, p, which);
    }

    /// Move `steps` stops back (negative: on), within the ends; the bar tells its window when it
    /// moved.
    // D2-11668
    fn scrollbar_move_steps(&mut self, h: Handle, which: Token, steps: i32) {
        let now = self.float_or(h, which, 0.0);
        let stops = self.int_or(h, attr::NUM_STOP_LOCATIONS, 1);
        #[allow(clippy::cast_precision_loss)]
        let p = if stops == 0 || stops == 1 || now > 1.0 {
            1.0
        } else {
            now - (1.0 / (stops - 1) as f32) * steps as f32
        };
        let p = p.clamp(0.0, 1.0);
        let moved = self.node(h).and_then(|n| n.float_attr(which)) != Some(p);
        self.set_attr(h, which, Value::Float(p));
        if moved {
            self.broadcast_on_root(h, msg::NEW_POSITION);
        }
    }

    /// Start (or restart) a smooth move toward the goal position over the bar's smooth-movement
    /// time; one under way restarts as if half done, from where it would have been.
    // D2-11662
    fn scrollbar_start_animation(&mut self, h: Handle) {
        let duration = self.float_or(h, name::SMOOTH_MOVEMENT_DURATION, 0.0);
        if duration == 0.0 {
            return;
        }
        let x = self.float_or(h, attr::X_POSITION, 0.0);
        let y = self.float_or(h, attr::Y_POSITION, 0.0);
        let goal = |a| -> Option<f32> {
            self.node(h).and_then(|n| match n.attr(a) {
                Some(Value::Float(f)) if *f != -1.0 => Some(*f),
                _ => None,
            })
        };
        let (gx, gy) = (goal(name::GOAL_X_POSITION), goal(name::GOAL_Y_POSITION));
        let now = self.now();
        let Some(s) = self.sb_mut(h) else { return };
        s.anim_end_x = gx.unwrap_or(x);
        s.anim_end_y = gy.unwrap_or(y);
        if s.bits & bits::ANIMATING == 0 {
            s.anim_start = now;
            s.anim_start_x = x;
            s.anim_start_y = y;
            s.bits |= bits::ANIMATING;
        } else {
            s.anim_start = now - f64::from(duration) * 0.5;
            s.anim_start_x = (x - s.anim_end_x) + x;
            s.anim_start_y = (y - s.anim_end_y) + y;
        }
        s.anim_end = f64::from(duration) + s.anim_start;
    }

    /// Stop a smooth move: no goal any more.
    // D2-11663
    fn scrollbar_stop_animation(&mut self, h: Handle) {
        if self.sb_has(h, bits::ANIMATING) {
            self.set_attr(h, name::GOAL_X_POSITION, Value::Int(NO_GOAL));
            self.set_attr(h, name::GOAL_Y_POSITION, Value::Int(NO_GOAL));
            self.sb_set(h, bits::ANIMATING, false);
        }
    }

    // -----------------------------------------------------------------------------------------
    // The pointer
    // -----------------------------------------------------------------------------------------

    /// A press on the bar: it says it was pressed; then (unless disabled) the wheel moves it; a
    /// press beside the widget pages toward the pointer (and keeps paging while held), or, when
    /// the bar moves to where it is touched, jumps the widget's middle there; a press on the
    /// widget (or a jump) takes the widget, which follows the pointer until let go.
    // D2-11683
    pub(crate) fn scrollbar_mouse_down(
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
        self.broadcast(h, name::MOUSE_PRESS, b);
        let wheel = b == button_id::WHEEL_UP || b == button_id::WHEEL_DOWN;
        if !wheel {
            self.sb_set(h, bits::PRESSED, true);
        }
        let Some(wg) = self.sb(h).and_then(|s| s.widget) else {
            return;
        };
        if self.sb_has(h, bits::DISABLED) {
            return;
        }
        if wheel {
            self.scrollbar_mouse_wheel(h, b);
            return;
        }
        let Some(wr) = self.screen_rect(wg) else {
            return;
        };
        let on_widget = self.node(wg).is_some_and(|n| n.over);
        if !on_widget {
            if !self.sb_has(h, bits::MOVE_TO_TOUCHED) {
                if x < wr.x0 {
                    self.sb_set(h, bits::PAGE_LEFT, true);
                    self.broadcast_on_root(h, lname::H_PAGE_DEC);
                } else if wr.x0 + wr.width() <= x {
                    self.sb_set(h, bits::PAGE_RIGHT, true);
                    self.broadcast_on_root(h, lname::H_PAGE_INC);
                }
                if y < wr.y0 {
                    self.sb_set(h, bits::PAGE_UP, true);
                    self.broadcast_on_root(h, lname::V_PAGE_DEC);
                } else if wr.y0 + wr.height() <= y {
                    self.sb_set(h, bits::PAGE_DOWN, true);
                    self.broadcast_on_root(h, lname::V_PAGE_INC);
                }
                if self.sb_has(h, bits::PAGE_UP | bits::PAGE_DOWN) {
                    let first = self.float_or(h, name::PAGE_FIRST_INTERVAL, 0.0);
                    let now = self.now();
                    if let Some(s) = self.sb_mut(h) {
                        s.next_page = f64::from(first) + now;
                    }
                }
                if self.sb_has(h, bits::PAGING | bits::DRAGGING) {
                    self.scrollbar_hear_moves(h);
                }
                return;
            }
            self.scrollbar_stop_animation(h);
            let Some(br) = self.screen_rect(h) else {
                return;
            };
            let which = self.sb_position_attr(h);
            #[allow(clippy::cast_precision_loss)]
            let p = if self.sb_has(h, bits::HORIZONTAL) {
                (x - wr.width() / 2 - br.x0) as f32 / (br.width() - wr.width()) as f32
            } else {
                (y - wr.height() / 2 - br.y0) as f32 / (br.height() - wr.height()) as f32
            };
            self.scrollbar_move_to(h, p, which);
        }
        self.sb_set(h, bits::DRAGGING, true);
        if let Some(wr) = self.screen_rect(wg) {
            if let Some(s) = self.sb_mut(h) {
                s.widget_offset = (x - wr.x0, y - wr.y0);
            }
        }
        self.set_state(wg, state::PRESSED);
        self.flush_pending();
        self.scrollbar_hear_moves(h);
        self.broadcast_on_root(h, lname::WIDGET_PRESS);
    }

    /// A release on the bar: it says so; it stops hearing the pointer; the widget, if it was held,
    /// is let go (and the bar says so); paging and dragging end.
    // D2-11674
    pub(crate) fn scrollbar_mouse_up(
        &mut self,
        h: Handle,
        x: i32,
        y: i32,
        b: u32,
        handled: &mut bool,
    ) {
        self.element_mouse_up(h, x, y, b, handled);
        self.broadcast(h, name::MOUSE_RELEASE, b);
        if self.sb_has(h, bits::HEARS_MOVES) {
            self.unregister_for_global_message(h, global::MOUSE_MOVE);
            self.sb_set(h, bits::HEARS_MOVES, false);
        }
        if let Some(wg) = self.sb(h).and_then(|s| s.widget) {
            if self.sb_has(h, bits::DRAGGING) {
                self.set_state(wg, state::NORMAL);
                self.flush_pending();
            }
            self.broadcast_on_root(h, lname::WIDGET_RELEASE);
        }
        if let Some(s) = self.sb_mut(h) {
            s.bits &= !(bits::PRESSED | bits::DRAGGING | bits::PAGING);
        }
    }

    /// The wheel: a bar with stops moves a stop; otherwise it tells its window to scroll a line.
    // D2-11678
    fn scrollbar_mouse_wheel(&mut self, h: Handle, b: u32) {
        let which = self.sb_position_attr(h);
        let up = b == button_id::WHEEL_UP;
        if self.node(h).and_then(|n| n.float_attr(which)).is_none() {
            return;
        }
        self.mgr.wheel_taken = true;
        if self.sb_has(h, bits::STOP_LOCATIONS) {
            self.scrollbar_move_steps(h, which, if up { 1 } else { -1 });
            return;
        }
        let what = match (self.sb_has(h, bits::HORIZONTAL), up) {
            (false, true) => lname::V_LINE_DEC,
            (false, false) => lname::V_LINE_INC,
            (true, true) => lname::H_LINE_DEC,
            (true, false) => lname::H_LINE_INC,
        };
        self.broadcast_on_root(h, what);
    }

    /// A wheel step no scroll bar took: the nearest bar of the window under the pointer takes it,
    /// looking out from the topmost element to the whole window.
    pub(crate) fn wheel_the_window(&mut self, top: Option<Handle>, x: i32, y: i32, b: u32) {
        let mut at = top;
        while let Some(h) = at {
            if let Some(bar) = self.nearest_bar(h, x, y) {
                let mut handled = false;
                self.mouse_down(bar, x, y, b, &mut handled);
                return;
            }
            at = self.node(h).and_then(|n| n.parent);
        }
    }

    /// The vertical bar shown and enabled under an element that is nearest `(x, y)`.
    fn nearest_bar(&self, h: Handle, x: i32, y: i32) -> Option<Handle> {
        let distance = |bar: Handle| {
            let r = self.screen_rect(bar).unwrap_or_default();
            let dx = (r.x0 - x).max(x - r.x1).max(0);
            let dy = (r.y0 - y).max(y - r.y1).max(0);
            i64::from(dx) * i64::from(dx) + i64::from(dy) * i64::from(dy)
        };
        let mut stack = vec![h];
        let mut best: Option<(i64, Handle)> = None;
        while let Some(at) = stack.pop() {
            let Some(n) = self.node(at) else { continue };
            if !n.visible {
                continue;
            }
            if self.kind_of(at) == crate::types::kind::SCROLLBAR
                && !self.sb_has(at, bits::HORIZONTAL | bits::DISABLED)
                && self.shown(at)
            {
                let d = distance(at);
                if best.is_none_or(|(b, _)| d < b) {
                    best = Some((d, at));
                }
            }
            stack.extend(n.children.iter().copied());
        }
        best.map(|(_, bar)| bar)
    }

    /// The pointer moved while the widget is held: the widget's top-left goes where the pointer
    /// took it from, as a share of the bar's length less the widget's.
    // D2-11676
    fn scrollbar_global_mouse_move(&mut self, h: Handle) {
        if !self.sb_has(h, bits::DRAGGING) {
            return;
        }
        let Some(wg) = self.sb(h).and_then(|s| s.widget) else {
            return;
        };
        let (Some(br), Some(wn)) = (self.screen_rect(h), self.node(wg)) else {
            return;
        };
        let (ww, wh) = (wn.rect.width(), wn.rect.height());
        let (px, py) = self.pointer();
        let (ox, oy) = self.sb(h).map_or((0, 0), |s| s.widget_offset);
        let which = self.sb_position_attr(h);
        if self.sb_has(h, bits::HORIZONTAL) {
            self.scrollbar_mouse_move_to(h, br.width() - ww, px - ox - br.x0, which);
        } else {
            self.scrollbar_mouse_move_to(h, br.height() - wh, py - oy - br.y0, which);
        }
    }

    /// The clock: a press held beside the widget pages again each page interval while the
    /// widget has not reached the pointer; a smooth move goes its share of the way.
    // D2-11675
    fn scrollbar_global_loop(&mut self, h: Handle) {
        let now = self.now();
        let next = self.sb(h).map_or(0.0, |s| s.next_page);
        if self.sb_has(h, bits::PAGING) && next <= now {
            let Some(wg) = self.sb(h).and_then(|s| s.widget) else {
                return;
            };
            let Some(wr) = self.screen_rect(wg) else {
                return;
            };
            let (px, py) = self.pointer();
            let x = self.float_or(h, attr::X_POSITION, 0.0);
            let y = self.float_or(h, attr::Y_POSITION, 0.0);
            let mut paged = false;
            if self.sb_has(h, bits::PAGE_LEFT) && px < wr.x0 && x > 0.0 {
                paged = true;
                self.broadcast_on_root(h, lname::H_PAGE_DEC);
            } else if self.sb_has(h, bits::PAGE_RIGHT) && wr.x0 + wr.width() <= px && x < 1.0 {
                paged = true;
                self.broadcast_on_root(h, lname::H_PAGE_INC);
            }
            let mut vpaged = false;
            if self.sb_has(h, bits::PAGE_UP) && py < wr.y0 && y > 0.0 {
                vpaged = true;
                self.broadcast_on_root(h, lname::V_PAGE_DEC);
            } else if self.sb_has(h, bits::PAGE_DOWN) && wr.y0 + wr.height() <= py && y < 1.0 {
                vpaged = true;
                self.broadcast_on_root(h, lname::V_PAGE_INC);
            }
            if paged || vpaged {
                let interval = self.float_or(h, name::PAGE_INTERVAL, 0.0);
                if let Some(s) = self.sb_mut(h) {
                    s.next_page += f64::from(interval);
                }
            } else if let Some(s) = self.sb_mut(h) {
                if s.next_page < now {
                    s.next_page = now;
                }
            }
        }
        if self.sb_has(h, bits::ANIMATING) {
            let Some(s) = self.sb(h) else { return };
            let span = s.anim_end - s.anim_start;
            #[allow(clippy::cast_possible_truncation)]
            let mut t = if span > 0.0 {
                ((now - s.anim_start) / span) as f32
            } else {
                1.0
            };
            let (sx, sy, ex, ey) = (s.anim_start_x, s.anim_start_y, s.anim_end_x, s.anim_end_y);
            if t >= 1.0 {
                t = 1.0;
                self.scrollbar_stop_animation(h);
            }
            self.set_attr(h, attr::X_POSITION, Value::Float((ex - sx) * t + sx));
            self.set_attr(h, attr::Y_POSITION, Value::Float((ey - sy) * t + sy));
        }
    }

    /// The manager's messages: the pointer's moves drag the widget; the clock pages and moves
    /// smoothly. A bar asked to update lays itself out.
    // D2-11677
    pub(crate) fn scrollbar_listen(&mut self, h: Handle, what: u32, param: u32) {
        self.element_listen(h, what, param);
        if what == global::MOUSE_MOVE {
            self.scrollbar_global_mouse_move(h);
        } else if what == global::TIME {
            self.scrollbar_global_loop(h);
        }
    }

    /// A bar going away stops hearing the pointer and its buttons.
    // D2-11669, D2-11681
    pub(crate) fn scrollbar_deleting(&mut self, h: Handle) {
        if self.sb_has(h, bits::HEARS_MOVES) {
            self.unregister_for_global_message(h, global::MOUSE_MOVE);
            self.sb_set(h, bits::HEARS_MOVES, false);
        }
        let named: Vec<u32> = BUTTONS
            .iter()
            .filter_map(|(a, _, _)| self.node(h).and_then(|n| n.id_attr(*a)))
            .collect();
        for id in named {
            self.unregister_for_message(h, Token(id), msg::HOT_CLICK);
        }
    }

    /// A hot click of one of the buttons the bar names (with `unique_msg`, only a button in the
    /// bar's own window).
    // D2-11684
    pub(crate) fn scrollbar_hear(&mut self, h: Handle, from: Handle, what: Token, _param: u32) {
        if what != msg::HOT_CLICK {
            return;
        }
        if self.node(h).is_some_and(|n| n.bool_attr(name::UNIQUE_MSG))
            && self.root_of(from) != self.root_of(h)
        {
            return;
        }
        let Some(id) = self.node(from).map(|n| n.id) else {
            return;
        };
        for (button, line, steps) in BUTTONS {
            self.scrollbar_button_hot_click(h, id, button, line, steps);
        }
    }

    /// One named button's hot click: the bar moves `steps` stops back when its buttons move it,
    /// else it tells its window to scroll a line.
    // D2-11679
    fn scrollbar_button_hot_click(
        &mut self,
        h: Handle,
        from: Token,
        button: Token,
        line: Token,
        steps: i32,
    ) {
        if self.node(h).and_then(|n| n.id_attr(button)) != Some(from.raw()) {
            return;
        }
        if self.sb_has(h, bits::BUTTONS_INTERACT) {
            let which = self.sb_position_attr(h);
            self.scrollbar_move_steps(h, which, steps);
            return;
        }
        self.broadcast_on_root(h, line);
    }
}
