use super::*;
use crate::{Box2D, ElementId};

/// The scrollbar's own attribute ids — property group 0x12, as its available-properties
/// query declares them.
pub mod attr {
    /// 0x75 — disallow updating: the layout update does nothing at all while it is set.
    pub const DISALLOW_UPDATING: u32 = 0x75;
    /// 0x76 — disabled.
    pub const DISABLED: u32 = 0x76;
    /// 0x77 — the **increment** button's element id.
    pub const INCREMENT_BUTTON: u32 = 0x77;
    /// 0x78 — the **decrement** button's element id.
    pub const DECREMENT_BUTTON: u32 = 0x78;
    /// 0x79 — hide the whole bar while it is disabled.
    pub const HIDE_WHEN_DISABLED: u32 = 0x79;
    /// 0x7A — flag bit `0x1000000`, the "tell the parent about disabled" flag.
    pub const NOTIFY_DISABLED: u32 = 0x7A;
    /// 0x7B — horizontal rather than vertical.
    pub const HORIZONTAL: u32 = 0x7B;
    /// 0x7C — **move to touched**: a press anywhere on the bar jumps the thumb to the pointer.
    /// Setting it also makes the thumb mouse-invisible and clears auto-repeat (0x0F),
    /// which is what stops the thumb from eating the press.
    pub const MOVE_TO_TOUCHED: u32 = 0x7C;
    /// 0x7D — how many discrete stops the bar has.
    pub const STOP_COUNT: u32 = 0x7D;
    /// 0x7E — the stops sit at the **centre** of their band rather than at its edges, and the
    /// thumb is not subtracted from the active length.
    pub const CENTRED_STOPS: u32 = 0x7E;
    /// 0x7F — the bar has stop locations.
    pub const HAS_STOP_LOCATIONS: u32 = 0x7F;
    /// 0x82 — the thumb is sized to [`PROPORTION`].
    pub const PROPORTIONAL: u32 = 0x82;
    /// 0x83 — animate to a new position over [`ANIM_DURATION`] instead of jumping.
    pub const SMOOTH_MOVEMENT: u32 = 0x83;
    /// 0x84 — the animation's duration in seconds.
    pub const ANIM_DURATION: u32 = 0x84;
    /// 0x85 — the animation's target, written by whoever wants a smooth move.
    pub const ANIM_TARGET: u32 = 0x85;
    /// 0x86 — **the position**, `0.0 ..= 1.0`.
    pub const POSITION: u32 = 0x86;
    /// 0x87 — the current stop index, when the bar has stop locations.
    pub const STOP: u32 = 0x87;
    /// 0x88 — the fraction of the track the thumb covers (default 1.0).
    pub const PROPORTION: u32 = 0x88;
    /// 0x89 — the thumb's minimum length in pixels.
    pub const MIN_WIDGET_SIZE: u32 = 0x89;
}

/// The flag word, from the eight one-line setters.
pub mod bits {
    pub const HORIZONTAL: u32 = 0x0000_0001;
    pub const PROPORTIONAL: u32 = 0x0000_0002;
    pub const DISABLED: u32 = 0x0000_0004;
    pub const HIDE_DISABLED: u32 = 0x0000_0008;
    pub const SMOOTH_MOVEMENT: u32 = 0x0000_0010;
    pub const DISALLOW_UPDATING: u32 = 0x0000_0020;
    pub const MOVE_TO_TOUCHED: u32 = 0x0000_0040;
    pub const HAS_STOPS: u32 = 0x0000_0100;
    /// 0x400000 — animating. Not one of the eight setters:
    /// the animation start sets it and the tick clears it when the animation reaches 1.0.
    pub const ANIMATING: u32 = 0x0040_0000;
    pub const NOTIFY_DISABLED: u32 = 0x0100_0000;
}

/// The client's thumb lookup — the thumb is the **direct**
/// child whose element id is 1, in every shipped layout that has one.
pub const WIDGET_ID: ElementId = ElementId(1);

/// The client's `abs(secondary) < 0x65` test — a thumb drag that
/// wanders more than 100 px off the bar's own axis snaps back to where it started.
pub const DRAG_CANCEL_DISTANCE: i32 = 0x65;

/// The scale `p1` of message `0x0A` carries; see the module documentation.
pub const POSITION_SCALE: f32 = 1000.0;

/// How a screen routes wheel input directly over a scrollbar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectWheel {
    /// Step the linked scrolling content or the bar's authored stops.
    Content,
    /// Adjust a continuous value by one percentage point.
    Percentage,
}

#[derive(Debug, Default)]
pub struct Scrollbar {
    /// **A scrollbar is a button, which is a text element.** No shipped scrollbar
    /// carries a caption, but the base is real and dropping it would silently discard one.
    pub button: super::button::Button,
    /// The flag word.
    pub bits: u32,
    /// The thumb, resolved lazily from the authored widget id.
    pub widget: Option<ElemHandle>,
    /// The increment and decrement button ids.
    pub increment_button: Option<ElementId>,
    pub decrement_button: Option<ElementId>,
    /// The scrolling area — the track, in the bar's own coordinates.
    pub scrolling_area: Box2D,
    /// A thumb drag is active.
    pub widget_drag_active: bool,
    /// Optional screen policy for wheel input over the track, thumb, or arrows.
    pub direct_wheel: Option<DirectWheel>,
    /// The drag start point, in the bar's own coordinates.
    pub drag_start: (i32, i32),
    /// The reset position — where the thumb was when the drag started.
    pub reset_position: f32,
    /// The animation's start and end positions and start and end times — the scrollbar's
    /// smooth movement.
    pub anim_start_pos: f32,
    pub anim_end_pos: f32,
    pub anim_start_time: f64,
    pub anim_end_time: f64,
}

pub fn create(_l: &crate::LayoutDesc, _d: &crate::ElementDesc) -> Box<dyn Element> {
    Box::new(Scrollbar::default())
}

fn set_bit(bits: &mut u32, mask: u32, on: bool) {
    if on {
        *bits |= mask;
    } else {
        *bits &= !mask;
    }
}

