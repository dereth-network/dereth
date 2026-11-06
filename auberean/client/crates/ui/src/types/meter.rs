//! Meters: a fill from 0 to 1, shown by a left fill up to the position and a right fill after it
//! (a top and a bottom fill likewise down a meter standing up); a goal position reached at once,
//! or smoothly over the clock; the fills' states changing as a smooth move passes set shares.

use crate::manager::global;
use crate::system::{Handle, Ui, Value};
use crate::token::{attr, state, Token};

/// A meter's fill, where it goes and, cut to length, its size.
type Fill = (Token, (i32, i32), Option<(i32, i32)>);

/// What a meter keeps between events.
#[derive(Clone, Debug, Default)]
pub struct MeterState {
    pub animating: bool,
    pub anim_start: f64,
    pub anim_end: f64,
    pub start_pos: f32,
    pub end_pos: f32,
    /// The fills' states by the share they begin at, and the next one a smooth move reaches.
    pub states: Vec<(f32, Token)>,
    pub next_state: Token,
    pub next_change: f32,
}

mod name {
    use crate::Token;
    pub const SMOOTH_MOVEMENT: Token = Token::of("smooth_movement");
    pub const SMOOTH_MOVEMENT_DURATION: Token = Token::of("smooth_movement_duration");
    pub const TOP_FILL: Token = Token::of("top_fill");
    pub const BOTTOM_FILL: Token = Token::of("bottom_fill");
}

impl Ui {
    /// A new meter: a smooth move of a quarter second, at 0 with its goal 0; the clock heard.
    // D2-11637, D2-11638, D2-11639
    pub(crate) fn meter_created(&mut self, h: Handle) {
        self.default_attr(h, name::SMOOTH_MOVEMENT_DURATION, Value::Float(0.25));
        self.default_attr(h, attr::POSITION, Value::Float(0.0));
        self.default_attr(h, attr::GOAL_POSITION, Value::Float(0.0));
        if let Some(n) = self.node_mut(h) {
            n.lists.meter.next_state = state::STATELESS;
        }
        self.register_for_global_message(h, global::TIME);
        self.lay_out_meter(h);
    }

    /// A meter's fills at its position: a left fill shows from the left edge to the position, a
    /// right fill from the position to the right edge (a top fill from the top down to it, a
    /// bottom fill from it down); a meter that moves its fills slides them, each keeping its own
    /// size, otherwise it cuts them to length.
    // D2-11625
    pub fn lay_out_meter(&mut self, h: Handle) {
        let Some(n) = self.node(h) else { return };
        let pos = f64::from(n.float_attr(attr::POSITION).or(n.meter).unwrap_or(0.0));
        let moves = n.bool_attr(attr::MOVE_FILL);
        let (w, hh) = (n.rect.width(), n.rect.height());
        // Worked out wide and cut toward zero.
        let share = |size: i32| dereth_primitives::num::to_i32_f64(pos * f64::from(size));
        let hidden = |size: i32| dereth_primitives::num::to_i32_f64((pos - 1.0) * f64::from(size));
        let (at, at_y) = (share(w), share(hh));
        // Where a moving fill goes, or where a cut fill goes and its size.
        let fills: [Fill; 4] = if moves {
            [
                (attr::LEFT_FILL, (hidden(w), 0), None),
                (name::TOP_FILL, (0, hidden(hh)), None),
                (attr::RIGHT_FILL, (at, 0), None),
                (name::BOTTOM_FILL, (0, at_y), None),
            ]
        } else {
            [
                (attr::LEFT_FILL, (0, 0), Some((at, hh))),
                (name::TOP_FILL, (0, 0), Some((w, at_y))),
                (attr::RIGHT_FILL, (at, 0), Some((w - at, hh))),
                (name::BOTTOM_FILL, (0, at_y), Some((w, hh - at_y))),
            ]
        };
        for (fill, (x, y), size) in fills {
            if let Some(f) = self.child(h, fill) {
                let Some(fl) = self.node(f) else { continue };
                let (fw, fh) = size.unwrap_or((fl.rect.width(), fl.rect.height()));
                let rect = crate::Rect::sized(x, y, fw, fh);
                if fl.sized && fl.rect == rect {
                    continue;
                }
                if let Some(fl) = self.node_mut(f) {
                    fl.rect = rect;
                    fl.sized = true;
                }
                self.place_children(f);
            }
        }
    }

    /// Set a meter's fill at once, 0 to 1 (any smooth move stops).
    pub fn set_meter(&mut self, h: Handle, fill: f32) {
        let fill = fill.clamp(0.0, 1.0);
        if let Some(n) = self.node_mut_undrawn(h) {
            n.meter = Some(fill);
            n.lists.meter.animating = false;
        }
        self.set_attr(h, attr::POSITION, Value::Float(fill));
        self.lay_out_meter(h);
    }