/// **Update scrollbar layout from outside the element.**
///
/// The scrollable's position update ends with a direct call to the bar's layout
/// update; its caller is the **scrollable**, not the
/// bar. In the client that is an ordinary call on a live object. Here the bar's
/// behaviour has usually been **lifted out of its arena slot** at that moment — an arrow click
/// runs the move-steps handler inside the bar's own dispatch, which raises `0x0D`…`0x10`, which
/// the log consumes, which writes attribute `0x86` back onto the bar — and
/// [`crate::UiSystem::on_set_attribute`] silently skips the subclass half of an element whose
/// slot is empty, so nothing lays the thumb out.
///
/// **Without this the thumb freezes**: arrowing through the whole chat log and back leaves it
/// where it was, and it moves only on *append* — an append runs from
/// the frame loop, where the bar's slot is occupied and the attribute write does reach it.
/// The same silence as `queue_set_state` and `update_mouse_visibility` are queued against.
///
/// The layout is re-derived from the bar's own attributes rather than from the lifted object,
/// which is sound because the layout update recomputes everything it reads: the thumb from
/// the direct child with id 1, and the scrolling area.
/// Nothing it touches is drag state.
pub fn update_layout_of(ui: &mut UiSystem, bar: ElemHandle) {
    let Some(props) = ui.node(bar).map(|n| n.merged_properties()) else {
        return;
    };
    let b = |id: u32| matches!(props.get(id), Some(crate::PropertyValue::Bool(true)));
    let mut bits = 0;
    set_bit(&mut bits, bits::HORIZONTAL, b(attr::HORIZONTAL));
    set_bit(&mut bits, bits::PROPORTIONAL, b(attr::PROPORTIONAL));
    set_bit(&mut bits, bits::DISABLED, b(attr::DISABLED));
    set_bit(&mut bits, bits::HIDE_DISABLED, b(attr::HIDE_WHEN_DISABLED));
    set_bit(
        &mut bits,
        bits::DISALLOW_UPDATING,
        b(attr::DISALLOW_UPDATING),
    );
    set_bit(&mut bits, bits::MOVE_TO_TOUCHED, b(attr::MOVE_TO_TOUCHED));
    set_bit(
        &mut bits,
        bits::HAS_STOPS,
        matches!(props.get(attr::STOP_COUNT), Some(crate::PropertyValue::Integer(n)) if *n != 0),
    );
    let mut s = Scrollbar {
        bits,
        increment_button: element_id(props.get(attr::INCREMENT_BUTTON)),
        decrement_button: element_id(props.get(attr::DECREMENT_BUTTON)),
        ..Scrollbar::default()
    };
    s.update_layout(ui, bar);
}

/// The client's float-to-long conversion, which truncates towards zero.
fn ftol(v: f32) -> i32 {
    dereth_primitives::num::to_i32(v)
}

#[allow(clippy::cast_precision_loss)]
fn f(v: i32) -> f32 {
    v as f32
}

impl Scrollbar {
    #[must_use]
    pub const fn horizontal(&self) -> bool {
        self.bits & bits::HORIZONTAL != 0
    }

    #[must_use]
    pub const fn move_to_touched(&self) -> bool {
        self.bits & bits::MOVE_TO_TOUCHED != 0
    }

    /// Float attribute `0x86` — the position, `0.0 ..= 1.0`.
    #[must_use]
    pub fn position(ui: &UiSystem, me: ElemHandle) -> f32 {
        ui.node(me)
            .and_then(|n| n.merged_properties().get_float(attr::POSITION))
            .unwrap_or(0.0)
    }

    fn int_attr(ui: &UiSystem, me: ElemHandle, id: u32) -> Option<i32> {
        ui.node(me).and_then(|n| n.merged_properties().get_int(id))
    }

    fn bool_attr(ui: &UiSystem, me: ElemHandle, id: u32) -> bool {
        ui.node(me)
            .and_then(|n| n.merged_properties().get_bool(id))
            .unwrap_or(false)
    }

    /// The stop-to-position conversion:
    ///
    /// ```text
    /// n = int attribute 0x7D (default 1); if absent or n == 0: return -1.0
    /// stop = clamp(stop, 0, n - 1)
    /// if bool attribute 0x7E: return (stop + 0.5) / n      // centred
    /// if n > 1:               return  stop        / (n - 1)
    /// return 0.5
    /// ```
    #[must_use]
    pub fn stop_to_position(ui: &UiSystem, me: ElemHandle, stop: i32) -> f32 {
        let Some(n) = Self::int_attr(ui, me, attr::STOP_COUNT).filter(|n| *n != 0) else {
            return -1.0;
        };
        let stop = stop.clamp(0, n - 1);
        if Self::bool_attr(ui, me, attr::CENTRED_STOPS) {
            return (f(stop) + 0.5) / f(n);
        }
        if n > 1 {
            return f(stop) / f(n - 1);
        }
        0.5
    }

    /// Behavior: the stop-to-position conversion's inverse, and `-1` when the bar has no
    /// stop count at all.
    ///
    /// Both truncated operands are the inverses of the two branches above: `pos·n` for centred stops (whose bands are `[k/n, (k+1)/n)`,
    /// so truncation is exact) and `pos·(n−1)` rounded for the others, which is the only
    /// mapping used here to return every stop-to-position of `k` to `k`.
    #[must_use]
    pub fn position_to_stop(ui: &UiSystem, me: ElemHandle, pos: f32) -> i32 {
        let Some(n) = Self::int_attr(ui, me, attr::STOP_COUNT).filter(|n| *n != 0) else {
            return -1;
        };
        if Self::bool_attr(ui, me, attr::CENTRED_STOPS) {
            return ftol(pos * f(n));
        }
        if n > 1 {
            return ftol(pos * f(n - 1) + 0.5);
        }
        0
    }

    /// Validate a position: snap to a stop when there are stop locations, then clamp
    /// into `0.0 ..= 1.0`.
    #[must_use]
    pub fn validate_position(&self, ui: &UiSystem, me: ElemHandle, mut pos: f32) -> f32 {
        if self.bits & bits::HAS_STOPS != 0 {
            let p = Self::stop_to_position(ui, me, Self::position_to_stop(ui, me, pos));
            if p >= 0.0 {
                pos = p;
            }
        }
        pos.clamp(0.0, 1.0)
    }

    /// The scrolling-area update — the bar's box, less whatever the two arrow buttons
    /// occupy at its ends.
    ///
    /// ```text
    /// area = (0, 0, width, height)
    /// if b = button(true):  area.top += b.height; area.left += b.width
    ///                       b.move_to(0, 0)
    /// if b = button(false): area.right -= b.width; area.bottom -= b.height
    ///                       b.move_to(area.right, area.bottom)
    /// ```
    ///
    /// **The two moves are performed**, checked against retail, in that order, `true` first —
    /// even though the shipped layouts author some pairs the other way round.
    ///
    /// The button lookup with `true` reads the increment-button id and with `false` the
    /// decrement one, so the first move **is** the increment button's. The scrollbar's global-loop recovery arm —
    /// the one that re-places an arrow whose id resolved to
    /// nothing at attribute-set time — performs *the same two moves* independently, which is
    /// a second witness inside the same client.
    ///
    /// **The shipped layouts do not agree with each other**, which is why no reading of them
    /// could have settled it: the three vertical bars (`0x2100003E`'s `0x10000455` and
    /// `0x10000367`, `0x2100004C`'s `0x100002DC`) author the *increment* at the far end,
    /// while the horizontal pair (`0x2100003E`'s `0x10000368` and `0x1000036D`) author it at
    /// the near end. Both cannot be the identity, so the authored positions are decoration
    /// and this is what places them. `0x10000368` is the clearest case: it is 72 wide with
    /// 20-wide arrows, and its far arrow is authored at x = 32 where the arithmetic puts it
    /// at 52.
    ///
    /// The move is coupled to the step's **sign**: retail negates for `0x0D`/`0x0F`.
    /// The scroll-delta query receives `id == 0x0D || id == 0x0F` as its negate flag, so the
    /// increment arrow — the one moved to `(0, 0)` —
    /// scrolls the view **up**. Landing either half without the other reverses the arrows on
    /// screen; see the negate group in [`crate::text::TextElement`] and [`listbox`].
    pub fn update_scrolling_area(&mut self, ui: &mut UiSystem, me: ElemHandle) {
        let b = ui.node(me).map_or(Box2D::default(), |n| n.region.box_);
        let mut area = Box2D {
            x0: 0,
            y0: 0,
            x1: b.width(),
            y1: b.height(),
        };
        if let Some(inc) = self.button_box(ui, me, true) {
            area.y0 += inc.height();
            area.x0 += inc.width();
            if let Some(h) = self.button_handle(ui, me, true) {
                ui.move_to(h, 0, 0);
            }
        }
        if let Some(dec) = self.button_box(ui, me, false) {
            area.x1 -= dec.width();
            area.y1 -= dec.height();
            if let Some(h) = self.button_handle(ui, me, false) {
                ui.move_to(h, area.x1, area.y1);
            }
        }
        self.scrolling_area = area;
    }

    /// The arrow-button lookup, reduced to what the layout needs of it: the button's
    /// box, or `None` when the id names nothing under this bar.
    fn button_box(&self, ui: &UiSystem, me: ElemHandle, increment: bool) -> Option<Box2D> {
        Some(ui.node(self.button_handle(ui, me, increment)?)?.region.box_)
    }

    /// The same lookup kept as a handle, so the layout update can move what it measured.
    fn button_handle(&self, ui: &UiSystem, me: ElemHandle, increment: bool) -> Option<ElemHandle> {
        let id = if increment {
            self.increment_button
        } else {
            self.decrement_button
        }?;
        ui.get_child_recursive(me, id)
    }

    fn widget_size(&self, ui: &UiSystem) -> (i32, i32) {
        self.widget
            .and_then(|w| ui.node(w))
            .map_or((0, 0), |n| (n.region.box_.width(), n.region.box_.height()))
    }

    /// Behavior: the track, less the thumb, which is the distance the
    /// thumb's **origin** can travel.
    #[must_use]
    pub fn active_size(&self, ui: &UiSystem, me: ElemHandle) -> (i32, i32) {
        let a = self.scrolling_area;
        let (mut w, mut h) = (a.x1 - a.x0, a.y1 - a.y0);
        let (ww, wh) = self.widget_size(ui);
        if !Self::bool_attr(ui, me, attr::CENTRED_STOPS) {
            w -= ww;
            h -= wh;
        }
        (w, h)
    }

    /// Behavior: a point in the bar's own coordinates to a position.
    ///
    /// ```text
    /// (aw, ah) = the active size
    /// x = (pt.x - area.left) - thumb_width  / 2;  clamp(x, 0, aw - 1)
    /// y = (pt.y - area.top)  - thumb_height / 2;  clamp(y, 0, ah - 1)
    /// v    = horizontal ? x      : y
    /// span = horizontal ? aw - 1 : ah - 1
    /// return span < 2 ? 0.0 : v / span
    /// ```
    ///
    /// The half-thumb subtraction is what makes the thumb follow the pointer's **centre**, and
    /// the `span < 2` guard is what makes a bar with nothing to scroll answer 0 for every
    /// point rather than dividing by zero.
    #[must_use]
    pub fn point_to_position(&self, ui: &UiSystem, me: ElemHandle, x: i32, y: i32) -> f32 {
        let (aw, ah) = self.active_size(ui, me);
        let (ww, wh) = self.widget_size(ui);
        let a = self.scrolling_area;
        let px = ((x - a.x0) - ww / 2).clamp(0, (aw - 1).max(0));
        let py = ((y - a.y0) - wh / 2).clamp(0, (ah - 1).max(0));
        let (v, span) = if self.horizontal() {
            (px, aw - 1)
        } else {
            (py, ah - 1)
        };
        if span < 2 {
            return 0.0;
        }
        f(v) / f(span)
    }

    /// Behavior: [`Self::point_to_position`]'s inverse, which is why
    /// its truncated operand is `position · (span − 1)`: with it, and only with it, a thumb
    /// moved to a position reports that same position back when the pointer is over its
    /// centre. The coordinate off the bar's axis is **0**, not the track's origin.
    #[must_use]
    pub fn position_to_widget_x0y0(&self, ui: &UiSystem, me: ElemHandle, pos: f32) -> (i32, i32) {
        let (aw, ah) = self.active_size(ui, me);
        let a = self.scrolling_area;
        if self.horizontal() {
            (a.x0 + ftol(pos * f((aw - 1).max(0))), 0)
        } else {
            (0, a.y0 + ftol(pos * f((ah - 1).max(0))))
        }
    }