    /// A meter going away forgets its fills' states.
    // D2-11634
    pub(crate) fn meter_deleting(&mut self, h: Handle) {
        if let Some(n) = self.node_mut(h) {
            n.lists.meter.states.clear();
        }
    }

    /// A new state, a new place or a new size: the fills follow.
    // D2-11626, D2-11627, D2-11628, D2-11629
    pub(crate) fn meter_set_state(&mut self, h: Handle, s: Token) -> bool {
        let r = self.set_state_base(h, s);
        self.lay_out_meter(h);
        r
    }

    /// The next of the fills' states a smooth move reaches past `at`: the nearest share not below
    /// it (else the whole, in the base state).
    // D2-11630
    fn meter_next_state_change(&mut self, h: Handle, at: f32) {
        let Some(n) = self.node_mut(h) else { return };
        let m = &mut n.lists.meter;
        let mut best = 1.0_f32;
        let mut share = 1.0_f32;
        let mut st = state::STATELESS;
        for (p, s) in &m.states {
            let d = *p - at;
            if (0.0..best).contains(&d) {
                best = d;
                share = *p;
                st = *s;
            }
        }
        m.next_change = share;
        m.next_state = st;
    }

    /// Have the fills take `s` once a smooth move passes `share`.
    // D2-11635
    pub fn meter_state_at(&mut self, h: Handle, s: Token, share: f32) {
        if let Some(n) = self.node_mut(h) {
            let m = &mut n.lists.meter;
            m.states.retain(|(p, _)| *p != share);
            m.states.push((share, s));
        }
    }

    /// The clock: a smooth move goes its share of the way (and the fills take the next state once
    /// it passes that state's share).
    // D2-11631, D2-11632
    pub(crate) fn meter_listen(&mut self, h: Handle, what: u32, param: u32) {
        self.element_listen(h, what, param);
        if what != global::TIME {
            return;
        }
        let now = self.now();
        let Some(m) = self.node(h).map(|n| n.lists.meter.clone()) else {
            return;
        };
        if !m.animating {
            return;
        }
        let span = m.anim_end - m.anim_start;
        #[allow(clippy::cast_possible_truncation)]
        let mut t = if span > 0.0 {
            ((now - m.anim_start) / span) as f32
        } else {
            1.0
        };
        if t >= 1.0 {
            t = 1.0;
            if let Some(n) = self.node_mut(h) {
                n.lists.meter.animating = false;
            }
        }
        if !m.states.is_empty() && m.next_change < t {
            for fill in [
                attr::LEFT_FILL,
                name::TOP_FILL,
                attr::RIGHT_FILL,
                name::BOTTOM_FILL,
            ] {
                if let Some(f) = self.child(h, fill) {
                    self.set_state(f, m.next_state);
                }
            }
            self.flush_pending();
            self.meter_next_state_change(h, t);
        }
        let p = (m.end_pos - m.start_pos) * t + m.start_pos;
        if let Some(n) = self.node_mut(h) {
            n.meter = Some(p);
        }
        self.set_attr(h, attr::POSITION, Value::Float(p));
    }

    /// Start (or restart) a smooth move to the goal position over the meter's smooth-movement
    /// time; one under way restarts as if half done, from where it would have been.
    // D2-11633
    fn meter_start_animation(&mut self, h: Handle) {
        let duration = self.float_or(h, name::SMOOTH_MOVEMENT_DURATION, 0.0);
        if duration == 0.0 {
            return;
        }
        let pos = self.float_or(h, attr::POSITION, 0.0);
        let goal = self.float_or(h, attr::GOAL_POSITION, 0.0);
        let now = self.now();
        let multi = {
            let Some(n) = self.node_mut(h) else { return };
            let m = &mut n.lists.meter;
            m.end_pos = goal;
            if m.animating {
                m.anim_start = now - f64::from(duration) * 0.5;
                m.start_pos = (pos - m.end_pos) + pos;
            } else {
                m.anim_start = now;
                m.start_pos = pos;
                m.animating = true;
            }
            m.anim_end = f64::from(duration) + m.anim_start;
            !m.states.is_empty()
        };
        if multi {
            self.meter_next_state_change(h, 0.0);
        }
    }

    /// After a number was set: a new position lays the fills out; a goal position starts a smooth
    /// move, or is taken at once.
    // D2-11636
    pub(crate) fn meter_float_set(&mut self, h: Handle, a: Token, v: f32, changed: bool) {
        if changed && a == attr::POSITION {
            if let Some(n) = self.node_mut_undrawn(h) {
                n.meter = Some(v);
            }
            self.lay_out_meter(h);
        }
        if a == attr::GOAL_POSITION {
            if self
                .node(h)
                .is_some_and(|n| n.bool_attr(name::SMOOTH_MOVEMENT))
            {
                self.meter_start_animation(h);
                return;
            }
            self.set_attr(h, attr::POSITION, Value::Float(v));
        }
    }
}