    /// Behavior: resolve the thumb, measure the track, size the thumb when
    /// the bar is proportional and put it where the position says.
    ///
    /// ```text
    /// if no thumb: thumb = the direct child with id 1
    /// if disallow-updating: return
    /// pos = float attribute 0x86
    /// if int attribute 0x7D is present and nonzero and int attribute 0x87 is absent:
    ///     set the position (pos, not smooth); return     // stops, but no stop chosen yet
    /// update the scrolling area
    /// if thumb:
    ///     if proportional:
    ///         prop = float attribute 0x88 (default 1.0)
    ///         min  = int attribute 0x89 (default 0)
    ///         span = horizontal ? area.width : area.height
    ///         size = max((long)(prop * span), min)
    ///         thumb.resize_to(horizontal ? size : width, horizontal ? height : size)
    ///     thumb.move_to(position_to_thumb_origin(pos))
    /// ```
    pub fn update_layout(&mut self, ui: &mut UiSystem, me: ElemHandle) {
        if self.widget.is_none() {
            self.widget = ui.get_child(me, WIDGET_ID);
        }
        if self.bits & bits::DISALLOW_UPDATING != 0 {
            return;
        }
        let pos = Self::position(ui, me);
        let has_stops = Self::int_attr(ui, me, attr::STOP_COUNT).is_some_and(|n| n != 0);
        if has_stops && Self::int_attr(ui, me, attr::STOP).is_none() {
            self.set_scrollbar_position(ui, me, pos);
            return;
        }
        self.update_scrolling_area(ui, me);
        // Retail's visibility tail also runs when the optional thumb is absent.
        if let Some(w) = self.widget {
            if self.bits & bits::PROPORTIONAL != 0 {
                let prop = ui
                    .node(me)
                    .and_then(|n| n.merged_properties().get_float(attr::PROPORTION))
                    .unwrap_or(1.0);
                let min = Self::int_attr(ui, me, attr::MIN_WIDGET_SIZE).unwrap_or(0);
                let a = self.scrolling_area;
                let span = if self.horizontal() {
                    a.x1 - a.x0
                } else {
                    a.y1 - a.y0
                };
                let size = ftol(prop * f(span)).max(min);
                let (ww, wh) = self.widget_size(ui);
                if self.horizontal() {
                    ui.resize_to(w, size, wh);
                } else {
                    ui.resize_to(w, ww, size);
                }
                // The thumb changed length, so the distance its origin may travel changed with it.
                self.update_scrolling_area(ui, me);
            }
            let (x, y) = self.position_to_widget_x0y0(ui, me, pos);
            ui.move_to(w, x, y);
        }
        // The scrollbar's layout update: visible unless
        // both disabled and hide-disabled are set. Do not hide an overflowing list's bar.
        ui.set_visible(
            me,
            self.bits & (bits::DISABLED | bits::HIDE_DISABLED)
                != (bits::DISABLED | bits::HIDE_DISABLED),
        );
    }

    /// The scrollbar's position setter:
    ///
    /// ```text
    /// pos = clamp(pos, 0.0, 1.0)
    /// stop = position_to_stop(pos)
    /// if stop >= 0: set the stop (stop, smooth); return
    /// float attribute 0x86 = pos
    /// broadcast message 10 with ((long)(pos * 1000), -1)
    /// ```
    pub fn set_scrollbar_position(&mut self, ui: &mut UiSystem, me: ElemHandle, pos: f32) {
        let pos = pos.clamp(0.0, 1.0);
        let stop = Self::position_to_stop(ui, me, pos);
        if stop >= 0 {
            self.set_scrollbar_stop(ui, me, stop);
            return;
        }
        self.write_position(ui, me, pos, u32::MAX);
    }

    /// Behavior: the same, with the stop index in `p2`.
    pub fn set_scrollbar_stop(&mut self, ui: &mut UiSystem, me: ElemHandle, stop: i32) {
        let pos = Self::stop_to_position(ui, me, stop);
        if pos < 0.0 {
            return;
        }
        ui.set_attribute_int(me, attr::STOP, stop);
        #[allow(clippy::cast_sign_loss)]
        self.write_position(ui, me, pos, stop as u32);
    }

    fn write_position(&mut self, ui: &mut UiSystem, me: ElemHandle, pos: f32, p2: u32) {
        ui.set_attribute_float(me, attr::POSITION, pos);
        // The write above cannot re-enter this element's own `on_set_attribute` — its behaviour
        // is lifted for the duration of the dispatch that got here — so the layout that
        // attribute 0x86 drives is run explicitly, exactly where the client's attribute handler
        // would have run it.
        self.update_layout(ui, me);
        #[allow(clippy::cast_sign_loss)]
        // The client multiplies the float position by 1000 and truncates, with no intervening
        // float store.
        let p1 =
            dereth_primitives::num::to_i32_f64(f64::from(pos) * f64::from(POSITION_SCALE)) as u32;
        ui.broadcast_element_message(me, msgid::SCROLL_POSITION, p1, p2);
    }

    /// Behavior: one arrow click's worth of movement.
    ///
    /// ```text
    /// if int attribute 0x87 (the stop) is present: set the stop (stop + delta, smooth); return
    /// broadcast message (delta < 1) + 0x0D with (delta, 0)
    /// ```
    ///
    /// **A bar with no stop locations does not move itself**: it tells its owner which way the
    /// player asked to go and the owner scrolls the content and writes the bar back. That is
    /// why a scrollbar over a list nobody has wired looks alive and does nothing.
    pub fn handle_move_steps(&mut self, ui: &mut UiSystem, me: ElemHandle, delta: i32) {
        if let Some(stop) = Self::int_attr(ui, me, attr::STOP) {
            self.set_scrollbar_stop(ui, me, stop + delta);
            return;
        }
        let id = if delta < 1 {
            msgid::SCROLL_PAGE_UP
        } else {
            msgid::SCROLL_PAGE_DOWN
        };
        #[allow(clippy::cast_sign_loss)]
        ui.broadcast_element_message(me, id, delta as u32, 0);
    }

    /// Behavior: what one wheel click does to a
    /// bar, once [`crate::scrollable::Scrollable::wheel_target`] has chosen it.
    ///
    /// ```text
    /// up = (action == 5)
    /// if float attribute 0x86 is absent: return                   // no position, no wheel
    /// if flag bit 0x100:                                          // has stop locations
    ///     move steps by (horizontal ? -1 : 1) * (up ? 1 : -1)
    /// else:
    ///     broadcast message 0xE - up with (0, 0)                  // 0x0D up, 0x0E down
    /// ```
    ///
    /// Three details worth stating, because each is invisible and each would still "work":
    ///
    /// * **The `0x86` read is a guard, not a value.** The position it fetches is never used.
    ///   A bar carrying no `0x86` attribute does nothing at all on the wheel.
    /// * **A horizontal bar wheels the other way.** Flag bit `0x1` is [`bits::HORIZONTAL`]
    ///   and it negates the step. It is only reachable through the stops arm, and
    ///   [`crate::scrollable::Scrollable::wheel_target`] hands the wheel to the *vertical* bar
    ///   only — so this branch is transcribed and, through that route, unreachable. Kept
    ///   because it is the client's; said out loud because "unfalsifiable here" is not
    ///   "wrong".
    /// * **The two ids are the arrow pair, not the page pair.** Wheel up raises `0x0D`, the
    ///   same id the increment arrow raises, so one wheel click moves exactly one
    ///   `inq_scroll_delta` step rather than a page. The move-steps handler raises
    ///   `(delta < 1) + 0x0D` off the same `+1`/`-1`, so the two arms agree on direction, as
    ///   they must.
    ///
    /// **The return value is this crate's, not the client's, and it guards the lifted-slot
    /// hazard.**
    /// The broadcast below is faithful and is kept, but its one listener that matters is the
    /// scrollable that called in here — whose own behaviour is lifted out of the arena for the
    /// duration, so the delivery reaches an empty slot and is silently dropped. The id is
    /// therefore handed back as well, and the caller applies it to itself with `&mut self`.
    /// A wheel that raised the right message and moved nothing looks exactly like a missing
    /// wire, which is why this is spelled out rather than left to the broadcast.
    pub fn handle_mouse_wheel(
        &mut self,
        ui: &mut UiSystem,
        me: ElemHandle,
        up: bool,
    ) -> Option<crate::MessageId> {
        let props = ui.node(me).map(|n| n.merged_properties())?;
        props.get_float(attr::POSITION)?;
        if self.bits & bits::HAS_STOPS != 0 {
            let sign = if self.bits & bits::HORIZONTAL != 0 {
                -1
            } else {
                1
            };
            let step = sign * if up { 1 } else { -1 };
            self.handle_move_steps(ui, me, step);
            // The stops arm moves the bar itself and raises `0x0A`, not `0x0D`/`0x0E`; there
            // is no step message for the caller to consume. Unreachable through the wheel in
            // the shipped data (no scrollable's `0x72` names a bar with stop locations), and
            // kept because it is the client's.
            return None;
        }
        let id = if up {
            msgid::SCROLL_PAGE_DOWN
        } else {
            msgid::SCROLL_PAGE_UP
        };
        ui.broadcast_element_message(me, id, 0, 0);
        Some(id)
    }

    /// Behavior: a click on the track, before or after the thumb.
    ///
    /// It broadcasts message `0x10 - (clicked before the thumb)` with `(0, 0)`, and nothing
    /// at all when the click was on the thumb itself.
    pub fn handle_page_click(&mut self, ui: &mut UiSystem, me: ElemHandle, x: i32, y: i32) {
        let Some(w) = self.widget.and_then(|w| ui.node(w)).map(|n| n.region.box_) else {
            return;
        };
        let (at, start, len) = if self.horizontal() {
            (x, w.x0, w.width())
        } else {
            (y, w.y0, w.height())
        };
        if at < start {
            ui.broadcast_element_message(me, msgid::SCROLL_LINE_DOWN, 0, 0);
        } else if start + len < at {
            ui.broadcast_element_message(me, msgid::SCROLL_LINE_UP, 0, 0);
        }
    }

    /// Behavior: put the position where the pointer is.
    pub fn scroll_to_point(&mut self, ui: &mut UiSystem, me: ElemHandle, x: i32, y: i32) {
        let pos = self.point_to_position(ui, me, x, y);
        self.set_scrollbar_position(ui, me, pos);
    }

    /// Start a widget drag: a move-to-touched bar jumps to the press first, then both
    /// kinds record where the drag began and what the position was, and arm the drag.
    pub fn start_widget_drag(&mut self, ui: &mut UiSystem, me: ElemHandle, x: i32, y: i32) {
        if self.move_to_touched() {
            self.scroll_to_point(ui, me, x, y);
        }
        self.drag_start = (x, y);
        self.reset_position = Self::position(ui, me);
        self.widget_drag_active = true;
    }

    /// Behavior: a thumb drag on a bar that does **not** jump to
    /// the pointer: the position moves by the drag's distance divided by the active length.
    pub fn scroll_n_pixels_from_reset(
        &mut self,
        ui: &mut UiSystem,
        me: ElemHandle,
        dx: i32,
        dy: i32,
    ) {
        let (aw, ah) = self.active_size(ui, me);
        let (d, span) = if self.horizontal() {
            (dx, aw)
        } else {
            (dy, ah)
        };
        if span == 0 {
            self.set_scrollbar_position(ui, me, 0.0);
            return;
        }
        let pos = f(d) / f(span) + self.reset_position;
        self.set_scrollbar_position(ui, me, pos);
    }

    /// Behavior: the smooth movement.
    ///
    /// A missing duration attribute `0x84`, or magnitude at most `0.0002`, does nothing. The
    /// client reads start position `0x86`, defaults end position to that start before reading
    /// `0x85`, stamps the current start time, sets animation bit `0x400000`, computes the end
    /// time, and registers for tick message 3 only when the bit was previously clear.
    ///
    /// Three details, each of which would still "work" if it were wrong:
    ///
    /// * **A bar with no `0x84` does not animate at all** — the duration read is a presence
    ///   test as well as a value, and there is no default. So does a bar whose duration is
    ///   `0.0`: the guard is `|d| > 0.0002`.
    /// * **The end position defaults to the start**, so a `0x85` write that fails to read
    ///   back glides nowhere rather than to zero.
    /// * **The registration is edge-triggered.** Re-targeting a bar that is already animating
    ///   re-aims it without a second global-message registration, because the message-3
    ///   listener table is a set and a second unregister would have to be matched.
    pub fn start_animation(&mut self, ui: &mut UiSystem, me: ElemHandle) {
        let props = ui.node(me).map(|n| n.merged_properties());
        let Some(duration) = props
            .as_ref()
            .and_then(|p| p.get_float(attr::ANIM_DURATION))
        else {
            return;
        };
        // The client's guard, written out, is `|d| > 0.0002`.
        // NaN takes the return, as it does in the client: neither comparison holds.
        if !matches!(
            duration.abs().partial_cmp(&0.0002),
            Some(std::cmp::Ordering::Greater)
        ) {
            return;
        }
        let was = self.bits & bits::ANIMATING != 0;
        self.anim_start_pos = props
            .as_ref()
            .and_then(|p| p.get_float(attr::POSITION))
            .unwrap_or(0.0);
        self.anim_end_pos = props
            .as_ref()
            .and_then(|p| p.get_float(attr::ANIM_TARGET))
            .unwrap_or(self.anim_start_pos);
        self.anim_start_time = ui.now.0;
        self.anim_end_time = ui.now.0 + f64::from(duration);
        self.bits |= bits::ANIMATING;
        if !was {
            ui.want_tick(me, true);
        }
    }

    /// The per-frame loop's second half — the animation's per-frame step.
    ///
    /// ```text
    /// if flag bit 0x400000:
    ///     t = (now - anim_start_time) / (anim_end_time - anim_start_time)
    ///     if t >= 1.0: clear bit 0x400000; t = 1.0; unregister from global message 3
    ///     float attribute 0x86 = (anim_end_pos - anim_start_pos) * t + anim_start_pos
    /// ```
    ///
    /// **`t` is not clamped below**, only above, which is the client's own arithmetic and is
    /// why the start time is taken from the same clock the tick carries.
    ///
    /// The write is the bare float attribute `0x86` and **not** the position setter, so a
    /// gliding bar raises **no** `0x0A` and its owner's offset does not follow it. That is
    /// deliberate in the client: the scrollbar-position update drives the bar *from*
    /// the offset, and `0x85` is written by a screen that already knows where the content is
    /// going — the character-generation profession page's `0x12`/`0x44` arm,
    /// which is a number typed into an attribute box, and
    /// the combat window. Those two are the only writers of `0x85` in the client.
    pub fn advance_animation(&mut self, ui: &mut UiSystem, me: ElemHandle) {
        if self.bits & bits::ANIMATING == 0 {
            return;
        }
        let span = self.anim_end_time - self.anim_start_time;
        #[allow(clippy::cast_possible_truncation)]
        // LINT-OK: the client's own `(float)` of a double ratio; the value is a 0..1 fraction.
        let mut t = ((ui.now.0 - self.anim_start_time) / span) as f32;
        if t >= 1.0 {
            self.bits &= !bits::ANIMATING;
            t = 1.0;
            ui.want_tick(me, false);
        }
        let pos = (self.anim_end_pos - self.anim_start_pos).mul_add(t, self.anim_start_pos);
        ui.set_attribute_float(me, attr::POSITION, pos);
        // The write above cannot re-enter this element's own `on_set_attribute` — its behaviour
        // is lifted for the duration of the global-message dispatch that got here — so the
        // layout attribute 0x86 drives is run explicitly. Same reason as `write_position`.
        self.update_layout(ui, me);
    }

    /// The default-hot-click setup:
    ///
    /// ```text
    /// bool attribute 0x0F = true                                  // auto-repeat
    /// if float attribute 0x10 is absent: set it to 0.5            // first delay
    /// if float attribute 0x11 is absent: set it to 0.125          // then every
    /// ```
    ///
    /// **This is what makes an arrow do anything at all.** `Scrollbar` listens for
    /// its arrows on element message `0x02`, the *hot* click, and a button raises `0x02` only
    /// when attribute `0x0F` is set — which no shipped scrollbar layout sets, because the
    /// element sets it on its own children. Without it an arrow raises `0x01` and the bar,
    /// which has no `0x01` arm, ignores it. The bar's initialisation calls it on **the bar
    /// itself** as well, unless the bar is move-to-touched, which is how a click on the empty
    /// track auto-repeats.
    pub fn setup_default_hot_click(ui: &mut UiSystem, e: ElemHandle) {
        use crate::props::attr as base;
        ui.set_attribute_bool(e, base::HOT_CLICK, true);
        let props = ui.node(e).map(|n| n.merged_properties());
        // The float `0.5`, the delay before the *first*
        // repeat.
        if props
            .as_ref()
            .and_then(|p| p.get_float(base::HOT_CLICK_FIRST_INTERVAL))
            .is_none()
        {
            ui.set_attribute_float(e, base::HOT_CLICK_FIRST_INTERVAL, 0.5);
        }
        // The float `0.125`, the period of every repeat
        // after the first. `Button`'s auto-repeat clock reads it; without it `Button` would
        // repeat once per frame.
        if props
            .as_ref()
            .and_then(|p| p.get_float(base::HOT_CLICK_REPEAT_INTERVAL))
            .is_none()
        {
            ui.set_attribute_float(e, base::HOT_CLICK_REPEAT_INTERVAL, 0.125);
        }
    }

    /// The message's window point in the **bar's** own coordinates, which is what every
    /// arithmetic above wants. The client does the same conversion by
    /// adding the source child's box origin when its parent is this bar.
    fn local(ui: &UiSystem, me: ElemHandle, m: &ElementMessage) -> (i32, i32) {
        let (ox, oy) = ui.screen_origin(me);
        let (x, y) = m.point.window;
        (x - ox, y - oy)
    }
}

/// The scrollable's wheel arm reaches the bar through
/// a type-`0x0B` downcast and then calls a **member** on it, so the bar's own state has to
/// be in hand. This lifts it out of the arena for the call and puts it back.
///
/// **It is a different element from the one dispatching**, which is what makes the lift legal:
/// the scrollable's slot is the empty one, the bar's is not. Handing a *fresh*
/// [`Scrollbar`] the call instead would lose the flag word — and flag bit `0x100` is
/// exactly what the wheel handler branches on, so the stops arm would never
/// be taken. A `None` from the downcast is the client's downcast returning null, and the
/// client does nothing in that case; so does this.
pub(crate) fn wheel(ui: &mut UiSystem, bar: ElemHandle, up: bool) -> Option<crate::MessageId> {
    let mut b = ui.take_behaviour(bar)?;
    let id = b
        .as_any_mut()
        .and_then(|a| a.downcast_mut::<Scrollbar>())
        .and_then(|sb| sb.handle_mouse_wheel(ui, bar, up));
    ui.put_behaviour(bar, b);
    id
}

impl Element for Scrollbar {
    /// Behavior: the type query returns this scrollbar for type `0x0B` and null otherwise.
    /// Its one caller is the scrollable wheel arm; see [`wheel`].
    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }

    /// The original mouse-visibility query always returns true for the five mouse-driven
    /// element types: button, menu, dragbar, resizebar, and scrollbar.
    fn should_be_mouse_visible(&self) -> bool {
        true
    }

    /// The original scrollbar shares button, text, and scrolling mouse-down behavior. A press
    /// on a bar or one of its arrow buttons moves the focus element.
    fn takes_focus_on_press(&self) -> bool {
        true
    }

    /// Post-initialization runs the base and then the first layout update, which is
    /// what puts the thumb in the track at all. Without it the shipped chat bar draws an
    /// empty track: the thumb is a child element whose layout box is a placeholder.
    fn post_init(&mut self, ctx: &mut ElemCtx<'_>) {
        self.button.post_init(ctx);
        let me = ctx.me;
        // The button-id update's tail and the bar's initialisation, both of which are
        // `setup_default_hot_click`. They run in the client at attribute-set and initialisation
        // time; here they run at `post_init`, which is the first moment the arrows exist as
        // children — the client's own answer to the same problem is the button-id update's
        // "not found: set the retry bit and try again from the per-frame loop".
        for id in [self.increment_button, self.decrement_button]
            .into_iter()
            .flatten()
        {
            if let Some(b) = ctx.ui.get_child_recursive(me, id) {
                Self::setup_default_hot_click(ctx.ui, b);
            }
        }
        if !self.move_to_touched() {
            Self::setup_default_hot_click(ctx.ui, me);
            // The write above cannot reach this element's own `Button` — its behaviour is
            // lifted for the duration of `post_init` — so the flag the mouse-down reads is set
            // here as well. `on_set_attribute` does it for every other element.
            self.button.hot_click_enabled = true;
        }
        self.update_layout(ctx.ui, me);
    }

    /// Behavior: the base first, then this type's own
    /// switch. Every arm that changes a length or a flag ends in the layout update.
    fn on_set_attribute(
        &mut self,
        ctx: &mut ElemCtx<'_>,
        id: u32,
        v: Option<&crate::PropertyValue>,
    ) {
        self.button.on_set_attribute(ctx, id, v);
        let b =
            |v: Option<&crate::PropertyValue>| matches!(v, Some(crate::PropertyValue::Bool(true)));
        let me = ctx.me;
        let mut relayout = true;
        match id {
            attr::DISALLOW_UPDATING => {
                set_bit(&mut self.bits, bits::DISALLOW_UPDATING, b(v));
                relayout = false;
            }
            attr::DISABLED => set_bit(&mut self.bits, bits::DISABLED, b(v)),
            attr::HIDE_WHEN_DISABLED => set_bit(&mut self.bits, bits::HIDE_DISABLED, b(v)),
            attr::HORIZONTAL => set_bit(&mut self.bits, bits::HORIZONTAL, b(v)),
            attr::PROPORTIONAL => set_bit(&mut self.bits, bits::PROPORTIONAL, b(v)),
            attr::SMOOTH_MOVEMENT => {
                set_bit(&mut self.bits, bits::SMOOTH_MOVEMENT, b(v));
                relayout = false;
            }
            attr::NOTIFY_DISABLED => set_bit(&mut self.bits, bits::NOTIFY_DISABLED, b(v)),
            attr::HAS_STOP_LOCATIONS => {
                set_bit(&mut self.bits, bits::HAS_STOPS, b(v));
                relayout = false;
            }
            // Has-stop-locations = `v != 0` — a stop **count** is what turns stops on.
            attr::STOP_COUNT => {
                let n = matches!(v, Some(crate::PropertyValue::Integer(n)) if *n != 0);
                set_bit(&mut self.bits, bits::HAS_STOPS, n);
                relayout = false;
            }
            attr::INCREMENT_BUTTON => {
                self.increment_button = element_id(v);
                relayout = false;
            }
            attr::DECREMENT_BUTTON => {
                self.decrement_button = element_id(v);
                relayout = false;
            }
            // Move-to-touched hides the thumb from mouse input and disables hot-clicking on
            // the bar, because the bar takes the press itself.
            attr::MOVE_TO_TOUCHED => {
                set_bit(&mut self.bits, bits::MOVE_TO_TOUCHED, b(v));
                relayout = false;
            }
            attr::POSITION => {
                // "clamp it, and write the clamp back when it moved by more than 0.0002".
                let Some(crate::PropertyValue::Float(p)) = v else {
                    return;
                };
                let clamped = self.validate_position(ctx.ui, me, *p);
                if (clamped - *p).abs() >= 0.0002 {
                    // The client re-enters its attribute handler here and falls through to
                    // the layout update on the second pass; this element's behaviour is lifted,
                    // so the second pass cannot happen and the layout is run directly.
                    ctx.ui.set_attribute_float(me, attr::POSITION, clamped);
                    self.update_layout(ctx.ui, me);
                    return;
                }
            }
            // The `0x85` case — the goal position. Without it, writing `0x85`
            // would fall through to the `_` arm and do *nothing at all*, so the two screens
            // that use it would move no thumb rather than moving it instantly.
            //
            // Without flag bit `0x10` (smooth movement) the goal is written straight to float
            // attribute `0x86`; with it, `start_animation` runs.
            attr::ANIM_TARGET => {
                let Some(crate::PropertyValue::Float(goal)) = v else {
                    return;
                };
                if self.bits & bits::SMOOTH_MOVEMENT == 0 {
                    let goal = *goal;
                    ctx.ui.set_attribute_float(me, attr::POSITION, goal);
                    // As everywhere else in this type: the write cannot re-enter this
                    // element's own `on_set_attribute`, so the layout `0x86` drives is run
                    // here.
                    self.update_layout(ctx.ui, me);
                    return;
                }
                self.start_animation(ctx.ui, me);
                return;
            }
            attr::PROPORTION | attr::MIN_WIDGET_SIZE | attr::STOP => {}
            _ => relayout = false,
        }
        if relayout {
            self.update_layout(ctx.ui, me);
        }
    }

    fn as_text_mut(&mut self) -> Option<&mut crate::text::TextElement> {
        self.button.as_text_mut()
    }

    fn compose_text(&self, screen: crate::Box2D) -> Vec<crate::text::PlacedGlyph> {
        self.button.compose_text(screen)
    }

    /// Delegated for the same reason [`Self::compose_text`] is:
    /// the original button, menu, and scrollbar share text-control outline behavior. This
    /// rebuild delegates the same element outline state through composition.
    ///
    /// Without this the outline would be drawn for a plain label and silently not
    /// for a button carrying the same attribute. 16 shipped elements are exactly that case.
    fn text_outline_color(&self) -> Option<u32> {
        self.button.text_outline_color()
    }

    /// Delegated for the same reason the two above are: the selection
    /// belongs to the original shared text behavior, so its color-inversion draw arm applies
    /// to this element too.
    fn selection_boxes(&self, screen: crate::Box2D) -> Vec<crate::Box2D> {
        self.button.selection_boxes(screen)
    }

    /// Delegated with the three above: the draw's caret arm is
    /// `TextElement`'s, so it is this widget's too.
    fn caret(&self, screen: crate::Box2D) -> Option<(crate::Box2D, u32)> {
        self.button.caret(screen)
    }

    /// The scrollbar's element-message handler, whose four arms are the whole
    /// of a scrollbar's interaction:
    ///
    /// Hot-click message `0x02` steps for either arrow or pages when the bar itself sent it.
    /// Press `0x1C` begins a held drag on the thumb or move-to track; release `0x1D` ends an
    /// active left-button drag. Move `0x1E` updates the position while the secondary-coordinate
    /// distance stays below `0x65`, or restores the reset position and cancels when it leaves
    /// that range. Every arm then chains to the inherited button handler.
    ///
    /// Every arm chains to the button's, which is why a scrollbar still rolls over and still
    /// auto-repeats: the arrows *are* buttons and this type only decides what their repeat
    /// means.
    fn listen_to_element_message(&mut self, ctx: &mut ElemCtx<'_>, m: &ElementMessage) -> R {
        let me = ctx.me;
        if self.direct_wheel.is_some()
            && m.id == msgid::MOUSE_PRESS
            && matches!(
                m.p1,
                crate::focus::action::WHEEL_UP | crate::focus::action::WHEEL_DOWN
            )
        {
            if self.bits & bits::DISABLED == 0 {
                let up = m.p1 == crate::focus::action::WHEEL_UP;
                if self.direct_wheel == Some(DirectWheel::Percentage) {
                    let delta = if up == self.horizontal() { -1 } else { 1 };
                    let percent = (dereth_primitives::num::to_i32(
                        (Self::position(ctx.ui, me) * 100.0).round(),
                    ) + delta)
                        .clamp(0, 100);
                    #[allow(clippy::cast_precision_loss)]
                    ctx.ui
                        .set_attribute_float(me, attr::POSITION, percent as f32 / 100.0);
                    self.update_layout(ctx.ui, me);
                    // Carry the exact percentage through the thousandths notification;
                    // re-truncating its float could lose a second attribute point.
                    ctx.ui.broadcast_element_message(
                        me,
                        msgid::SCROLL_POSITION,
                        u32::try_from(percent * 10).unwrap_or(0),
                        u32::MAX,
                    );
                } else {
                    self.handle_mouse_wheel(ctx.ui, me, up);
                }
            }
            return R::StopProcessing;
        }
        let mine = self.widget == Some(m.source) || (m.source == me && self.move_to_touched());
        let held = ctx
            .ui
            .is_pressed_on(m.source, crate::focus::action::PRIMARY_CLICK)
            || ctx
                .ui
                .is_pressed_on(m.source, crate::focus::DOUBLE_CLICK_ACTIONS[0]);
        let is_left = m.p1 == crate::focus::action::PRIMARY_CLICK
            || m.p1 == crate::focus::DOUBLE_CLICK_ACTIONS[0];

        if m.id == msgid::BUTTON_HOT_CLICK {
            if Some(m.source_id) == self.increment_button {
                self.handle_move_steps(ctx.ui, me, 1);
                return R::DontDoDefault;
            }
            if Some(m.source_id) == self.decrement_button {
                self.handle_move_steps(ctx.ui, me, -1);
                return R::DontDoDefault;
            }
            if m.source == me {
                let (x, y) = Self::local(ctx.ui, me, m);
                self.handle_page_click(ctx.ui, me, x, y);
                return R::DontDoDefault;
            }
        } else if m.id == msgid::MOUSE_PRESS {
            if mine && held {
                let (x, y) = Self::local(ctx.ui, me, m);
                self.start_widget_drag(ctx.ui, me, x, y);
            } else if m.source == me && self.button.hot_click_enabled {
                // **A click on the empty track must be paged here.**
                //
                // The button's mouse-down raises a hot click (`0x02`) on **this same
                // element**, and the scrollbar's message handler turns it into a page click. Here
                // that broadcast is raised by `Button` from *inside this dispatch*, with this
                // element's behaviour lifted out of its arena slot, so
                // `UiSystem::deliver_element` drops it and the bar never sees its own press.
                // (An **arrow** works because the arrow is a different element, whose slot is
                // the lifted one while the bar's is occupied, so arrows scroll the log and a
                // track click would not.)
                //
                // The press is therefore paged here, at the same point in the same gesture the
                // client pages it. The auto-repeat that follows arrives on the tick as message
                // `0x02` from `Button::listen_to_global_message`, whose source is this element
                // but whose dispatch is not nested inside it, and is handled by the arm above.
                let (x, y) = Self::local(ctx.ui, me, m);
                self.handle_page_click(ctx.ui, me, x, y);
            }
        } else if m.id == msgid::MOUSE_RELEASE {
            if mine && is_left && self.widget_drag_active {
                self.widget_drag_active = false;
            }
        } else if m.id == msgid::MOUSE_MOVE && mine && held {
            let (x, y) = Self::local(ctx.ui, me, m);
            let secondary = if self.horizontal() { y } else { x };
            if secondary.abs() < DRAG_CANCEL_DISTANCE {
                if self.move_to_touched() {
                    self.scroll_to_point(ctx.ui, me, x, y);
                } else {
                    let (sx, sy) = self.drag_start;
                    self.scroll_n_pixels_from_reset(ctx.ui, me, x - sx, y - sy);
                }
                self.widget_drag_active = true;
            } else if self.widget_drag_active {
                let back = self.reset_position;
                self.set_scrollbar_position(ctx.ui, me, back);
                self.widget_drag_active = false;
            }
        }
        // The base's own arms: rollover, the press/release state and the auto-repeat.
        self.button.listen_to_element_message(ctx, m)
    }

    /// The scrollbar's global-message listener — the base, then, for message 3, the
    /// per-frame loop. The base's own message-3 arm is the button's
    /// auto-repeat, which is why an arrow keeps firing while it is held.
    fn listen_to_global_message(&mut self, ctx: &mut ElemCtx<'_>, id: MessageId, p: u32) {
        self.button.listen_to_global_message(ctx, id, p);
        if id == crate::msg::global::TICK {
            let me = ctx.me;
            self.advance_animation(ctx.ui, me);
        }
    }
}

/// An element-id-valued attribute: the layout stores them as enums.
fn element_id(v: Option<&crate::PropertyValue>) -> Option<ElementId> {
    match v {
        Some(crate::PropertyValue::Enum(e)) => Some(ElementId(*e)),
        Some(crate::PropertyValue::Integer(i)) => u32::try_from(*i).ok().map(ElementId),
        _ => None,
    }
}
