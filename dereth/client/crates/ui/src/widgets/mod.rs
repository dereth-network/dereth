//! The engine widget types.
//!
//! The containers (`Field`, `Button`, `Panel`, `GroupBox`, `Dragbar`, `Resizebar`,
//! `Viewport`) and the lists and indicators (`ListBox`, `Menu`, `Meter`, `Scrollbar`,
//! `ColorPicker`). Each
//! module's doc comment names the class, the id and the messages it raises.
//!
//! `TextElement`(0x0C) is large enough to have its own tree, [`crate::text`].
//!
//! The rule that shapes the animated ones: a widget registers for
//! [global message 3](crate::msg::global::TICK) **only while it has work** and unregisters the
//! moment it is idle.

use crate::element::{Element, ElementMessageListenResult as R};
use crate::msg::element::id as msgid;
use crate::{ElemCtx, ElemHandle, ElementMessage, ListenerId, MessageId, StateId, UiSystem};

impl UiSystem {
    /// Behavior: a `Dragbar` calls this on its **parent**, with the
    /// **screen** coordinates of the press.
    ///
    /// Unless the element is already resizing or moving, it records the press point, its own
    /// top-left corner and its height and width, then sets the moving flag (`0x40000`).
    ///
    /// **The six recorded values and the guard both matter.** Without the values
    /// [`Self::mouse_move_element`] has no origin to move from — a press on
    /// the radar's drag button would mark the radar as moving and no amount of pointer motion would
    /// move it. The guard is the client's: a second `start_movement` mid-drag (a second
    /// mouse button pressed on the same handle) would otherwise re-base the origin and make the
    /// window jump.
    pub fn start_movement(&mut self, h: ElemHandle, x: i32, y: i32) {
        let Some(n) = self.node(h) else { return };
        if n.flags.is_resizing() || n.flags.is_moving() {
            return;
        }
        let b = n.region.box_;
        if let Some(n) = self.node_mut(h) {
            n.movement = crate::element::MovementOrigin {
                mouse: (x, y),
                start: (b.x0, b.y0),
                size: (b.width(), b.height()),
            };
            n.flags.set_is_moving(true);
        }
    }

    /// Stop moving the element.
    pub fn stop_movement(&mut self, h: ElemHandle) {
        if let Some(n) = self.node_mut(h) {
            n.flags.set_is_moving(false);
        }
    }

    /// Behavior: the whole of what dragging a window by its
    /// handle does. `x`/`y` are **screen** coordinates.
    ///
    /// ```text
    /// x = drag_start.x - mouse_initial.x + mouse.x
    /// y = drag_start.y - mouse_initial.y + mouse.y
    /// if shift is down: snap both to a multiple of 10
    /// clamp x to 0 ..= parent.width  - width
    /// clamp y to 0 ..= parent.height - height
    /// move_to(x, y)          // the overridable move
    /// ```
    ///
    /// Two things are load-bearing:
    ///
    /// * **The clamp is against the parent**, so a window can never be dragged off the game view.
    ///   The movement helper repeats the same clamp for callers that do not come
    ///   through here.
    /// * **The move is overridable**, which lets the gameplay screen persist the new position
    ///   into the player's saved settings on the way past. This crate has no overridable move; the
    ///   screen that owns the window watches its box instead — see `GamePlayScreen::update_radar`.
    ///
    /// **Shift snaps to 10 pixels.** The modifier state comes from
    /// [`crate::InputPump::shift_key_down`], latched once a frame by [`Self::use_time`], which
    /// `mouse_resize_element` uses too.
    ///
    /// **The two axes are not symmetric, and the asymmetry is the client's:**
    ///
    /// ```text
    /// if shift is down:
    ///     x -= x % 10
    ///     if y < parent.height - height: y -= y % 10
    /// ```
    ///
    /// X snaps unconditionally; **Y snaps only while the window is not already at the parent's
    /// bottom edge**, and the test is made *before* the clamps below, against the unclamped `y`.
    /// A window resting on the bottom therefore keeps its exact bottom alignment instead of being
    /// pulled up to the next multiple of ten — which is the one place where snapping would
    /// visibly detach it from the edge it was dropped against.
    ///
    /// The remaining hop is one trait override in `dereth-client`; see
    /// [`crate::InputPump::shift_key_down`].
    pub fn mouse_move_element(&mut self, h: ElemHandle, x: i32, y: i32) {
        let Some(n) = self.node(h) else { return };
        let m = n.movement;
        let (w, ht) = (n.region.box_.width(), n.region.box_.height());
        let (pw, ph) = self
            .parent(h)
            .and_then(|p| self.node(p))
            .map_or((w, ht), |p| (p.region.box_.width(), p.region.box_.height()));
        let mut nx = m.start.0 - m.mouse.0 + x;
        let mut ny = m.start.1 - m.mouse.1 + y;
        if self.shift_key_down {
            nx -= nx % crate::layout::SHIFT_SNAP;
            if ny < ph - ht {
                ny -= ny % crate::layout::SHIFT_SNAP;
            }
        }
        let nx = nx.clamp(0, (pw - w).max(0));
        let ny = ny.clamp(0, (ph - ht).max(0));
        self.move_to(h, nx, ny);
    }

    /// Behavior: a `Resizebar` calls this on its parent with the
    /// border zone it represents **and the screen coordinates of the press**.
    ///
    /// Unless the element is already resizing or moving, it records its own top-left corner, its
    /// height and width and the press point; then, if `border` is not `BORDER_NONE`, it stores the
    /// border and sets the resizing flag (`0x80000`).
    ///
    /// **The two coordinates, the six recorded values and the guard all matter.** Without the
    /// values [`Self::mouse_resize_element`] has no origin to resize from — the same as
    /// `start_movement`, one function along.
    ///
    /// Note what the guard covers and what it does not: the six longs are latched **inside** it but
    /// **before** the `border != BORDER_NONE` test, so a `BORDER_NONE` call re-bases the origin
    /// without arming a resize. That is the client's order and it is kept.
    pub fn start_resizing(&mut self, h: ElemHandle, border: crate::BorderLocation, x: i32, y: i32) {
        let Some(n) = self.node(h) else { return };
        if n.flags.is_resizing() || n.flags.is_moving() {
            return;
        }
        let b = n.region.box_;
        if let Some(n) = self.node_mut(h) {
            n.movement = crate::element::MovementOrigin {
                mouse: (x, y),
                start: (b.x0, b.y0),
                size: (b.width(), b.height()),
            };
            if border != crate::BorderLocation::None {
                n.current_border = border;
                n.flags.set_is_resizing(true);
            }
        }
    }

    /// Stop resizing.
    ///
    /// The client clears **only** the flag (`flags &= 0xFFF7FFFF`); the current border is left
    /// where it was, and `mouse_resize_element` is reached only while the flag is set, so nothing
    /// reads the stale value. Clearing it here as well is a harmless tidy this crate already did,
    /// and it is kept so that a `current_border` outside a drag is unambiguously "not resizing".
    pub fn stop_resizing(&mut self, h: ElemHandle) {
        if let Some(n) = self.node_mut(h) {
            n.flags.set_is_resizing(false);
            n.current_border = crate::BorderLocation::None;
        }
    }

    /// Behavior: the whole of what dragging a window's border
    /// does. `x`/`y` are **screen** coordinates, as they are for [`Self::mouse_move_element`].
    ///
    /// The geometry, the four attribute clamps and the shift snap are
    /// [`crate::layout::mouse_resize`]; this is the part that needs the tree:
    ///
    /// ```text
    /// if current_border == BORDER_NONE: return
    /// ... the per-border arms, the four clamps, the shift snap, x>=0, y>=0 ...
    /// if parent.width  < right:  right  = parent.width
    /// if parent.height < bottom: bottom = parent.height
    /// resize_to(right - left, bottom - top)
    /// move_to(left, top)
    /// ```
    ///
    /// **The two parent clamps are `[inferred]`, and only the assignment is.** The comparison,
    /// both calls and the branch are certain; the store is not.
    /// There is exactly one value the call can be producing and one variable it can be
    /// producing it for. The clamp's *existence* and its direction are `\[verified\]`.
    ///
    /// **`resize_to` runs before `move_to`, which is the client's order — and swapping them changes
    /// nothing measurable in this build.** That is a correction to what this comment said when it
    /// was written: it claimed the order was load-bearing because `resize_to` re-anchors the
    /// children against the new size while the origin is still the old one. A mutation that
    /// swapped the two lines **survived every test in the crate**, and the reason is structural
    /// rather than a weak test: [`Self::update_for_parent_size_change`] recomputes each child from
    /// its **design** rectangle against the parent's final box, not incrementally, so both orders
    /// converge. In retail the order does matter for a different reason — resizing
    /// rebuilds the surface and forces a redraw, which a D3D12 rebuild that redraws every frame
    /// does not have. Kept in the client's order because it is the client's; the claim that it is
    /// *observable* here is withdrawn. The code is
    /// right, the tests are right, and the explanation was wrong.
    pub fn mouse_resize_element(&mut self, h: ElemHandle, x: i32, y: i32) {
        let Some(n) = self.node(h) else { return };
        let border = n.current_border;
        if border == crate::BorderLocation::None {
            return;
        }
        let m = n.movement;
        let p = n.merged_properties();
        let clamps = crate::SizeClamps {
            min_w: p.get_int(crate::props::attr::MIN_WIDTH),
            max_w: p.get_int(crate::props::attr::MAX_WIDTH),
            min_h: p.get_int(crate::props::attr::MIN_HEIGHT),
            max_h: p.get_int(crate::props::attr::MAX_HEIGHT),
        };
        let (nx, ny, nw, nh) = crate::layout::mouse_resize(
            (m.start.0, m.start.1, m.size.0, m.size.1),
            border,
            x - m.mouse.0,
            y - m.mouse.1,
            clamps,
            self.shift_key_down,
        );
        let (mut right, mut bottom) = (nx + nw, ny + nh);
        if let Some(p) = self.parent(h).and_then(|p| self.node(p)) {
            let (pw, ph) = (p.region.box_.width(), p.region.box_.height());
            // INFERRED: the unrecovered store; see the doc comment.
            if pw < right {
                right = pw;
            }
            if ph < bottom {
                bottom = ph;
            }
        }
        self.resize_to(h, right - nx, bottom - ny);
        self.move_to(h, nx, ny);
    }

    /// Convenience for a widget that wants the frame tick: register or unregister as its own state
    /// requires, and never leave a stale registration behind.
    pub fn want_tick(&mut self, h: ElemHandle, want: bool) {
        let who = ListenerId::Element(h);
        if want {
            self.register_for_global_message(crate::msg::global::TICK, who);
        } else {
            self.unregister_for_global_message(crate::msg::global::TICK, who);
        }
    }
}

/// `Field` (3) — a plain rectangle or frame. It is also the base class of `Dialog`.
///
/// The rollover-state-change flag and the saved old state are **not** a mouse-over rollover. It is
/// the *drag-over* highlight and nothing else:
///
/// Leaving restores the saved state when drag rollover changed it. Entering does nothing unless a
/// drag is active and both drop-catcher attributes allow the test. The drag callback and disabled
/// flag then choose state 9 or 10; the original state is saved only on the first change.
///
/// So the two states are **9** ("this drop is allowed") and **10** ("it is not"), the trigger is a
/// live drag rather than a bare hover, and there is no state-1 rollover here at all — state 1 is
/// `Button`'s *resting* state ([`button::state`]). Moving a hovered field
/// into state 1 would give anything sharing a button's state records the wrong picture.
pub mod field {
    use super::*;

    /// The two states the field chooses between while a drag is in flight.
    pub mod state {
        use crate::StateId;
        /// The drop would be accepted.
        pub const DROP_OK: StateId = StateId(9);
        /// It would be refused — the drag-drop callback said no, or attribute 0x38 is set.
        pub const DROP_REFUSED: StateId = StateId(10);
    }

    #[derive(Debug, Default)]
    pub struct Field {
        /// Set while the drag highlight is showing, so the state it
        /// replaced can be put back exactly once.
        pub rollover_state_change: bool,
        /// The state restored when the drag leaves.
        pub old_state: StateId,
    }

    pub fn create(_l: &crate::LayoutDesc, _d: &crate::ElementDesc) -> Box<dyn Element> {
        Box::new(Field::default())
    }

    impl Element for Field {
        fn listen_to_element_message(&mut self, ctx: &mut ElemCtx<'_>, m: &ElementMessage) -> R {
            if m.source != ctx.me || m.id != msgid::MOUSE_OVER {
                return R::Default;
            }
            if m.p1 == 0 {
                if self.rollover_state_change {
                    let s = self.old_state;
                    self.rollover_state_change = false;
                    ctx.ui.queue_set_state(ctx.me, s);
                }
                return R::Default;
            }
            // Only while something is actually being dragged, and only on a drop catcher.
            if !ctx.ui.is_dragging() {
                return R::Default;
            }
            let Some(n) = ctx.ui.node(ctx.me) else {
                return R::Default;
            };
            let props = n.merged_properties();
            if !props
                .get_bool(crate::props::attr::DROP_CATCHER)
                .unwrap_or(false)
            {
                return R::Default;
            }
            let refused = props
                .get_bool(crate::props::attr::DROP_DISABLED)
                .unwrap_or(false)
                || !n.drop_catcher;
            let s = if refused {
                state::DROP_REFUSED
            } else {
                state::DROP_OK
            };
            if n.state != s {
                if !self.rollover_state_change {
                    self.rollover_state_change = true;
                    self.old_state = n.state;
                }
                ctx.ui.queue_set_state(ctx.me, s);
            }
            R::Default
        }
    }
}

/// `Button` (1) — a push button. Derives from `TextElement`, so it carries a caption.
///
/// Broadcasts element message **1** with `p1 = 7` on an ordinary click and **2** for a hot click.
/// Mouse-down alone raises nothing unless auto-repeat is enabled. Supports
/// **hot-clicking**: auto-repeat while held, driven by global message 3
/// (the hot-clicking flag and the next hot-click time).
pub mod button {
    use super::*;

    /// The auto-repeat interval used when the element carries neither attribute.
    ///
    /// **The auto-repeat interval is not a constant.** Both periods are read out of the element:
    /// the button's mouse-down takes attribute
    /// `attr::HOT_CLICK_FIRST_INTERVAL` (`0x10`) for the delay before the first repeat, and its
    /// global-message listener takes `attr::HOT_CLICK_REPEAT_INTERVAL` (`0x11`) for every repeat
    /// after it. This value is used only for a button that carries neither — which the client's
    /// float-attribute read leaves **undefined**, so any fallback at all is more defined than the
    /// original.
    ///
    /// **This is a declared deviation and it is unreachable on a scrollbar**, because
    /// the scrollbar's default-hot-click setup writes both attributes on the bar
    /// and on each of its arrows before any of them can be pressed. It can only be reached by a
    /// plain `Button` whose layout sets `0x0F` and neither interval.
    pub const FALLBACK_INTERVAL: f64 = 0.1;

    /// The seven states the button's state update chooses between.
    ///
    /// **These are verified, and they are not the generic four in [`crate::element::state`].**
    ///
    /// ```text
    /// update_state:
    ///     disabled = bool attribute 0x0D
    ///     if disabled: s = 0x0D
    ///     else:
    ///         toggled  = bool attribute 0x0E
    ///         rollover = bool attribute 0x13
    ///         base = toggled ? 6 : 1
    ///         s = base                                            // 1, or 6 when toggled
    ///         if pressed or (rollover and mouse_over_top): s = base + 1   // 2, or 7
    ///         if pressed and mouse_over_top:               s = base + 2   // 3, or 8
    ///     if the layout has a state record for s: set_state(s)
    /// ```
    ///
    /// The state-record guard is load-bearing: a button whose layout defines no record for
    /// the state it computed **stays where it is** rather than losing its picture.
    pub mod state {
        use crate::StateId;
        /// Resting.
        pub const NORMAL: StateId = StateId(1);
        /// Moused over, when attribute 0x13 is set.
        pub const ROLLOVER: StateId = StateId(2);
        /// Held down with the pointer still over it.
        pub const PRESSED: StateId = StateId(3);
        /// Resting, toggled on.
        pub const TOGGLED: StateId = StateId(6);
        /// Moused over, toggled on.
        pub const TOGGLED_ROLLOVER: StateId = StateId(7);
        /// Held down, toggled on.
        pub const TOGGLED_PRESSED: StateId = StateId(8);
        /// Disabled — attribute 0x0D. The meter's "nothing to show" media state is the same id.
        pub const DISABLED: StateId = StateId(0x0D);
    }

    #[derive(Debug, Default)]
    pub struct Button {
        /// The original button inherits text-control behavior; this rebuild composes it explicitly.
        /// That is why a button's caption
        /// is attribute **0x17** on the button itself and not a child element: the caption *is* the
        /// text element's glyph list. The relevant text behavior is delegated below.
        pub text: crate::text::TextElement,
        /// The mouse was pressed on this button and has not been released.
        pub pressed: bool,
        /// Hot-clicking (auto-repeat) is in progress.
        pub hot_clicking: bool,
        /// The time of the next hot click, in the client timer's scale,
        /// which for this crate is [`UiSystem::now`](crate::UiSystem::now).
        ///
        /// **Written by the mouse-down and advanced by the tick.** Without it the tick arm would
        /// fire once per frame, so a held arrow would scroll at the frame rate.
        pub next_hot_click: f64,
        /// Whether this button auto-repeats at all.
        pub hot_click_enabled: bool,
        /// Hot-clicking-in-progress as the mouse-up saw it, carried from 0x1D to 0x19.
        pub suppress_click: bool,
    }

    pub fn create(_l: &crate::LayoutDesc, _d: &crate::ElementDesc) -> Box<dyn Element> {
        Box::new(Button::default())
    }

    impl Button {
        /// One of the two float-attribute reads that decide the auto-repeat cadence —
        /// `0x10` before the first repeat, `0x11` before every one after it.
        ///
        /// The client widens the `float` it reads to a `double` before adding, which is what this
        /// cast is.
        ///
        /// **The `unwrap_or` is a declared deviation**: retail's float-attribute read returns
        /// `false` on an absent attribute and leaves its out-parameter undefined, so there is no
        /// faithful answer to transcribe. See [`FALLBACK_INTERVAL`] for why
        /// no scrollbar can reach it.
        fn interval(&self, ctx: &ElemCtx<'_>, id: u32) -> f64 {
            ctx.ui
                .node(ctx.me)
                .and_then(|n| n.merged_properties().get_float(id))
                .map_or(FALLBACK_INTERVAL, f64::from)
        }

        /// The button's state update — the whole of it; see [`state`].
        ///
        /// Called from its post-init, its attribute setter for **0x0D, 0x0E and
        /// 0x13**, its mouse-down, its mouse-up and its mouse-over-top —
        /// every one of the five, because leaving one out leaves a button stuck in the wrong
        /// picture.
        pub fn update_state(&self, ctx: &mut ElemCtx<'_>) {
            use crate::props::attr;
            let Some(n) = ctx.ui.node(ctx.me) else { return };
            let props = n.merged_properties();
            let mouse_over_top = n.region.flags.mouse_over_top;
            let s = if props.get_bool(attr::DISABLED).unwrap_or(false) {
                state::DISABLED
            } else {
                // `(-(toggled != 0) & 5) + 1` — 1 or 6, +1 for rollover, +2 for pressed-over.
                let base = if props.get_bool(attr::TOGGLED).unwrap_or(false) {
                    6
                } else {
                    1
                };
                let rollover = props.get_bool(attr::ROLLOVER_HIGHLIGHT).unwrap_or(false);
                let mut s = base;
                if self.pressed || (rollover && mouse_over_top) {
                    s = base + 1;
                }
                if self.pressed && mouse_over_top {
                    s = base + 2;
                }
                StateId(s)
            };
            // Set the state only if the layout has a record for it. State 0 is the
            // element's own base record, which answers none for state 0. The state update never
            // asks for 0, so the guard only ever refuses a state the
            // layout does not define.
            if ctx
                .ui
                .node(ctx.me)
                .is_some_and(|n| n.desc.access_state(s).is_some())
            {
                ctx.ui.queue_set_state(ctx.me, s);
            }
        }

        /// The button's mouse-down gate — **which action is a press at all.**
        ///
        /// Without it the mouse wheel over a radio button toggles it.
        /// It is three tests, easy to misread as two:
        ///
        /// ```text
        ///   is_left = action == 7
        ///   if the element does not want double clicks (flag bit 23) and action == 0x0A:
        ///       treat it as a press and go on to the two tests below
        ///   else if not is_left: return
        ///   if resizing (flag bit 19): return
        ///   if moving (flag bit 18):   return
        /// ```
        ///
        /// So actions **5** and **6** — `DIMOFS_Z[+]` and `DIMOFS_Z[-]`, the two wheel detents —
        /// never set the pressed flag and never reach the state update. The same shape is
        /// on this build's other two kinds that take a press:
        /// the text element's mouse-down (the same comparison, after the
        /// same two bitfield tests) and the list box's mouse-down
        /// (which tests 7 and 0x0A), and the smart-box wrapper's mouse-down switches on 7/8/0x0A
        /// alone. `Scrollbar` and `CheckboxOption` are `Button`s, so this
        /// one gate covers the arrows, the toggles and the radios together rather than
        /// special-casing any of them.
        ///
        /// The wheel still reaches the scrolling ancestor: the element's own mouse-down
        /// broadcasts `0x1C` *before* any of this runs, and the bubble up the parent chain is
        /// what [`crate::scrollable::Scrollable::wheel_target`] consumes.
        fn press_action_is_a_press(&self, ctx: &ElemCtx<'_>, action: u32) -> bool {
            let Some(n) = ctx.ui.node(ctx.me) else {
                return false;
            };
            let dbl = action == crate::focus::DOUBLE_CLICK_ACTIONS[0];
            if action != crate::focus::action::PRIMARY_CLICK
                && !(dbl && !n.flags.wants_dbl_clicks())
            {
                return false;
            }
            // Resizing then moving, in the client's order.
            !(n.flags.is_resizing() || n.flags.is_moving())
        }

        /// The button's mouse-up gate, which is **not** the same predicate as
        /// [`Self::press_action_is_a_press`] and is transcribed separately for that reason:
        ///
        /// ```text
        ///   if not pressed: nothing at all happens
        ///   action == 7    -> in
        ///   action == 0x0A -> in, else nothing happens
        /// ```
        ///
        /// Everything the mouse-up does — the `0x0E` toggle flip, clearing the pressed flag, the
        /// state update and the click itself — sits **inside** that block, so a release carrying
        /// a wheel detent leaves a button exactly as it found it. There is no wants-double-clicks
        /// test here and no moving/resizing test: both spellings of the left button are accepted
        /// outright, which is the note `crate::focus::UiSystem::mouse_up` already carries.
        fn release_action_is_a_click(&self, action: u32) -> bool {
            self.pressed
                && (action == crate::focus::action::PRIMARY_CLICK
                    || action == crate::focus::DOUBLE_CLICK_ACTIONS[0])
        }

        /// The button's click handler. It belongs to the button, not to any screen.
        ///
        /// It reads enum attribute `0x12`, refuses a missing value or action 1, then dispatches an
        /// input event with that action, toggle type 3, and its press edge set.
        ///
        /// so the two refusals are **no `0x12` at all** and **`0x12 == 1`**, the client's "do
        /// nothing" action. The three singleton null checks have no counterpart in a build where
        /// the manager is `self`. The manager's action handler's press-edge arm is the key-press
        /// event = [`crate::UiSystem::key_press`], whose tail is
        /// the visibility-toggle action — element message `0x31` on every element
        /// registered under attribute `0x57`, whose `0x58` then shows, hides or toggles it.
        ///
        /// **Re-entrancy:** this runs
        /// with the button's behaviour lifted out of its arena slot, so `key_press`'s
        /// `dispatch_action` on the focus element cannot take it back if the focus element *is*
        /// this button — which it usually is, since a press focuses the button. Nothing is lost: `Button` does not
        /// override `on_action`, the default answers `false`, and `dispatch_child_action` walks
        /// the **parents**, which are not lifted. The retail chain reaches
        /// the text element's action handler, whose own switch covers the text-editing actions
        /// `0x16`–`0x28` and would not consume a panel action either.
        ///
        /// Returns whether the action was fired, which is what decides `StopProcessing` above.
        pub fn handle_button_click(&self, ctx: &mut ElemCtx<'_>) -> bool {
            use crate::props::attr;
            let Some(n) = ctx.ui.node(ctx.me) else {
                return false;
            };
            let Some(action) = n.merged_properties().get_enum(attr::BUTTON_INPUT_ACTION) else {
                return false;
            };
            if action == 1 {
                return false;
            }
            let (x, y) = ctx.ui.mouse_pos();
            ctx.ui.key_press(&crate::focus::InputEvent {
                action,
                start: true,
                x,
                y,
            });
            true
        }
    }

    impl Element for Button {
        /// **`Button` is always mouse-visible, and that is what makes any button in this
        /// client clickable at all.**
        ///
        /// The original button, menu, drag handle, resize handle, and scrollbar all answer true to
        /// the mouse-visibility query. Nothing else on the shipped
        /// character-management layout is hit-testable: its six buttons carry no tooltip and no
        /// context-menu flag, so the base mouse-visibility query answers false for
        /// every one of them, and no shipped code registers any of the four mouse messages for
        /// their element ids either — the two other routes to mouse visibility.
        ///
        /// Without it initialisation step 5 leaves every button
        /// mouse-invisible, so recursive hit testing could never return one under its
        /// mouse-visible-or-block-clicks predicate.
        fn should_be_mouse_visible(&self) -> bool {
            true
        }

        /// The original button's inherited text/scrollable mouse-down behavior means a press on
        /// **any** button moves the focus element; see
        /// [`crate::element::Element::takes_focus_on_press`].
        ///
        /// **The retail ordering is the opposite of this build's, and the outcome is the same.**
        /// Retail runs the inherited text mouse-down — hence its focus-taking step — **first**, and
        /// only then marks the button pressed and updates its state. So retail's button *is* in
        /// state 1 or 2 when `0x2F` arrives, the default message handler's arm does push it, and
        /// the state update overwrites the result one statement later. This build raises the press
        /// state from the `0x1C` broadcast in step 5 of [`crate::UiSystem::mouse_down`], which is
        /// before step 7's focus take, so the button is already in state 3 and the arm declines
        /// it. Either way no button ends a press in state 4.
        fn takes_focus_on_press(&self) -> bool {
            true
        }

        /// Behavior: the base, then `update_state`.
        fn post_init(&mut self, ctx: &mut ElemCtx<'_>) {
            self.update_state(ctx);
        }

        /// The button's state setter — the whole of it, and it is **not** a
        /// pass-through:
        ///
        /// For a toggle button, state 6 sets attribute `0x0E` and state 1 clears it when that
        /// changes the current toggle. It then synchronizes disabled attribute `0x0D` with whether
        /// the requested state is `0x0D`; only a request that changes neither attribute reaches the
        /// base state setter.
        ///
        /// **The `0x0D` arm is how a button whose layout ships `Disabled = true` becomes usable.**
        /// The element's initialisation, step 3, sets the desc's default state, which
        /// for an ordinary button is state 1; the attribute still says disabled, the two disagree,
        /// and this override answers by writing the attribute **false** into the instance
        /// collection and refusing the state. Step 4 then applies the desc's own properties over
        /// the top and `update_state` lands on state 1.
        ///
        /// It is not hypothetical: the shipped `chargen_master` layout `0x21000038` carries
        /// `0x0D = true` on exactly three elements — `0x100003C9` *Help*, `0x100003CA` *Exit* and
        /// `0x100003CB` *Random* — and `CharGenScreen` never enables them. Without this override
        /// all three stay in state `0x0D` and the mouse-up's disabled gate refuses their
        /// click for ever. **In the retail client, clicking the wizard's *Exit* raises
        /// `ID_CharGen_ExitWarning` ("You are about to return to the Main Menu"), so retail's
        /// button is live.**
        ///
        /// This is the rule behind the otherwise surprising behavior that setting state 0 enables
        /// a button.
        ///
        /// **`update_state` has to be called here.** In the
        /// client the bool attribute setter ends in the overridable attribute handler, and
        /// the button's handler's `0x0E`/`0x0D` arms are
        /// the state update. In this crate the behaviour object is **lifted out of its
        /// arena slot** for the duration of this call, so [`crate::UiSystem::on_set_attribute`]'s
        /// `take_behaviour` answers `None` and the subclass half of the dispatch is skipped
        /// entirely — the attribute would change and the button never restate. The visible effect
        /// would be that the char-gen tab strip highlights **the previously selected tab**: every
        /// progress-state change writes `0x0E` on two toggle tabs and neither would restate until
        /// something else happened to call the state update. Calling it directly here is what the
        /// overridable dispatch would have done, and `update_state` queues through
        /// [`crate::UiSystem::queue_set_state`] so it still lands after the lift ends.
        fn set_state(&mut self, ctx: &mut ElemCtx<'_>, s: crate::StateId) -> bool {
            use crate::props::attr;
            let props = match ctx.ui.node(ctx.me) {
                Some(n) => n.merged_properties(),
                None => return false,
            };
            if props.get_bool(attr::TOGGLE_BUTTON).unwrap_or(false) {
                let toggled = props.get_bool(attr::TOGGLED).unwrap_or(false);
                if s == state::TOGGLED && !toggled {
                    ctx.ui.set_attribute_bool(ctx.me, attr::TOGGLED, true);
                    self.update_state(ctx);
                    return true;
                }
                if s == state::NORMAL && toggled {
                    ctx.ui.set_attribute_bool(ctx.me, attr::TOGGLED, false);
                    self.update_state(ctx);
                    return true;
                }
            }
            let disabled = props.get_bool(attr::DISABLED).unwrap_or(false);
            if (s == state::DISABLED) != disabled {
                ctx.ui
                    .set_attribute_bool(ctx.me, attr::DISABLED, s == state::DISABLED);
                self.update_state(ctx);
                ctx.ui.update_mouse_visibility(ctx.me);
                return true;
            }
            false
        }

        /// The button attribute handler chains to the text-element base first.
        fn on_set_attribute(
            &mut self,
            ctx: &mut ElemCtx<'_>,
            id: u32,
            v: Option<&crate::PropertyValue>,
        ) {
            use crate::props::attr;
            self.text.on_set_attribute(ctx, id, v);
            match id {
                // 0x0D also re-runs `update_mouse_visibility`.
                attr::DISABLED => {
                    self.update_state(ctx);
                    ctx.ui.update_mouse_visibility(ctx.me);
                }
                attr::TOGGLED | attr::ROLLOVER_HIGHLIGHT => self.update_state(ctx),
                // If hot-clicking is in progress and the new value is false: stop and unregister.
                attr::HOT_CLICK => {
                    self.hot_click_enabled = matches!(v, Some(crate::PropertyValue::Bool(true)));
                    if self.hot_clicking && !self.hot_click_enabled {
                        self.hot_clicking = false;
                        ctx.ui.want_tick(ctx.me, false);
                    }
                }
                _ => {}
            }
        }

        fn as_text_mut(&mut self) -> Option<&mut crate::text::TextElement> {
            Some(&mut self.text)
        }

        fn compose_text(&self, screen: crate::Box2D) -> Vec<crate::text::PlacedGlyph> {
            self.text.compose_text(screen)
        }

        /// Delegated for the same reason [`Self::compose_text`] is:
        /// the original button, menu, and scrollbar share text-control outline behavior. This
        /// rebuild delegates the same element outline state through composition.
        ///
        /// Without this the outline would be drawn for a plain label and silently not
        /// for a button carrying the same attribute. 16 shipped elements are exactly that case.
        fn text_outline_color(&self) -> Option<u32> {
            self.text.text_outline_color()
        }

        /// Delegated for the same reason the two above are: the selection
        /// belongs to the original shared text behavior, so its color-inversion draw arm applies
        /// to this element too.
        fn selection_boxes(&self, screen: crate::Box2D) -> Vec<crate::Box2D> {
            self.text.selection_boxes(screen)
        }

        /// Delegated with the three above: the draw's caret arm is
        /// `TextElement`'s, so it is this widget's too.
        fn caret(&self, screen: crate::Box2D) -> Option<(crate::Box2D, u32)> {
            self.text.caret(screen)
        }

        fn listen_to_element_message(&mut self, ctx: &mut ElemCtx<'_>, m: &ElementMessage) -> R {
            if m.source != ctx.me {
                return R::Default;
            }
            // The button's element-message handler. Messages from another
            // element and messages other than click 1 use the inherited text handler. A disabled
            // button swallows its own click; an enabled one stops only when its click action fires.
            //
            // Three arms, and **two of the three stop the message**:
            //
            // - disabled (attribute `0x0D`) — `handle_button_click` is **not** called and the
            //   message still stops. A disabled button swallows its own click; no ancestor sees
            //   it. That is the client's own early return of `2`.
            // - an action fired — stop.
            // - no `0x12`, or `0x12 == 1` — fall through to the base, i.e. bubble.
            //
            // **`StopProcessing` is the literal `2`**, pinned as a literal in this crate's button
            // dispatch tests. If the message went on bubbling after the action fired, an ancestor
            // could undo what the action did.
            if m.id == msgid::BUTTON_CLICKED {
                let disabled = ctx
                    .ui
                    .node(ctx.me)
                    .and_then(|n| n.merged_properties().get_bool(crate::props::attr::DISABLED))
                    .unwrap_or(false);
                if disabled {
                    return R::StopProcessing;
                }
                if self.handle_button_click(ctx) {
                    return R::StopProcessing;
                }
                return R::Default;
            }
            if m.id == msgid::MOUSE_PRESS {
                // **The action gate first** — see [`Self::press_action_is_a_press`]:
                // without it a wheel detent would press every button it passed over, and the matching
                // release would flip the `0x0E` of any that carried `0x0B`, which is every
                // `CheckboxOption` on every option page.
                if !self.press_action_is_a_press(ctx, m.p1) {
                    return R::Default;
                }
                // The button's mouse-down: the press itself raises **nothing**

                // unless attribute 0x0F (auto-repeat) is set, in which case it raises the *hot*
                // click straight away and arms the repeat.
                self.pressed = true;
                // The mouse-down sets the pressed flag and then calls the state update; the
                // state it lands in is 3 (or 8) while the pointer is still over the button and 2
                // (or 7, or the resting state) once it leaves. It is **not**
                // `element::state::ACTIVE` (5), which is the generic-element enum and is a
                // different table.
                self.update_state(ctx);
                if self.hot_click_enabled {
                    self.hot_clicking = true;
                    ctx.ui.want_tick(ctx.me, true);
                    // Mouse-down's final step adds float attribute
                    // `0x10` to the current time to schedule the first hot click.
                    //
                    // The **first** interval is 0x10 and it is a different attribute from the one
                    // every later repeat reads; see [`attr::HOT_CLICK_FIRST_INTERVAL`].
                    self.next_hot_click = ctx.ui.now.0
                        + self.interval(ctx, crate::props::attr::HOT_CLICK_FIRST_INTERVAL);
                    ctx.ui
                        .broadcast_element_message(ctx.me, msgid::BUTTON_HOT_CLICK, m.p1, 0);
                }
                return R::DontDoDefault;
            }
            if m.id == msgid::MOUSE_OVER_TOP {
                // The button's mouse-over-top setter stores the flag and updates state, and does
                // nothing else. It is a virtual called by the
                // mouse-over switch, which raises message 0x1B at the same moment; the
                // message is what this crate has, and running `update_state` off it is what makes
                // a rollover-highlight button light up under the pointer. Without it a button
                // stays in state 1 for ever and the whole rollover half of the layout is dead
                // artwork.
                self.update_state(ctx);
                return R::Default;
            }
            if m.id == msgid::MOUSE_RELEASE {
                // The release handler gates its **whole** body on `pressed &&
                // (action == 7 || action == 0x0A)`; see [`Self::release_action_is_a_click`].
                if !self.release_action_is_a_click(m.p1) {
                    return R::Default;
                }
                // 0x1D arrives before 0x19, so hot-clicking-in-progress -- which the client reads
                // *inside* its own mouse-up -- has to be carried across the two messages.
                self.suppress_click = self.hot_clicking;
                self.pressed = false;
                self.hot_clicking = false;
                ctx.ui.want_tick(ctx.me, false);
                // The release handler flips the toggle **before** updating state, which is what
                // makes a toggle button change picture on the click that toggles it.
                //
                // **The `!disabled` half matters.** The client's flip happens only while the
                // pointer is over the button and it is not disabled, so a disabled toggle button
                // does not change position under a click. Flipping `0x0E` anyway and only
                // refusing to raise the message would differ exactly where a `set_state(0x0D)`
                // button is clicked — the client's Enter Game and
                // Create are disabled that way — and the visible consequence would be a greyed
                // toggle that silently swaps its 1/6 half while it is unusable.
                //
                // A note on the same test: the click flag is computed *inside* it, so
                // the mouse-up's `(!disabled || click-while-disabled)` can only ever be evaluated
                // with `disabled == false`. Attribute `0x0C UICore_Button_clickWhileDisabled` is
                // read and **cannot change the answer** — a shipped branch that can never fire,
                // kept here as the client has it rather than tidied away.
                let disabled = ctx
                    .ui
                    .node(ctx.me)
                    .and_then(|n| n.merged_properties().get_bool(crate::props::attr::DISABLED))
                    .unwrap_or(false);
                if !disabled
                    && ctx
                        .ui
                        .node(ctx.me)
                        .and_then(|n| {
                            n.merged_properties()
                                .get_bool(crate::props::attr::TOGGLE_BUTTON)
                        })
                        .unwrap_or(false)
                    && ctx
                        .ui
                        .node(ctx.me)
                        .is_some_and(|n| n.region.flags.mouse_over_top)
                {
                    let now = ctx
                        .ui
                        .node(ctx.me)
                        .and_then(|n| n.merged_properties().get_bool(crate::props::attr::TOGGLED))
                        .unwrap_or(false);
                    ctx.ui
                        .set_attribute_bool(ctx.me, crate::props::attr::TOGGLED, !now);
                }
                self.update_state(ctx);
                return R::DontDoDefault;
            }
            if m.id == msgid::MOUSE_CLICK {
                // The same gate one message later. The click flag is computed **inside** the
                // mouse-up's `action == 7 || action == 0x0A` block, so a wheel detent never raises
                // message 1 however the manager spells the release. This build splits the
                // mouse-up's tail into `0x19`, so the test has to be repeated here; `self.pressed`
                // has already been cleared by the `0x1D` arm above, which is why only the action is
                // asked for.
                if m.p1 != crate::focus::action::PRIMARY_CLICK
                    && m.p1 != crate::focus::DOUBLE_CLICK_ACTIONS[0]
                {
                    return R::Default;
                }
                // The button's mouse-up:
                //
                // ```text
                // disabled = bool attribute 0x0D
                // click = false
                // if pressed and (action == 7 or action == 10):
                //     if pointer over the button and not disabled:
                //         ...0x0B/0x0E toggle...
                //         click = not hot_clicking
                //     pressed = false; ...
                //     if click: broadcast message 1 with (7, 0)
                // ```
                //
                // **The ordinary click is message 1 with `p1 = 7`, not message 2.** Message 2 is
                // the auto-repeat "hot click", raised on mouse-down when attribute
                // 0x0F is set and then by the message-3 tick. Every screen's element-message
                // listener in the client tests for message `1` from a button, including all six
                // buttons on the character-generation screen, so a widget
                // that broadcasts 2 leaves every screen deaf.
                //
                // `msgid::MOUSE_CLICK` is raised only when the
                // press was on this same element, which is the pressed flag; the disabled
                // gate is the client's own and is why the button refresh's `set_state(0x0D)` is
                // enough to make a button inert.
                let was_hot = std::mem::take(&mut self.suppress_click);
                let disabled = ctx
                    .ui
                    .node(ctx.me)
                    .and_then(|n| n.merged_properties().get_bool(crate::props::attr::DISABLED))
                    .unwrap_or(false);
                if !disabled && !was_hot {
                    // **Queued, not broadcast.** The client raises this *after* the
                    // base has raised `0x1D` and `0x19`; raising it here, inside `0x19`'s own
                    // dispatch, would make the nested serial number swallow the rest of `0x19`'s
                    // bubble and no ancestor of any button would ever see it. See
                    // [`crate::UiSystem::queue_element_message`].
                    ctx.ui
                        .queue_element_message(ctx.me, msgid::BUTTON_CLICKED, 7, 0);
                }
                return R::DontDoDefault;
            }
            R::Default
        }

        /// The button's global-message listener — the auto-repeat clock.
        ///
        /// **It is not "broadcast 2 while hot-clicking", i.e. one repeat per
        /// frame.** That would scroll a held arrow at the frame rate: two rows in two frames
        /// where retail moves one, and worse the faster the client runs. Every list in the
        /// client is affected, because every scrollbar's arrows are `Button`s and
        /// the scrollbar's default-hot-click setup gives all of them the same two attributes.
        ///
        /// When the pointer is off the button, a scheduled time strictly before the current time is
        /// parked at the current time. When the pointer is on it, a scheduled time at or before the
        /// current time broadcasts hot-click message 2 and advances by float attribute `0x11`.
        ///
        /// Three things a quick reading of the comparisons would not have settled:
        ///
        /// * **The repeat boundary is inclusive.** The equal case *fires*:
        ///   `next_hot_click <= now`.
        /// * **The park boundary is exclusive.** Equality does not store: `next_hot_click < now`.
        /// * **The advance accumulates from the previous scheduled time**, not from `now`:
        ///   the addition starts from the scheduled time itself. That is what makes the cadence a
        ///   function of elapsed time rather than of when a frame happened to land.
        ///
        /// The first arm is the one that is easy to leave out and it is not cosmetic: while the
        /// pointer is held down but dragged off the arrow, the clock is dragged forward with it,
        /// so re-entering the arrow after two seconds fires **once**, not sixteen times.
        fn listen_to_global_message(&mut self, ctx: &mut ElemCtx<'_>, id: MessageId, _p: u32) {
            if id != crate::msg::global::TICK || !self.hot_clicking {
                return;
            }
            let now = ctx.ui.now.0;
            let over_top = ctx
                .ui
                .node(ctx.me)
                .is_some_and(|n| n.region.flags.mouse_over_top);
            if !over_top {
                // Strictly past only, so an exactly-due repeat is not re-stamped.
                if self.next_hot_click < now {
                    self.next_hot_click = now;
                }
                return;
            }
            // Due *or* exactly due.
            if self.next_hot_click > now {
                return;
            }
            ctx.ui
                .broadcast_element_message(ctx.me, msgid::BUTTON_HOT_CLICK, 0, 0);
            self.next_hot_click +=
                self.interval(ctx, crate::props::attr::HOT_CLICK_REPEAT_INTERVAL);
        }
    }
}

/// The drag handle (2) — a title-bar grab handle. On mouse-down it calls `start_movement` on its
/// **parent**, which is how AC's windows move.
pub mod dragbar {
    use super::*;

    #[derive(Debug, Default)]
    pub struct Dragbar {
        /// The mouse was pressed on this handle — the one byte the constructor zeroes.
        pub mouse_pressed: bool,
    }

    pub fn create(_l: &crate::LayoutDesc, _d: &crate::ElementDesc) -> Box<dyn Element> {
        Box::new(Dragbar::default())
    }

    impl Element for Dragbar {
        /// The original mouse-visibility query always returns true for the five mouse-driven
        /// element types: button, menu, dragbar, resizebar, and scrollbar.
        fn should_be_mouse_visible(&self) -> bool {
            true
        }

        /// The dragbar's element-message handler, and the three arms are the
        /// whole of window dragging in this client:
        ///
        /// ```text
        /// only for the handle's own messages, and only with a parent:
        /// 0x1C (press), p1 == 7, parent neither resizing nor moving:
        ///     start moving (the parent's start_movement at the press point)
        ///     the parent's mouse-down at the window point
        /// 0x1D (release), p1 == 7, pressed on the handle:
        ///     the parent's mouse-up at the window point with action 7
        ///     stop moving (the parent's stop_movement)
        ///     return StopProcessing
        /// 0x1E (move), parent moving, pressed on the handle:
        ///     the parent's mouse_move_element at the window point
        ///     return StopProcessing
        /// ```
        ///
        /// **The `0x1E` arm, the pressed flag and the action-7 gate all matter.** Without
        /// the `0x1E` arm a dragbar has no motion handler at all, so the moving flag is set and
        /// cleared and the window never moves (the radar's drag button is the visible case).
        /// The pointer keeps hearing `0x1E` after it leaves the 26×26 handle because
        /// `mouse_down` takes the mouse **capture**, and `mouse_move` sends `0x1E` to the capture
        /// holder in preference to whatever is under the pointer.
        ///
        /// `p1 == 7` is `action::PRIMARY_CLICK`: the right button does not drag windows.
        fn listen_to_element_message(&mut self, ctx: &mut ElemCtx<'_>, m: &ElementMessage) -> R {
            if m.source != ctx.me {
                return R::Default;
            }
            let Some(p) = ctx.ui.parent(ctx.me) else {
                return R::Default;
            };
            let (x, y) = m.point.window;
            if m.id == msgid::MOUSE_PRESS {
                if m.p1 != crate::focus::action::PRIMARY_CLICK {
                    return R::Default;
                }
                self.mouse_pressed = true;
                ctx.ui.start_movement(p, x, y);
                return R::DontDoDefault;
            }
            if m.id == msgid::MOUSE_RELEASE {
                if m.p1 != crate::focus::action::PRIMARY_CLICK || !self.mouse_pressed {
                    return R::Default;
                }
                self.mouse_pressed = false;
                ctx.ui.stop_movement(p);
                return R::StopProcessing;
            }
            if m.id == msgid::MOUSE_MOVE
                && self.mouse_pressed
                && ctx.ui.node(p).is_some_and(|n| n.flags.is_moving())
            {
                ctx.ui.mouse_move_element(p, x, y);
                return R::StopProcessing;
            }
            R::Default
        }
    }
}

/// The resize handle (9) — a corner or edge grab handle. Calls `start_resizing` on its parent
/// with the matching [`BorderLocation`](crate::BorderLocation), and element message **0x20** is
/// broadcast while resizing.
///
/// Resizing needs all five pieces: this widget's motion arm, the drag origin
/// `UiSystem::start_resizing` records, `layout::mouse_resize`, the reader of
/// `ElementNode::current_border`, and `UiSystem::mouse_resize_element`. Without any one of them a
/// press on a resize handle sets a flag, a release clears it, and nothing in between changes a
/// single pixel.
///
/// **The border comes from four booleans on the bar, not from a hit test.** There is no zone
/// classification anywhere in the client: the resize start
/// reads four bool attributes off *itself* and folds them into one `BorderLocation`. See
/// `attr` and `border_from_edges`.
pub mod resizebar {
    use super::*;

    /// The four edge flags the resizebar reads, in property group `0x10`, as its
    /// available-properties query declares them.
    ///
    /// **The ids are named by the decision tree they feed, not by the layout tool** — its names
    /// for them are not known. The resize start's fold is unambiguous about which is
    /// which; see `border_from_edges` for the mapping and the two arithmetic identities that
    /// pin it.
    pub mod attr {
        /// `0x2A` — this bar drags the **bottom** edge.
        pub const BOTTOM: u32 = 0x2A;
        /// `0x2B` — the **left** edge.
        pub const LEFT: u32 = 0x2B;
        /// `0x2C` — the **right** edge.
        pub const RIGHT: u32 = 0x2C;
        /// `0x2D` — the **top** edge.
        pub const TOP: u32 = 0x2D;
    }

    /// The client's fold, as nested tests:
    ///
    /// ```text
    /// if (!right) {
    ///     if (!left)  { if (!top) { if (!bottom) return;  else BOTTOM } else TOP }
    ///     else        { if (!top) BORDER_LEFT - (bottom != 0)  else UPPER_LEFT }
    /// } else {
    ///     if (!top)   BORDER_RIGHT + (bottom != 0)
    ///     else        UPPER_RIGHT
    /// }
    /// ```
    ///
    /// The two arithmetic forms are what fix the enum's order and therefore the attribute names:
    /// `BORDER_LEFT - 1` must be `BORDER_LL` and `BORDER_RIGHT + 1` must be `BORDER_LR`, which is
    /// exactly the [`BorderLocation`](crate::BorderLocation) order this crate already carries.
    /// Two independent readings agreeing is what makes the
    /// `0x2A`..`0x2D` assignment a measurement rather than a guess.
    ///
    /// A bar with **no** edge flag at all returns [`BorderLocation::None`](crate::BorderLocation),
    /// which is the client's bare `return;` — it never even latches the drag origin.
    #[must_use]
    pub const fn border_from_edges(
        left: bool,
        top: bool,
        right: bool,
        bottom: bool,
    ) -> crate::BorderLocation {
        use crate::BorderLocation as B;
        match (right, left, top, bottom) {
            (false, false, false, false) => B::None,
            (false, false, false, true) => B::Bottom,
            (false, false, true, _) => B::Top,
            (false, true, false, false) => B::Left,
            (false, true, false, true) => B::LowerLeft,
            (false, true, true, _) => B::UpperLeft,
            (true, _, false, false) => B::Right,
            (true, _, false, true) => B::LowerRight,
            (true, _, true, _) => B::UpperRight,
        }
    }

    #[derive(Debug, Default)]
    pub struct Resizebar {
        /// The mouse was pressed on this handle — the constructor zeroes it. Without it the
        /// motion arm cannot tell a drag from a pointer that merely crossed the handle.
        pub mouse_pressed: bool,
        /// The zone the last press resolved to, kept only so a test and a debugger can see it.
        /// The client stores this on the **parent** (its current border) and re-reads the four
        /// attributes on every press; so does this.
        pub border: crate::BorderLocation,
    }

    pub fn create(_l: &crate::LayoutDesc, _d: &crate::ElementDesc) -> Box<dyn Element> {
        Box::new(Resizebar::default())
    }

    impl Element for Resizebar {
        /// The original mouse-visibility query always returns true for the five mouse-driven
        /// element types: button, menu, dragbar, resizebar, and scrollbar.
        fn should_be_mouse_visible(&self) -> bool {
            true
        }

        /// The resizebar's element-message handler, whose three arms all act on
        /// the **parent**:
        ///
        /// ```text
        /// only for the handle's own messages, and only with a parent:
        /// 0x1C (press), p1 == 7, parent neither resizing nor moving:
        ///     start the mouse resize (the parent's start_resizing at the window point)
        ///     the parent's mouse-down at the window point
        /// 0x1D (release), p1 == 7, pressed on the handle:
        ///     the parent's mouse-up at the window point with action 7
        ///     stop the mouse resize (the parent's stop_resizing)
        ///     return StopProcessing
        /// 0x1E (move), parent resizing, pressed on the handle:
        ///     the parent's mouse_resize_element at the window point
        ///     return StopProcessing
        /// ```
        ///
        /// **The `0x1E` arm is essential**, as in the dragbar: without it the flag is set and
        /// cleared and no pointer motion ever reaches the
        /// parent. Everything else here follows the dragbar's shape, including why the handle keeps
        /// hearing `0x1E` after the pointer leaves its own box (the press takes the mouse capture,
        /// and `mouse_move` prefers the capture holder to whatever is under the pointer).
        ///
        /// **Two clauses are deliberately not transcribed, for the same reason as in the
        /// dragbar**: the parent's mouse-down and mouse-up calls are the element
        /// base's own press and release, i.e. a `0x1C`/`0x1D` broadcast *from the parent*, and
        /// raising one from inside this handler would re-enter a dispatch whose own slot is empty.
        /// Named here rather than silently dropped; the dragbar has the identical gap.
        ///
        /// `p1 == 7` is `action::PRIMARY_CLICK`, so the right button does not resize windows
        /// any more than it drags them.
        fn listen_to_element_message(&mut self, ctx: &mut ElemCtx<'_>, m: &ElementMessage) -> R {
            if m.source != ctx.me {
                return R::Default;
            }
            let Some(p) = ctx.ui.parent(ctx.me) else {
                return R::Default;
            };
            let (x, y) = m.point.window;
            if m.id == msgid::MOUSE_PRESS {
                if m.p1 != crate::focus::action::PRIMARY_CLICK {
                    return R::Default;
                }
                // The parent must be neither resizing nor moving. `start_resizing` repeats the
                // same guard (it is that function's own), but the client tests it here too and
                // the pressed flag must not be set when it fails.
                let busy = ctx
                    .ui
                    .node(p)
                    .is_some_and(|n| n.flags.is_resizing() || n.flags.is_moving());
                if busy {
                    return R::Default;
                }
                // The resize start: the four attributes, then the fold.
                let Some(props) = ctx.ui.node(ctx.me).map(|n| n.merged_properties()) else {
                    return R::Default;
                };
                let b = |id: u32| props.get_bool(id).unwrap_or(false);
                self.border =
                    border_from_edges(b(attr::LEFT), b(attr::TOP), b(attr::RIGHT), b(attr::BOTTOM));
                if self.border == crate::BorderLocation::None {
                    // The client's bare `return;`: a bar declaring no edge is not a handle.
                    return R::Default;
                }
                self.mouse_pressed = true;
                ctx.ui.start_resizing(p, self.border, x, y);
                ctx.ui
                    .broadcast_element_message(p, msgid::BEING_RESIZED, 0, 0);
                return R::DontDoDefault;
            }
            if m.id == msgid::MOUSE_RELEASE {
                if m.p1 != crate::focus::action::PRIMARY_CLICK || !self.mouse_pressed {
                    return R::Default;
                }
                self.mouse_pressed = false;
                ctx.ui.stop_resizing(p);
                return R::StopProcessing;
            }
            if m.id == msgid::MOUSE_MOVE
                && self.mouse_pressed
                && ctx.ui.node(p).is_some_and(|n| n.flags.is_resizing())
            {
                ctx.ui.mouse_resize_element(p, x, y);
                return R::StopProcessing;
            }
            R::Default
        }
    }
}

/// `Panel` (8) — a tabbed container. A tab-to-page table (tab element id → page element id)
/// and its page-to-tab inverse, with the open page and open tab. Broadcasts **0x2C** on page
/// change.
pub mod panel {
    use super::*;
    use crate::ElementId;
    use std::collections::BTreeMap;

    /// The property ids `setup_tab_page_hash` walks, with the retail `MasterProperty` names.
    ///
    /// `0x2E` is the trigger: the panel's attribute handler is one line, which runs
    /// `setup_tab_page_hash` when the property is `0x2E`, and initialisation step 4
    /// re-dispatches every property in the description, so a panel builds its tab table the
    /// moment its tree comes up. [verified against `client_local_English.dat`: the six
    /// tabbed pages of `classic_gameplay` each carry `0x2E`, and every entry resolves to a tab and
    /// a page that are both children of the panel]
    pub mod attr {
        /// `UICore_Panel_pages` -- an `Array` of [`PAGE_DATA`] structs.
        pub const PAGES: u32 = 0x2E;
        /// `UICore_Panel_page_data` -- one `Struct`.
        pub const PAGE_DATA: u32 = 0x2F;
        /// `UICore_Panel_tab_element` -- the tab's element id.
        pub const TAB_ELEMENT: u32 = 0x30;
        /// `UICore_Panel_page_element` -- the page's element id.
        pub const PAGE_ELEMENT: u32 = 0x31;
        /// `UICore_Panel_page_open` -- set on the entry the panel opens with. **Absent means
        /// false**: three of `0x1000018F`'s four entries omit the member entirely.
        pub const PAGE_OPEN: u32 = 0x32;
    }

    /// The two states a tab element carries. Every one of the shipped tabs is a `TextElement`
    /// declaring exactly `0x0B` and `0x0C`, which differ only in the font colour array (0x1B):
    /// `0x0B` is `0xFF8080FF` and `0x0C` is `0xFFCDCDCC`.
    ///
    /// The panel update puts every tab into one of the two, but which one is not recoverable
    /// statically. The pairing here is the one a windowed retail run shows:
    /// with the character panel open and the Skills tab clicked, the Skills label is the pale one
    /// and Attributes and Titles are the blue ones.
    pub mod tab_state {
        use crate::StateId;
        /// The tab of a page that is **not** open.
        pub const CLOSED: StateId = StateId(0x0B);
        /// The tab of the open page.
        pub const OPEN: StateId = StateId(0x0C);
    }

    #[derive(Debug, Default)]
    pub struct Panel {
        /// Tab element id → page element id.
        pub tab_to_page: BTreeMap<ElementId, ElementId>,
        /// Page element id → tab element id.
        pub page_to_tab: BTreeMap<ElementId, ElementId>,
        /// The open tab.
        pub open_tab: Option<ElementId>,
        /// The open page.
        pub open_page: Option<ElementId>,
    }

    pub fn create(_l: &crate::LayoutDesc, _d: &crate::ElementDesc) -> Box<dyn Element> {
        Box::new(Panel::default())
    }

    impl Panel {
        pub fn add_tab(&mut self, tab: ElementId, page: ElementId) {
            self.tab_to_page.insert(tab, page);
            self.page_to_tab.insert(page, tab);
        }

        /// Rebuild both tab/page hashes from the `0x2E`
        /// array, then `update(default_page, default_tab)`.
        ///
        /// The client clears both tables first and resets the two tokens to 0, which is what makes
        /// the closing `update` run at all (see [`Self::update`]'s guard).
        pub fn setup_tab_page_hash(
            &mut self,
            ctx: &mut ElemCtx<'_>,
            v: Option<&crate::PropertyValue>,
        ) {
            self.tab_to_page.clear();
            self.page_to_tab.clear();
            self.open_tab = None;
            self.open_page = None;
            let Some(crate::PropertyValue::Array(rows)) = v else {
                return;
            };
            let mut default: Option<(ElementId, ElementId)> = None;
            for row in rows {
                if row.id != attr::PAGE_DATA {
                    continue;
                }
                let crate::PropertyValue::Struct(members) = &row.value else {
                    continue;
                };
                let member = |id: u32| {
                    members
                        .iter()
                        .find(|(k, _)| *k == id)
                        .map(|(_, p)| &p.value)
                };
                let (Some(crate::PropertyValue::Enum(tab)), Some(crate::PropertyValue::Enum(page))) =
                    (member(attr::TAB_ELEMENT), member(attr::PAGE_ELEMENT))
                else {
                    continue;
                };
                let (tab, page) = (ElementId(*tab), ElementId(*page));
                // Each hash refuses a duplicate key, and the client tests both returns
                // before it does anything else with the row.
                if self.tab_to_page.contains_key(&tab) || self.page_to_tab.contains_key(&page) {
                    continue;
                }
                self.tab_to_page.insert(tab, page);
                self.page_to_tab.insert(page, tab);
                if matches!(
                    member(attr::PAGE_OPEN),
                    Some(crate::PropertyValue::Bool(true))
                ) {
                    default = Some((tab, page));
                }
            }
            // **The tabs have to be hit-testable, and nothing else in the data makes them so.**
            // Every shipped tab is a `TextElement`, and neither route to mouse visibility
            // reaches one: text mouse visibility is true only for editable or selectable text,
            // while the base rule requires either the context-menu bit or valid tooltip text.
            // Both rules answer false for a plain label with no tooltip,
            // which is what all six panels' tabs are. Context-menu lookup can therefore never
            // return one and no click can reach `open_tab`.
            //
            // The tab-page setup's own tail is the only candidate: for each entry it finds the
            // tab recursively and makes one call on it with the argument `1`. Reading that call
            // as "make the tab mouse-visible" is what makes the panel
            // behave the way the retail client does. `// UNVERIFIED:` the argument's meaning.
            for tab in self.tab_to_page.keys().copied().collect::<Vec<_>>() {
                let me = ctx.me;
                if let Some(h) = ctx.ui.get_child_recursive(me, tab) {
                    ctx.ui.set_mouse_visible(h, true);
                }
            }
            // The client seeds the pair from the entry carrying `UICore_Panel_page_open`; with no
            // such entry both tokens stay 0 and `update` hides every page.
            let (tab, page) = default.unwrap_or((ElementId(0), ElementId(0)));
            self.update(ctx, page, tab);
        }

        /// The panel's update.
        ///
        /// **Retail's guard is `&&`, not `||`**: it runs only if `page != open_page && tab !=
        /// open_tab`, so in retail a call naming the page that is already open (or the tab that is
        /// already open) does nothing, whatever the other token is.
        ///
        /// **This build does not reproduce that**: the code below returns only when *both* tokens
        /// match, so a call that changes just one of them still runs here.
        pub fn update(&mut self, ctx: &mut ElemCtx<'_>, page: ElementId, tab: ElementId) {
            let same_page = self.open_page.is_some_and(|p| p == page);
            let same_tab = self.open_tab.is_some_and(|t| t == tab);
            if same_page && same_tab {
                return;
            }
            self.open_tab = Some(tab);
            self.open_page = Some(page);
            let me = ctx.me;
            for (t, p) in self.tab_to_page.clone() {
                if let Some(h) = ctx.ui.get_child_recursive(me, p) {
                    ctx.ui.set_visible(h, p == page);
                }
                if let Some(h) = ctx.ui.get_child_recursive(me, t) {
                    let s = if t == tab {
                        tab_state::OPEN
                    } else {
                        tab_state::CLOSED
                    };
                    ctx.ui.set_state(h, s);
                }
            }
            ctx.ui
                .broadcast_element_message(me, msgid::TAB_PAGE_CHANGED, 0, 0);
        }

        /// Resolve tab to page to tab, refusing unless the round
        /// trip lands back on the tab it started from.
        pub fn open_tab(&mut self, ctx: &mut ElemCtx<'_>, tab: ElementId) -> bool {
            let Some(page) = self.tab_to_page.get(&tab).copied() else {
                return false;
            };
            if self.page_to_tab.get(&page).copied() != Some(tab) {
                return false;
            }
            self.update(ctx, page, tab);
            true
        }
    }

    impl Element for Panel {
        /// `(0x08)` — a caller that wants the tab table.
        fn as_any(&self) -> Option<&dyn std::any::Any> {
            Some(self)
        }

        fn on_set_attribute(
            &mut self,
            ctx: &mut ElemCtx<'_>,
            id: u32,
            v: Option<&crate::PropertyValue>,
        ) {
            if id == attr::PAGES {
                self.setup_tab_page_hash(ctx, v);
            }
        }

        fn listen_to_element_message(&mut self, ctx: &mut ElemCtx<'_>, m: &ElementMessage) -> R {
            let me = ctx.me;
            let from_self = ctx
                .ui
                .node(me)
                .is_some_and(|n| n.element_id() == m.source_id);
            if from_self {
                // The panel's own visibility changed. Shown: put the open page back up. Hidden:
                // take every page down, so a re-show cannot reveal two at once.
                if m.id != msgid::VISIBILITY_CHANGED {
                    return R::Default;
                }
                // **Queued, not applied inline.** Setting an element's visibility raises `0x18`,
                // so changing a child's
                // visibility from inside a `0x18` handler nests a broadcast, which takes the next
                // serial and stamps every ancestor; the message still bubbling is then refused at
                // each one by `claim_serial` and dies below whatever was listening.
                //
                // Applied inline, the **six** of
                // `PanelStack`'s sixteen pages that are element type `0x00000008` (this type) would
                // never re-broadcast their own visibility, so `PanelStack::on_page_visibility_changed`
                // would never hear about them and the panel stack would never learn which page was
                // current. Six of the seven toolbar panel buttons open one of those six.
                //
                // `queue_set_visible` runs it once the outermost broadcast has unwound, so the
                // panel's own `0x18` completes its bubble first and the page's follows. See
                // [`crate::UiSystem::queue_set_visible`].
                if m.p1 == 0 {
                    for p in self.page_to_tab.keys().copied().collect::<Vec<_>>() {
                        if let Some(h) = ctx.ui.get_child_recursive(me, p) {
                            ctx.ui.queue_set_visible(h, false);
                        }
                    }
                } else if let Some(open) = self.open_page {
                    if let Some(h) = ctx.ui.get_child_recursive(me, open) {
                        ctx.ui.queue_set_visible(h, true);
                    }
                }
                return R::Default;
            }
            // The panel opens a tab on **0x19** (mouse
            // click) or **0x29** (activated), never on a button message (compare
            // [`msgid::BUTTON_CLICKED`]).
            if m.id == msgid::MOUSE_CLICK || m.id == msgid::ACTIVATED {
                if self.open_tab(ctx, m.source_id) {
                    return R::StopProcessing;
                }
                return R::Default;
            }
            // A direct registered page controls the tabbed container's visibility too. The
            // retail listener checks that the page's parent is this panel, reads the child's
            // current local-visible bit, then sets this panel visible iff its
            // selected page is locally visible. Without that tail F8/F9 (and F3-F6) only show
            // a page inside a hidden parent. Read live flags, not a potentially queued p1.
            if m.id == msgid::VISIBILITY_CHANGED
                && ctx
                    .ui
                    .node(m.source)
                    .is_some_and(|n| n.region.parent == Some(me))
            {
                if let Some(tab) = self.page_to_tab.get(&m.source_id).copied() {
                    if ctx
                        .ui
                        .node(m.source)
                        .is_some_and(|n| n.region.flags.visible)
                    {
                        self.open_tab(ctx, tab);
                    }
                    let visible = self
                        .open_page
                        .and_then(|page| ctx.ui.get_child_recursive(me, page))
                        .and_then(|h| ctx.ui.node(h))
                        .is_some_and(|n| n.region.flags.visible);
                    // Match the existing self-visibility arm's deferred writes: finish this
                    // child's serial before broadcasting the parent's own visibility notice.
                    ctx.ui.queue_set_visible(me, visible);
                }
            }
            R::Default
        }
    }
}

/// `GroupBox` (0x11) — a radio-button group driven by its selected-child attribute.
pub mod groupbox {
    use super::*;
    use crate::ElementId;

    /// Initialization copies this nonzero element id to [`SELECTED_BUTTON`].
    pub const DEFAULT_BUTTON: u32 = 0xB0;
    /// Selection consumes this element id, not a media-state number.
    pub const SELECTED_BUTTON: u32 = 0xB1;
    /// Allow-reselect: whether a click on the selected child propagates.
    pub const ALLOW_RESELECT: u32 = 0xC1;

    #[derive(Debug, Default)]
    pub struct GroupBox {
        /// The selected button's element id.
        pub selected: Option<ElementId>,
    }

    pub fn create(_l: &crate::LayoutDesc, _d: &crate::ElementDesc) -> Box<dyn Element> {
        Box::new(GroupBox::default())
    }

    impl GroupBox {
        /// The group box's attribute setter: state 1 on the old child, state 6 on
        /// the new child. The button's state setter also updates its toggle attribute; generic
        /// ACTIVE (5) is not the button's toggled state and does not select it.
        fn apply_selection(&mut self, ctx: &mut ElemCtx<'_>, id: u32) {
            if let Some(old) = self
                .selected
                .and_then(|id| ctx.ui.get_child_recursive(ctx.me, id))
            {
                ctx.ui.set_state(old, button::state::NORMAL);
            }
            self.selected = (id != 0).then_some(ElementId(id));
            if let Some(new) = ctx.ui.get_child_recursive(ctx.me, ElementId(id)) {
                ctx.ui.set_state(new, button::state::TOGGLED);
            }
        }

        fn set_selection(&mut self, ctx: &mut ElemCtx<'_>, id: u32) {
            ctx.ui.set_attribute_enum(ctx.me, SELECTED_BUTTON, id);
            // This behaviour is lifted out during post_init/listen, so the setter cannot call
            // our on_set_attribute. Execute that same consumer here, as the button's set_state does
            // for its own nested attribute writes; external writes use on_set_attribute below.
            self.apply_selection(ctx, id);
        }
    }

    impl Element for GroupBox {
        fn post_init(&mut self, ctx: &mut ElemCtx<'_>) {
            let id = ctx
                .ui
                .node(ctx.me)
                .and_then(|n| n.merged_properties().get_enum(DEFAULT_BUTTON))
                .unwrap_or(0);
            // An absent attribute leaves retail's out-parameter undefined; zero is the safe
            // fallback for synthetic/incomplete layouts. The shipped combat group supplies Medium.
            if id != 0 {
                self.set_selection(ctx, id);
            }
        }

        fn on_set_attribute(
            &mut self,
            ctx: &mut ElemCtx<'_>,
            id: u32,
            v: Option<&crate::PropertyValue>,
        ) {
            if id == SELECTED_BUTTON {
                // The setter skips the value read when BaseProperty has no value, retaining the
                // cached id; it still deselects/reselects that child on either side of the read.
                let selected = match v {
                    Some(crate::PropertyValue::Enum(selected)) => *selected,
                    _ => self.selected.map_or(0, |id| id.0),
                };
                self.apply_selection(ctx, selected);
            }
        }

        fn listen_to_element_message(&mut self, ctx: &mut ElemCtx<'_>, m: &ElementMessage) -> R {
            if m.id != msgid::BUTTON_CLICKED {
                return R::Default;
            }
            let selected = ctx
                .ui
                .node(ctx.me)
                .and_then(|n| n.merged_properties().get_enum(SELECTED_BUTTON))
                .unwrap_or(0);
            if selected == m.source_id.0 {
                // The mouse-up toggles the selected button off before sending message 1. Retail
                // restores state 6 even when the group's 0xC1 gate then swallows the message.
                ctx.ui.set_state(m.source, button::state::TOGGLED);
                if !ctx
                    .ui
                    .node(ctx.me)
                    .and_then(|n| n.merged_properties().get_bool(ALLOW_RESELECT))
                    .unwrap_or(false)
                {
                    return R::StopProcessing;
                }
            } else {
                self.set_selection(ctx, m.source_id.0);
            }
            R::Default
        }
    }
}

/// `Viewport` (0x0D) — the 3D preview: owns a viewport render object and carries a
/// `CreatureMode` for posing a rendered creature (the paper doll, char-gen and the barber).
///
/// The rendering is the renderer's and the creature posing is the animation system's; this side owns only the
/// element and the fact that it always owns its own render object.
pub mod viewport {
    use super::*;

    #[derive(Debug, Default)]
    pub struct Viewport {
        /// `CreatureMode`, opaque here.
        pub creature_mode: u32,
    }

    pub fn create(_l: &crate::LayoutDesc, _d: &crate::ElementDesc) -> Box<dyn Element> {
        Box::new(Viewport::default())
    }

    impl Element for Viewport {}
}

/// `ListBox` (5) — a multi-column list of child elements with composed scrolling behavior.
/// Broadcasts **4** on selection change (`p1 = index`, `p2 = item`) and
/// **0x43** on row activation.
pub mod listbox {
    use super::*;
    use crate::ElementId;

    #[derive(Debug, Default)]
    pub struct ListBox {
        /// The original smart array of row **elements**.
        ///
        /// **Handles, not `ElementId`s.** An element id cannot stand in for a
        /// row here: every row of a menu popup is created from the *same* template description
        /// (`0x1000001E` for the chat window's talk-focus menu, all fourteen of them), so ids are
        /// not distinct, and the client's item-equals-selected-item
        /// pointer compare has no id-shaped equivalent.
        pub items: Vec<ElemHandle>,
        /// ItemList's active rows exclude its hidden slot cache. Other legacy binders still
        /// discover their rows from children until they publish the item list explicitly.
        pub items_are_authoritative: bool,
        /// The selected item.
        pub selected: Option<usize>,
        pub cols: u32,
        pub rows: u32,
        /// The element a drag was last over.
        pub drag_last_over: Option<ElementId>,
        /// The scroll animation: start time, end time, end x and end y.
        pub anim: Option<(f64, f64, i32, i32)>,
        /// `Scrollable`'s virtual content rectangle.
        pub scroll_offset: (i32, i32),
        /// The element is in state `0x0D` — the one state in which the list box's mouse-visibility
        /// override can still answer false. Tracked here because the
        /// override is asked before `UiSystem` has a state to consult.
        pub disabled: bool,

        // ---- shared scrolling state ------------------------------------
        /// The six pieces of scrolling state inherited by the original list box and composed here.
        ///
        /// The original list box shares scrolling behavior, established by its scroll-to-Y,
        /// scroll-to-X, adjust-to-scrollable-change, and scroll-delta operations. Its
        /// element-message handler delegates to that shared behavior last.
        ///
        /// Without it the wizard's skills list — 42 rows of 26 px in a 309 px box — would have a
        /// bar that reports the gesture and a list that never consumes it.
        pub scroll: crate::scrollable::Scrollable,
        /// Each row's **unscrolled** origin, re-captured whenever the row set changes.
        ///
        /// The client does not need this: its layout update re-places every row at
        /// `position - scroll offset` each time the dirty bit `0x200` is set, so the origin is
        /// implicit in the arithmetic. In this build the placement is the screen-side
        /// `ListBoxWidget::update_layout`, which places rows unscrolled; recording the origins is
        /// how the same picture is reached without owning that call.
        origins: Vec<(ElemHandle, i32, i32)>,
        /// `(handle, width, height)` per row when [`Self::origins`] was captured.
        fingerprint: Vec<(ElemHandle, i32, i32)>,
        /// `(handle, x0, y0, width, height)` for every row **exactly as [`Self::place_rows`] last
        /// left it** — the cache's invalidation key.
        ///
        /// [`Self::fingerprint`] alone, i.e. the handles and their sizes,
        /// cannot see the one event this cache exists to react to. The stat-management panel
        /// re-files a trained skill by **re-ordering** the item list and re-running
        /// the layout update: the same handles, at the same sizes, in the same tree order, at
        /// different positions. A size-only key would compare equal, `origins` would keep the
        /// *pre-click* values, and the very next tick's `place_rows` would put every row back where
        /// it had been — so the model would re-file the row, the row's own cells would update, and
        /// **the drawn frame would not move**. Comparing against what this element itself last wrote separates "the screen
        /// re-placed the rows" from "I placed them there", which is the distinction the size-only
        /// key could not make.
        placed: Vec<(ElemHandle, i32, i32, i32, i32)>,
        /// The column and row counts, derived from the placed rows — the two numbers used to divide
        /// the paper size into each row's step.
        pub grid: (i32, i32),
        /// Whether this list has already registered itself on its bar(s).
        bars_bound: bool,
    }

    pub fn create(_l: &crate::LayoutDesc, _d: &crate::ElementDesc) -> Box<dyn Element> {
        Box::new(ListBox::default())
    }

    /// `ListBox`'s own layout attributes, taken off the client's
    /// attribute switch. Four of them are flag bits the constructor leaves clear.
    ///
    /// **The flag word starts at `0x290`**, i.e.
    /// `0x200 | 0x80 | 0x10` — so `ClickSelect`, `DragSelect`, `DragRollover`, `Horizontal` and
    /// `SelectedItemStateChange` are all **off** until a layout asks for them, and each has
    /// exactly one writer in the client (its setter helper) with exactly one caller (the arm
    /// below).
    ///
    /// They are read back out of the element's merged properties where they are needed rather
    /// than mirrored into a field, as the existing code already does for `0x5F` and `0x5C`; a
    /// mirror would be a second source of truth for data the node already holds. The attribute
    /// setter writes the instance property before it calls
    /// the attribute handler, so a runtime write is visible to the merged read too.
    pub mod attr {
        /// `0x59` — click-select, flag bit `0x2`. **The gate on both press
        /// sites.** 20 of the 35 shipped list boxes declare it.
        pub const CLICK_SELECT: u32 = 0x59;
        /// `0x5A` — drag rollover, bit 3. Nothing in the shipped data declares it.
        pub const DRAG_ROLLOVER: u32 = 0x5A;
        /// `0x5B` — drag select, bit 2. Five shipped list boxes declare it **false** and none
        /// declares it true, which is why the behavior the bit gates is not transcribed: it is
        /// unreachable in shipped data.
        pub const DRAG_SELECT: u32 = 0x5B;
        /// `0x5C` — the horizontal flag, bit 0.
        pub const HORIZONTAL: u32 = 0x5C;
        /// `0x5D` — the state `set_selected_item` puts on the row **losing** the selection.
        /// Read off the **list box**, not off the row.
        pub const UNSELECTED_STATE: u32 = 0x5D;
        /// `0x5E` — the state it puts on the row **gaining** it.
        pub const SELECTED_STATE: u32 = 0x5E;
        /// `0x5F` — the layout update's only attribute read.
        pub const MAX_COLUMNS: u32 = 0x5F;
        /// `0x61` — selected-item state change, bit 5. The gate on the two row-state writes
        /// above.
        pub const SELECTED_ITEM_STATE_CHANGE: u32 = 0x61;
    }

    /// One of the flag word's layout-driven bits, off the live element. See `attr`.
    fn bit(ui: &UiSystem, me: ElemHandle, id: u32) -> bool {
        ui.node(me)
            .map(crate::ElementNode::merged_properties)
            .and_then(|p| p.get_bool(id))
            .unwrap_or(false)
    }

    /// The two selecting **input actions** are 7 and `0x0A`.
    ///
    /// `7` is `PRIMARY_CLICK` and `0x0A` is the double-click resolution of the same button —
    /// input map 3 binds `DIMOFS_BUTTON0` twice, `Click` to 7 and `MouseDblClick` to `0x0A`, and
    /// the input resolver prefers the larger activation, so **the second press
    /// of a pair arrives as `0x0A`**. A list box that took only 7 would go deaf on every second
    /// click, which is exactly the gesture the list box is listening for.
    pub const PRESS_ACTIONS: [u32; 2] = [crate::focus::action::PRIMARY_CLICK, 0x0A];

    impl ListBox {
        /// The list box's selected-item setter, whole.
        ///
        /// ```text
        /// if item == selected:
        ///     if notify: broadcast 0x43 with (selected, 0)
        ///     return
        /// index = -1; selected = none
        /// for i in 0 .. items.len():
        ///     if items[i] == item: index = i; selected = item; break
        /// if flag bit 0x20:
        ///     if old:      old.set_state(enum attribute 0x5D)
        ///     if selected: new.set_state(enum attribute 0x5E)
        /// if notify: broadcast 4 with (index, selected)
        /// ```
        ///
        /// # The equal branch is the only producer of `0x43` in the client
        ///
        /// Nothing else in the client raises `0x43`, so **`0x43` means
        /// "you pressed the row that was already selected"** — it is not "double click" and it is
        /// not raised by `0x1A`. The double-click detector's one-second window is a
        /// *detector* built on top of it, and because one press runs this function **twice** (see
        /// [`Self::press_select`]) a real double-click is two presses.
        ///
        /// An early return when the item is already selected would silence exactly the line that
        /// broadcasts here, and `0x43` would then have to come from `MOUSE_DOUBLE_CLICK` instead,
        /// which is wrong.
        ///
        /// # An easy misreading
        ///
        /// The **second** notify test is easy to misread as a test of the *item*. Both tests read
        /// `notify`.
        ///
        /// The row call sets the row's **state**, *not* its media state.
        ///
        /// # One representational deviation, named
        ///
        /// Retail's selected item is a pointer and this build carries an **index** into
        /// [`Self::items`], so the one state retail can hold and this cannot is "selected, and the
        /// selected element is no longer in the item list". Every writer here goes through the
        /// array, `ListBoxWidget::mirror_items` carries the selection across by handle, and
        /// deletion clears it, so no path in this build produces that state.
        pub fn set_selected_item(
            &mut self,
            ui: &mut UiSystem,
            me: ElemHandle,
            item: Option<ElemHandle>,
            notify: bool,
        ) {
            let old = self.selected.and_then(|i| self.items.get(i).copied());
            if old == item {
                if notify {
                    // **Queued, not raised -- and that is the client's own behaviour.**
                    // The manager's own broadcast stamps the serial
                    // and then, if a broadcast is already in progress, queues the message and
                    // dispatches it only after the broadcast in flight has unwound. This function
                    // is reached from inside the `0x1C` broadcast, so raising here directly would
                    // allocate a larger serial, stamp every ancestor as it bubbled, and kill the
                    // `0x1C` where the nesting happened -- see
                    // [`crate::UiSystem::queue_element_message`]. Measured: it took the
                    // stat-management panel's own `0x1C` arm off the air, so neither stat panel
                    // could select a row any more.
                    ui.queue_element_message(
                        me,
                        msgid::LIST_ITEM_ACTIVATED,
                        old.map_or(0, ElemHandle::raw),
                        0,
                    );
                }
                return;
            }
            // The walk: not found leaves `index = -1` **and** no selected item, so a
            // `set_selected_item(some_stranger, true)` *clears* the selection rather than leaving
            // it.
            self.selected = item.and_then(|h| self.items.iter().position(|i| *i == h));
            let new = self.selected.and_then(|i| self.items.get(i).copied());
            if bit(ui, me, attr::SELECTED_ITEM_STATE_CHANGE) {
                // The enum attribute is read off the **list box** and its result is used
                // whether or not the attribute was found -- it goes
                // straight into the state setter -- so the eleven shipped lists that enable `0x61`
                // without naming the pair get the `MasterProperty` defaults, `0x5D = 1` and
                // `0x5E = 6`. Skipping `set_state` when the list box named no state would leave
                // every list but the five vendor panes out of its row template's authored state 6,
                // and the Friends list would draw no band.
                if let Some(o) = old {
                    let s = ui.get_attribute_enum(me, attr::UNSELECTED_STATE).1;
                    ui.set_state(o, crate::StateId(s));
                }
                if let Some(n) = new {
                    let s = ui.get_attribute_enum(me, attr::SELECTED_STATE).1;
                    ui.set_state(n, crate::StateId(s));
                }
            }
            if notify {
                // The index is `0xffffffff` when the walk found nothing, and the client sends it
                // as-is. A reader that wants an index reads it back as -1.
                let index = self
                    .selected
                    .and_then(|i| u32::try_from(i).ok())
                    .unwrap_or(u32::MAX);
                // Queued for the same reason as the `0x43` above.
                ui.queue_element_message(
                    me,
                    msgid::LIST_SELECTION_CHANGED,
                    index,
                    new.map_or(0, ElemHandle::raw),
                );
            }
        }

        /// Select by index, minus its scroll-to-show tail.
        ///
        /// ```text
        /// if index < items.len(): set_selected_item(items[index], notify)
        ///                         scroll items[index] into view
        /// else:                   set_selected_item(none, notify)
        /// ```
        ///
        /// **An out-of-range index clears the selection**, it does not leave it alone — and it
        /// still notifies. The scroll tail is [`scroll_item_to_view`], which needs this object out
        /// of its slot and so cannot be called from in here; a caller that wants both calls it
        /// afterwards, which is what the client's scroll-to-view users do.
        pub fn select(&mut self, ui: &mut UiSystem, me: ElemHandle, index: usize) {
            let item = self.items.get(index).copied();
            self.set_selected_item(ui, me, item, true);
        }

        /// The item-index-at-point query — which row is at an
        /// **element-relative** point.
        ///
        /// ```text
        /// if no items or x < 0 or width  <= x: return false
        /// if             y < 0 or height <= y: return false
        /// run the layout update
        /// ix = x + scroll.x;  iy = y + scroll.y
        /// col = first c with sum(item_widths [0..=c]) >= ix, else 0
        /// row = first r with sum(item_heights[0..=r]) >= iy, else 0
        /// index = horizontal ? cols * row + col : rows * col + row
        /// return index < items.len()
        /// ```
        ///
        /// The `else 0` is the client's: each loop's counter keeps the value it was initialised
        /// with when the accumulation never reaches the point, and both start at 0.
        ///
        /// The item widths and heights are the layout update's first pass — the per-column and
        /// per-row **maxima**. They are recomputed here from the rows' own **unscrolled** origins
        /// (`box + scroll offset`, reversing the placement subtraction) rather
        /// than cached, because in this build the rows may have been placed either by this
        /// element's `update_layout` or by a screen-side binder, and a cached pair would be stale
        /// for exactly one of the two. Over rows on a grid the two readings are the same numbers:
        /// the distinct origins on each axis are the column and row counts, and a band's size is
        /// the largest row sitting on it.
        /// **This is the function the item-under-mouse query needs and the widget did not have.** The twin in
        /// `dereth_ui_screens::panels::listbox::ListBoxWidget` stays where it is: it
        /// answers for the binder's own row array, which the two stat panels index into.
        #[must_use]
        pub fn inq_item_index_at_point(
            &self,
            ui: &UiSystem,
            me: ElemHandle,
            x: i32,
            y: i32,
        ) -> Option<usize> {
            if self.items.is_empty() {
                return None;
            }
            let b = ui.node(me).map(|n| n.region.box_)?;
            if x < 0 || b.width() <= x || y < 0 || b.height() <= y {
                return None;
            }
            // `(unscrolled x, unscrolled y, width, height)` per row, in item-list order.
            let cells: Vec<(i32, i32, i32, i32)> = self
                .items
                .iter()
                .map(|h| {
                    ui.node(*h).map_or((0, 0, 0, 0), |n| {
                        let r = n.region.box_;
                        (
                            r.x0 + self.scroll.x,
                            r.y0 + self.scroll.y,
                            r.width(),
                            r.height(),
                        )
                    })
                })
                .collect();
            let band_sizes = |axis: fn(&(i32, i32, i32, i32)) -> i32,
                              size: fn(&(i32, i32, i32, i32)) -> i32|
             -> Vec<i32> {
                let mut origins: Vec<i32> = cells.iter().map(axis).collect();
                origins.sort_unstable();
                origins.dedup();
                origins
                    .iter()
                    .map(|o| {
                        cells
                            .iter()
                            .filter(|c| axis(c) == *o)
                            .map(size)
                            .max()
                            .unwrap_or(0)
                    })
                    .collect()
            };
            let widths = band_sizes(|c| c.0, |c| c.2);
            let heights = band_sizes(|c| c.1, |c| c.3);
            let band = |sizes: &[i32], at: i32| -> usize {
                let mut acc = 0i32;
                for (i, s) in sizes.iter().enumerate() {
                    acc += *s;
                    if at <= acc {
                        return i;
                    }
                }
                0
            };
            let col = band(&widths, x + self.scroll.x);
            let row = band(&heights, y + self.scroll.y);
            let index = if bit(ui, me, attr::HORIZONTAL) {
                widths.len() * row + col
            } else {
                heights.len() * col + row
            };
            (index < self.items.len()).then_some(index)
        }

        /// The list box's item-under-mouse query.
        ///
        /// ```text
        /// if the mouse is not over the list: return none
        /// if inq_item_index_at_point(mouse.x - screen.x0, mouse.y - screen.y0) is i:
        ///     return items[i]
        /// return none
        /// ```
        ///
        /// `(wx, wy)` is the pointer in **window** coordinates — `ElementMessage::point.window`,
        /// which the manager's mouse-down event stamped from the same
        /// input-device mouse position the client reads here. It is converted against **this
        /// element's** screen origin rather than the message's, so a message that bubbled up from a
        /// child still resolves against the right rectangle, exactly as the client's global read
        /// does.
        ///
        /// The mouse-over guard is subsumed by [`Self::inq_item_index_at_point`]'s own bounds
        /// test: a point outside the list box's rectangle is precisely the case the mouse-over flag
        /// is false for. (The same reading the screen-side twin records.)
        #[must_use]
        pub fn get_item_under_mouse(
            &self,
            ui: &UiSystem,
            me: ElemHandle,
            wx: i32,
            wy: i32,
        ) -> Option<ElemHandle> {
            let b = ui.screen_box(me);
            let i = self.inq_item_index_at_point(ui, me, wx - b.x0, wy - b.y0)?;
            self.items.get(i).copied()
        }

        /// One of the two press sites, which are the same four statements:
        ///
        /// ```text
        /// if flag bit 0x2:
        ///     item = get_item_under_mouse()
        ///     if item: set_selected_item(item, true)
        ///              if flag bit 0x4: start a drag select
        /// ```
        ///
        /// Drag selection is **not** transcribed: flag bit `0x4` is attribute
        /// `0x5B`, and a census of the shipped layouts finds five list boxes
        /// declaring it *false* and **none** declaring it true, so the call is unreachable in
        /// shipped data. Named here rather than silently dropped.
        ///
        /// Returns whether a row was under the pointer.
        fn press_select(&mut self, ui: &mut UiSystem, me: ElemHandle, wx: i32, wy: i32) -> bool {
            if !bit(ui, me, attr::CLICK_SELECT) {
                return false;
            }
            let Some(item) = self.get_item_under_mouse(ui, me, wx, wy) else {
                return false;
            };
            self.set_selected_item(ui, me, Some(item), true);
            true
        }

        /// Put one already-created element into
        /// the item list at `index`, under this list.
        ///
        /// The client's insert also marks the layout dirty; here row placement belongs to the
        /// list box and runs when the caller asks for it.
        pub fn insert_item(
            &mut self,
            ui: &mut UiSystem,
            me: ElemHandle,
            item: ElemHandle,
            index: usize,
        ) -> bool {
            if ui.node(item).is_none() {
                return false;
            }
            ui.set_parent(item, Some(me));
            let at = index.min(self.items.len());
            self.items.insert(at, item);
            if let Some(sel) = self.selected {
                if at <= sel {
                    self.selected = Some(sel + 1);
                }
            }
            true
        }

        /// Drop every row and its element.
        pub fn flush(&mut self, ui: &mut UiSystem) {
            self.selected = None;
            for h in std::mem::take(&mut self.items) {
                ui.remove_and_delete_root(h);
            }
            self.origins.clear();
            self.fingerprint.clear();
            self.placed.clear();
        }

        /// Return row `i`, or `None` when it is out of range.
        #[must_use]
        pub fn get_item(&self, index: usize) -> Option<ElemHandle> {
            self.items.get(index).copied()
        }

        /// The client's two passes, over the item list.
        ///
        /// ```text
        /// cols = int attribute 0x5F                 // absent == 0
        /// if cols < 0: cols = n; rows = (n != 0)
        /// else:        cols = min(max(cols, 1), n); rows = ceil(n / cols)
        /// // widths[col] = max item width in that column, heights[row] = max item height in it,
        /// // then place each item at the running sum, wrapping on cols (horizontal) or on
        /// // rows (the default, which fills column-major)
        /// ```
        ///
        /// **This is the same arithmetic as
        /// `dereth_ui_screens::panels::listbox::ListBoxWidget::update_layout`, which
        /// keeps its own row array because it is a screen-side binder for lists
        /// whose rows come from a template list.** The copy is here because a menu popup is built
        /// inside this crate and has no screen-side binder; the two were compared line for line
        /// and they agree. They could be reconciled onto this one.
        ///
        /// Returns `(cols, rows)`.
        pub fn update_layout(&mut self, ui: &mut UiSystem, me: ElemHandle) -> (i32, i32) {
            let n = i32::try_from(self.items.len()).unwrap_or(0);
            let props = ui.node(me).map(crate::ElementNode::merged_properties);
            let max_columns = props.as_ref().and_then(|p| p.get_int(0x5F)).unwrap_or(0);
            let horizontal = props
                .as_ref()
                .and_then(|p| p.get_bool(0x5C))
                .unwrap_or(false);
            let (cols, rows) = if max_columns < 0 {
                (n, i32::from(n != 0))
            } else {
                let cols = max_columns.max(1).min(n);
                let rows = if cols == 0 { 0 } else { (n + cols - 1) / cols };
                (cols, rows)
            };
            self.grid = (cols.max(0), rows.max(0));
            if cols <= 0 || rows <= 0 {
                return self.grid;
            }
            let ncols = usize::try_from(cols).unwrap_or(0);
            let nrows = usize::try_from(rows).unwrap_or(0);
            let mut widths = vec![0i32; ncols];
            let mut heights = vec![0i32; nrows];
            let (mut col, mut row) = (0usize, 0usize);
            for h in &self.items {
                let b = ui.node(*h).map(|nd| nd.region.box_).unwrap_or_default();
                if let Some(w) = widths.get_mut(col) {
                    *w = (*w).max(b.width());
                }
                if let Some(ht) = heights.get_mut(row) {
                    *ht = (*ht).max(b.height());
                }
                if horizontal {
                    if col == ncols - 1 {
                        col = 0;
                        row += 1;
                    } else {
                        col += 1;
                    }
                } else if row == nrows - 1 {
                    row = 0;
                    col += 1;
                } else {
                    row += 1;
                }
            }
            let (mut x, mut y) = (0i32, 0i32);
            let (mut col, mut row) = (0usize, 0usize);
            for h in self.items.clone() {
                ui.move_to(h, x, y);
                if horizontal {
                    if col == ncols - 1 {
                        x = 0;
                        col = 0;
                        y += heights.get(row).copied().unwrap_or(0);
                        row += 1;
                    } else {
                        x += widths.get(col).copied().unwrap_or(0);
                        col += 1;
                    }
                } else if row == nrows - 1 {
                    y = 0;
                    x += widths.get(col).copied().unwrap_or(0);
                    row = 0;
                    col += 1;
                } else {
                    y += heights.get(row).copied().unwrap_or(0);
                    row += 1;
                }
            }
            self.grid
        }

        // ---- `Scrollable`'s half ------------------------------------------------------

        /// The scroll refresh plus the tail of the layout update, driven off the rows' own
        /// geometry.
        ///
        /// The client sums the item widths over the columns and the item heights over the rows —
        /// the per-column and per-row maxima it built in the layout update's first pass — and hands
        /// the two totals to the scrollable-area resize. Over rows that are already placed on
        /// that grid, the same two totals are `max(x + width)` and `max(y + height)`, which is what
        /// this computes; and the number of distinct origins on each axis is the column/row count.
        ///
        /// Returns whether anything changed.
        pub fn refresh_scroll(&mut self, ui: &mut UiSystem, me: ElemHandle) -> bool {
            // The client's registration, done here rather than in
            // `post_init` because a list and its bar are **siblings**: nothing the bar broadcasts
            // would otherwise bubble through the list, and the bar may not be in the tree yet when
            // the list is initialised. Once, guarded, so the listener list cannot grow.
            if !self.bars_bound {
                for horizontal in [true, false] {
                    if let Some(bar) = self.scroll.scrollbar(ui, me, horizontal) {
                        ui.register_for_element_messages(bar, crate::ListenerId::Element(me));
                        self.bars_bound = true;
                    }
                }
            }
            let children = if self.items_are_authoritative {
                self.items.clone()
            } else {
                ui.children(me)
            };
            let rows: Vec<(ElemHandle, i32, i32, i32, i32)> = children
                .into_iter()
                .filter_map(|c| {
                    let b = ui.node(c)?.region.box_;
                    Some((c, b.x0, b.y0, b.width(), b.height()))
                })
                .collect();
            if rows != self.placed {
                // A row was added, removed, resized **or moved by someone other than this
                // element**, so `ListBoxWidget::update_layout` has just re-placed every one of
                // them **unscrolled**: their current boxes are the origins. Position is part of
                // the key because a re-order is the only change the stat panel's re-file
                // makes, and it changes nothing else.
                self.fingerprint = rows.iter().map(|(c, _, _, w, h)| (*c, *w, *h)).collect();
                self.origins = rows.iter().map(|(c, x, y, _, _)| (*c, *x, *y)).collect();
                let mut xs: Vec<i32> = self.origins.iter().map(|(_, x, _)| *x).collect();
                let mut ys: Vec<i32> = self.origins.iter().map(|(_, _, y)| *y).collect();
                xs.sort_unstable();
                xs.dedup();
                ys.sort_unstable();
                ys.dedup();
                self.grid = (
                    i32::try_from(xs.len()).unwrap_or(0),
                    i32::try_from(ys.len()).unwrap_or(0),
                );
            }
            let (mut w, mut h) = (0, 0);
            for ((_, ox, oy), (_, cw, ch)) in self.origins.iter().zip(self.fingerprint.iter()) {
                w = w.max(ox + cw);
                h = h.max(oy + ch);
            }
            let mut changed = self.scroll.resize_scrollable_area(ui, me, w, h);
            changed |= self.scroll.update_scrollbar_size(ui, me, true);
            changed |= self.scroll.update_scrollbar_size(ui, me, false);
            self.place_rows(ui);
            changed
        }

        /// The client's second pass: every row sits at its own grid position less
        /// the scroll offset. Rows pushed outside the list box are clipped by
        /// the region draw routine's ancestor intersection, so nothing else is needed to hide
        /// them.
        fn place_rows(&mut self, ui: &mut UiSystem) {
            for (h, ox, oy) in self.origins.clone() {
                ui.move_to(h, ox - self.scroll.x, oy - self.scroll.y);
            }
            self.scroll_offset = (self.scroll.x, self.scroll.y);
            // Record what was just written, so the next `refresh_scroll` can tell its own
            // placement apart from one the screen made. The stored value keeps the
            // width and height, so this is exactly the boxes the rows now hold.
            self.placed = self
                .origins
                .iter()
                .zip(self.fingerprint.iter())
                .map(|((h, ox, oy), (_, w, ht))| {
                    (*h, ox - self.scroll.x, oy - self.scroll.y, *w, *ht)
                })
                .collect();
        }

        /// Behavior: the scroll-delta query, the step in **pixels**
        /// one arrow click or one track click moves the list.
        ///
        /// ```text
        /// if page: step = horizontal ? width : height
        /// else:
        ///     n = horizontal ? cols : rows
        ///     if n != 0:
        ///         item = (horizontal ? scrollable width : scrollable height) / n
        ///         view = horizontal ? width : height
        ///         step = (view <= item) ? view : item      // i.e. min(view, one row)
        /// return negative ? -step : step
        /// ```
        ///
        /// So an **arrow** moves the list by exactly one row and a **track click** by one whole
        /// view — a page. Note this is not `TextElement`'s, whose
        /// page arm is `view - step`; the list's page arm is the plain view height.
        #[must_use]
        pub fn inq_scroll_delta(
            &self,
            ui: &UiSystem,
            me: ElemHandle,
            horizontal: bool,
            negative: bool,
            page: bool,
        ) -> i32 {
            let b = ui
                .node(me)
                .map_or_else(crate::Box2D::default, |n| n.region.box_);
            let view = if horizontal { b.width() } else { b.height() };
            let mut step = 0;
            if page {
                step = view;
            } else {
                let n = if horizontal { self.grid.0 } else { self.grid.1 };
                if n != 0 {
                    let content = if horizontal {
                        self.scroll.width
                    } else {
                        self.scroll.height
                    };
                    let item = content / n;
                    step = if view <= item { view } else { item };
                }
            }
            if negative {
                -step
            } else {
                step
            }
        }
    }

    // ---- driving `Scrollable`'s half from outside the arena -----------------------
    //
    // The list box is a scrollable in the client, so the offset, the row placement
    // and the hit test are all one object's business and the layout update does the lot
    // in one call. In this build the *rows* are built and measured by
    // `dereth_ui_screens::panels::listbox::ListBoxWidget`, which is a screen-side object with no
    // access to this behaviour's slot. These three functions are the seam: they are what a screen
    // calls instead of reaching into `ElementNode::behaviour` by hand, and they lift the behaviour
    // through `take_behaviour`/`put_behaviour` so the deferred state and mouse-visibility flushes
    // keep their bookkeeping.
    //
    // Every one of them answers `None`/`false` when the behaviour is not in its slot, which is the
    // case while this element's own handler is running: a widget cannot be read back out of the
    // arena from inside its own handler. A screen must not call them
    // from there, and getting a silent `false` rather than a panic is deliberate.

    /// Read the horizontal and vertical scroll offsets from a live list box.
    ///
    /// **This is the number the client adds back before it walks the
    /// bands**. A screen-side copy that nothing wrote would let
    /// the rows scroll (this behaviour's row placement moves them) while the hit test
    /// went on answering as though they had not.
    #[must_use]
    pub fn scroll_offset_of(ui: &UiSystem, list: ElemHandle) -> Option<(i32, i32)> {
        let l = ui
            .node(list)?
            .behaviour
            .as_ref()?
            .as_any()?
            .downcast_ref::<ListBox>()?;
        Some((l.scroll.x, l.scroll.y))
    }

    /// The scrollable's offset setter followed by the list box's adjust-to-scrollable-change —
    /// move the offset (clamped) and
    /// re-place every row against it. Returns whether the offset moved.
    pub fn set_scroll_offset(ui: &mut UiSystem, list: ElemHandle, x: i32, y: i32) -> bool {
        let Some(mut b) = ui.take_behaviour(list) else {
            return false;
        };
        let moved = match b.as_any_mut().and_then(|a| a.downcast_mut::<ListBox>()) {
            Some(l) => {
                let m = l.scroll.set_scrollable_xy(ui, list, x, y, false);
                l.place_rows(ui);
                m
            }
            None => false,
        };
        ui.put_behaviour(list, b);
        moved
    }

    /// Scroll a row into view, over an active row's unscrolled origin.
    /// The screen binder has already placed the grid; refresh captures its per-band sums.
    /// Both before-viewport arms use the row's top-left. The right/bottom arms adjust only
    /// one coordinate before passing both to set_scrollable_xy; do not independently minimize axes.
    pub fn scroll_item_to_view(ui: &mut UiSystem, list: ElemHandle, item: ElemHandle) -> bool {
        let Some(mut b) = ui.take_behaviour(list) else {
            return false;
        };
        let moved = (|| {
            let l = b.as_any_mut()?.downcast_mut::<ListBox>()?;
            l.refresh_scroll(ui, list);
            let (_, x, y) = l.origins.iter().find(|(h, _, _)| *h == item).copied()?;
            let view = ui.node(list)?.region.box_;
            let row = ui.node(item)?.region.box_;
            let (sx, sy) = (l.scroll.x, l.scroll.y);
            let (nx, ny) = if x < sx || y < sy {
                (x, y)
            } else if x > sx + view.width() - row.width() {
                (x - view.width() + row.width(), y)
            } else if y > sy + view.height() - row.height() {
                (x, y - view.height() + row.height())
            } else {
                return Some(false);
            };
            let changed = l.scroll.set_scrollable_xy(ui, list, nx, ny, false);
            l.place_rows(ui);
            Some(changed)
        })()
        .unwrap_or(false);
        ui.put_behaviour(list, b);
        moved
    }

    /// The selected-item setter against a bare list-box handle performs the list box's normal
    /// selection operation. It is reached from **outside** `dereth-ui`, where
    /// [`UiSystem::take_behaviour`] / [`UiSystem::put_behaviour`] are `pub(crate)`.
    ///
    /// The spell-component panel's selection-changed notice ends in
    /// selecting `item` on the component list box with notify set — with the matched row on one arm
    /// and a null on the clear arm — and it is a
    /// *panel* in `dereth-ui-screens` that has to make that call. Writing the
    /// selection straight onto the behaviour instead would lose the row **state** change
    /// (the selection band) and the queued `4`, both of which this keeps.
    ///
    /// Returns whether the handle was a list box at all, so a caller can tell "cleared" from
    /// "there was nothing to clear".
    pub fn set_selected_item_of(
        ui: &mut UiSystem,
        list: ElemHandle,
        item: Option<ElemHandle>,
        notify: bool,
    ) -> bool {
        let Some(mut b) = ui.take_behaviour(list) else {
            return false;
        };
        let ok = match b.as_any_mut().and_then(|a| a.downcast_mut::<ListBox>()) {
            Some(l) => {
                l.set_selected_item(ui, list, item, notify);
                true
            }
            None => false,
        };
        ui.put_behaviour(list, b);
        ok
    }

    /// The list box's layout-update tail — the scrollable-area resize
    /// over the row grid, then the second pass that places every row at
    /// `running_sum - scroll offset`.
    ///
    /// The screen-side widget owns the *first* pass (the per-column and per-row maxima) and calls
    /// this at the end of its own `update_layout`, so the two halves run in one frame exactly as
    /// the client's single function does. Without it the rows sit unscrolled until the next global
    /// tick, and anything that reads geometry in between sees a list that has forgotten where it
    /// was scrolled to.
    pub fn refresh_scroll_of(ui: &mut UiSystem, list: ElemHandle) -> bool {
        let Some(mut b) = ui.take_behaviour(list) else {
            return false;
        };
        let changed = match b.as_any_mut().and_then(|a| a.downcast_mut::<ListBox>()) {
            Some(l) => l.refresh_scroll(ui, list),
            None => false,
        };
        ui.put_behaviour(list, b);
        changed
    }

    impl Element for ListBox {
        /// The list box's original mouse-visibility behavior.
        ///
        /// The base mouse-visibility result matters only in disabled state `0x0D`; every other
        /// list-box state is mouse-visible.
        ///
        /// i.e. **a list box is always mouse-visible except when it is disabled** — the same folded
        /// `return true` shape as on `Button`, and the same consequence:
        /// without it the mouse hit tester can never return a list, so nothing in
        /// one can be clicked, dragged or dropped on. The item-list widget shares it, so without it
        /// the inventory lists are invisible to the pointer. The state-`0xD` (disabled)
        /// arm is the one exception and is reproduced by this type's `disabled` field.
        fn should_be_mouse_visible(&self) -> bool {
            !self.disabled
        }

        /// The original list box begins mouse-down with the shared scrollable behavior, so a press
        /// on a list moves
        /// the focus element.
        ///
        /// This is the case with a **picture**: a list box has no press state of its own, so the
        /// `0x2F` arm moves it out of state 0/1/5. That is retail's own behaviour and it is what
        /// makes focus-driven `0x0A` registration possible at all — a list that can
        /// never hold focus can never register the wheel map.
        fn takes_focus_on_press(&self) -> bool {
            true
        }

        fn as_any(&self) -> Option<&dyn std::any::Any> {
            Some(self)
        }

        /// The mutable list-box downcast used by menu operations.
        fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
            Some(self)
        }

        /// The scrollable's two attribute arms, reached here because
        /// the list box is a scrollable and its attribute setter chains to
        /// the base.
        fn on_set_attribute(
            &mut self,
            ctx: &mut ElemCtx<'_>,
            id: u32,
            v: Option<&crate::props::PropertyValue>,
        ) {
            use crate::props::PropertyValue;
            let bar = match v {
                Some(PropertyValue::Enum(e)) => Some(crate::ElementId(*e)),
                Some(PropertyValue::Integer(i)) => u32::try_from(*i).ok().map(crate::ElementId),
                _ => None,
            };
            match id {
                crate::scrollable::attr::H_SCROLLBAR => {
                    self.scroll.h_scrollbar = bar;
                    if bar.is_some() {
                        self.scroll.update_scrollbar_size(ctx.ui, ctx.me, true);
                    }
                }
                crate::scrollable::attr::V_SCROLLBAR => {
                    self.scroll.v_scrollbar = bar;
                    if bar.is_some() {
                        self.scroll.update_scrollbar_size(ctx.ui, ctx.me, false);
                    }
                }
                _ => {}
            }
        }

        /// Behavior: register on whichever bars `0x71` and
        /// `0x72` name, and size them once. The registration is what makes the bar's messages
        /// arrive at all: a list and its bar are **siblings** in every shipped char-gen layout, so
        /// nothing the bar broadcasts would otherwise bubble through the list.
        fn post_init(&mut self, ctx: &mut ElemCtx<'_>) {
            if self.scroll.h_scrollbar.is_none() && self.scroll.v_scrollbar.is_none() {
                return;
            }
            let me = ctx.me;
            self.refresh_scroll(ctx.ui, me);
            // The layout update's dirty bit `0x200` is consumed by the client's own layout pass;
            // this crate has none, so the list re-measures itself on the frame tick instead.
            ctx.ui.want_tick(ctx.me, true);
        }

        fn listen_to_element_message(&mut self, ctx: &mut ElemCtx<'_>, m: &ElementMessage) -> R {
            // The scrollable's element-message handler, first arm, which
            // the list box's own handler reaches by chaining to the
            // base. Answered before anything else, because the message comes from the bar and not
            // from a row.
            if let Some(horizontal) =
                self.scroll
                    .message_is_from_my_bar(ctx.ui, ctx.me, m.source_id, m.source)
            {
                let delta = if crate::scrollable::Scrollable::is_step_message(m.id) {
                    // **The negate group is `0x0D` and `0x0F`, taken from retail rather than
                    // reasoned about.** The scrollbar-message helper builds
                    // `inq_scroll_delta`'s three arguments as
                    //
                    // ```text
                    //   page     = (id == 0x0F || id == 0x10)    "a page rather than a line"
                    //   negative = (id == 0x0D || id == 0x0F)    "negate the step"
                    //   horizontal
                    // ```
                    //
                    // and the text element's and the list box's scroll-delta queries both end
                    // by negating the step when `negative` is set. It is not `0x0E || 0x0F`, as
                    // where the arrows *sit* might suggest: `0x0F` is a track click above the
                    // thumb and must page up, and it groups with `0x0D`, the **increment** arrow —
                    // the one the scrolling-area update moves to `(0, 0)`, i.e. the top.
                    //
                    // Getting both this flag and the arrow placement wrong cancels on every
                    // vertical bar in the shipped data, so a screenshot cannot see it; it does
                    // not cancel on a horizontal bar. The scroll and chat tests assert direction.
                    let negative = m.id.0 == 0x0D || m.id.0 == 0x0F;
                    let page = m.id.0 == 0x0F || m.id.0 == 0x10;
                    self.inq_scroll_delta(ctx.ui, ctx.me, horizontal, negative, page)
                } else {
                    0
                };
                let me = ctx.me;
                if self
                    .scroll
                    .handle_scrollbar_message(ctx.ui, me, horizontal, m.id, delta)
                {
                    // Behavior: mark the layout dirty, which is
                    // what re-places every row against the new offset.
                    self.place_rows(ctx.ui);
                }
                return R::StopProcessing;
            }
            // The base scrollable handler's second arm handles wheel input. See the twin in
            // `text::element_text` for why the reflected message is applied here rather than
            // waited for.
            if let Some(bar) = self.scroll.wheel_target(ctx.ui, ctx.me, m.id, m.p1) {
                let up = m.p1 == crate::focus::action::WHEEL_UP;
                if let Some(id) = crate::widgets::scrollbar::wheel(ctx.ui, bar, up) {
                    // The same group as above; see `listen_to_element_message`'s first arm.
                    let negative = id.0 == 0x0D || id.0 == 0x0F;
                    let page = id.0 == 0x0F || id.0 == 0x10;
                    let me = ctx.me;
                    let delta = self.inq_scroll_delta(ctx.ui, me, false, negative, page);
                    if self
                        .scroll
                        .handle_scrollbar_message(ctx.ui, me, false, id, delta)
                    {
                        self.place_rows(ctx.ui);
                    }
                }
                return R::StopProcessing;
            }
            // ---- the press, which has two list-box selection sites --------------------------
            // The inherited mouse-down first broadcasts `0x1C`; the list-box handler selects the
            // row for primary action 7 when click-select bit 2 is set. Mouse-down's own tail then
            // selects for action 7 or `0x0A` under the same bit, unless moving or resizing.
            //
            // **Both sites are here, in that order, and that is not a duplicate.** In the client
            // one is reached through the base class's broadcast and the other is the mouse-down's
            // own tail; this build has no separate mouse-down hook on the behaviour — the element's
            // press body is reached from its `0x1C` arm, which is the seam
            // `text::element_text::TextElement::mouse_down` already uses — so the two land in one
            // function. Collapsing them to one call would be a behaviour change, not a cleanup:
            // the **doubling is what makes a double-click two presses**. `set_selected_item` raises
            // `0x43` whenever it is handed the row that is already selected, so press one raises
            // one `0x43` (from the tail, over the row the first call just selected) and press two
            // raises two — and the double-click behavior is armed by the first and fires on
            // the second. A list that ran this once per press would need **three** clicks to open
            // a journal page.
            //
            // The two guards differ and both are transcribed: the `0x1C` arm takes action `7`
            // alone, the tail takes `7` or `0x0A` and is skipped while the element is being moved
            // or resized. Note this is *narrower* than the stat panels' own `0x1C` arms, which also
            // takes `8` and `0xB` — the two stat panels are more permissive than the widget, which
            // is why their copies are not duplicates of this one.
            if m.id == msgid::MOUSE_PRESS {
                let me = ctx.me;
                let (wx, wy) = m.point.window;
                if m.p1 == crate::focus::action::PRIMARY_CLICK {
                    self.press_select(ctx.ui, me, wx, wy);
                }
                let busy = ctx
                    .ui
                    .node(me)
                    .is_some_and(|n| n.flags.is_moving() || n.flags.is_resizing());
                if !busy && PRESS_ACTIONS.contains(&m.p1) {
                    self.press_select(ctx.ui, me, wx, wy);
                }
                // The list box's handler delegates to the base, so
                // the base still sees the press: it is what moves the element into state 4.
                return R::Default;
            }
            R::Default
        }

        fn listen_to_global_message(&mut self, ctx: &mut ElemCtx<'_>, id: MessageId, _p: u32) {
            if id != crate::msg::global::TICK {
                return;
            }
            // A list that owns a bar keeps ticking: the tick is where the layout update's dirty bit
            // is consumed, and a row added after `post_init` has to reach the bar.
            if self.scroll.h_scrollbar.is_some() || self.scroll.v_scrollbar.is_some() {
                let me = ctx.me;
                self.refresh_scroll(ctx.ui, me);
                return;
            }
            if self.anim.is_none() {
                ctx.ui.want_tick(ctx.me, false);
            }
        }
    }
}

/// `Menu` (6) — a drop-down: a `Button` that owns a popup containing a
/// `ListBox`. Broadcasts **7** for selection, **8** when opened, and **9** when closed.
pub mod menu {
    use super::*;
    use crate::ElementId;

    /// Attribute **2** — the list box's element id inside the popup.
    /// The menu's initialisation reads enum attribute 2, finds that descendant of the popup and
    /// requires it to be type 5.
    pub const ATTR_LIST_BOX: u32 = 2;
    /// Attribute **6** — the popup's root element id inside the layout attribute **7** names.
    /// Popup construction reads the layout from attribute 7, falls back to the menu's own layout,
    /// reads the root id from attribute 6, and creates that root from the selected layout.
    pub const ATTR_POPUP_ROOT: u32 = 6;
    /// Attribute **7** — the layout the popup root is built out of, a `DataFile`.
    /// Popup construction falls back to the menu's own layout when it is absent.
    pub const ATTR_POPUP_LAYOUT: u32 = 7;
    /// Attribute **9** — the element id one row is created from.
    /// The insert-text-item reads it as an enum attribute.
    pub const ATTR_ITEM_ELEMENT: u32 = 9;
    /// Attribute **10** — the layout that row template lives in, with the same fall-back to the
    /// menu's own layout.
    pub const ATTR_ITEM_LAYOUT: u32 = 10;
    /// Attribute **1** — an initial selected item id, read during menu initialization and skipped
    /// when zero.
    pub const ATTR_INITIAL_SELECTION: u32 = 1;
    /// Attribute **3** — centres the popup on the menu instead of aligning
    /// their left edges.
    pub const ATTR_POPUP_CENTRED: u32 = 3;
    /// Attribute **5** — puts the popup **above** the menu rather than below
    /// it. The chat window's talk-focus menu `0x10000014` carries `5 = true`, which is why its
    /// fourteen rows rise out of the bottom of the screen.
    pub const ATTR_POPUP_ABOVE: u32 = 5;
    /// Attribute **8** — the `TextElement` inside the menu that the selection
    /// copies the chosen row's pre-parsed text into.
    pub const ATTR_SELECTION_TEXT: u32 = 8;
    /// Attribute **0x0E** — the open flag, written by the open and close operations.
    /// It is `UICore_Button_toggled`, which is how an open drop-down draws pressed.
    pub const ATTR_OPEN: u32 = 0x0E;

    #[derive(Debug, Default)]
    pub struct Menu {
        /// **A menu is a button, which is a text element** — the drop-down shows the
        /// chosen row's text on its own face.
        pub button: super::button::Button,
        /// The menu is open.
        pub open: bool,
        /// The popup — the element [`make_popup`] created, a **root** of the manager and not a
        /// child of the menu.
        pub popup: Option<ElemHandle>,
        /// The list box — the `ListBox` [`initialize_popup`] found inside the popup.
        pub list_box: Option<ElemHandle>,
        /// Attribute [`ATTR_POPUP_ROOT`]'s value: which element of the popup layout to build.
        pub popup_id: Option<ElementId>,
        /// Attribute [`ATTR_LIST_BOX`]'s value: which element inside the popup is the list.
        pub list_box_id: Option<ElementId>,
        /// Attribute [`ATTR_ITEM_ELEMENT`]'s value: the row template's element id.
        pub item_id: Option<ElementId>,
        /// Attribute [`ATTR_SELECTION_TEXT`]'s value.
        pub selection_text_id: Option<ElementId>,
        /// Attribute [`ATTR_POPUP_LAYOUT`]'s value.
        pub popup_layout: Option<crate::DataId>,
        /// Attribute [`ATTR_ITEM_LAYOUT`]'s value.
        pub item_layout: Option<crate::DataId>,
        /// The list box's x and y borders — the popup's size less the list box's,
        /// latched by [`initialize_popup`] and added back by [`recalculate_popup_size`].
        pub border: (i32, i32),
    }

    pub fn create(_l: &crate::LayoutDesc, _d: &crate::ElementDesc) -> Box<dyn Element> {
        Box::new(Menu::default())
    }

    /// The menu's selected-index query — the index of the selected item within the list's rows, or
    /// **-1** when there is no list, no selection, or no matching row.
    ///
    /// **The `-1`s are the point, not the fallback.** The confirmation-menu dialog's cancel arm
    /// never calls this at all — its answer is primed with `-1` and the query runs only on the
    /// accept arm — and both menu subclasses' cancel operations write a literal `-1`.
    /// A menu with no list box answering -1 is therefore
    /// indistinguishable, by design, from a menu whose player chose nothing.
    ///
    /// The list box is written by [`initialize_popup`], which is the initialisation's second
    /// half and runs after [`make_popup`]; until both have run this answers -1.
    #[must_use]
    pub fn selected_index(ui: &UiSystem, me: ElemHandle) -> i32 {
        let Some(list_h) = list_box_handle(ui, me) else {
            return -1;
        };
        let Some(list) = ui.node(list_h).and_then(|n| {
            n.behaviour
                .as_ref()?
                .as_any()?
                .downcast_ref::<super::listbox::ListBox>()
        }) else {
            return -1;
        };
        list.selected
            .and_then(|i| i32::try_from(i).ok())
            .unwrap_or(-1)
    }

    /// The list box — the `ListBox` [`initialize_popup`] resolved inside the popup.
    ///
    /// `None` is the client's null list box, which every menu member answers -1 / false /
    /// nothing to. It is **not** a recursive lookup of
    /// attribute 2 below the *menu*: the initialisation looks
    /// below the **popup**, and the popup is a root
    /// element rather than a child of the menu, so a lookup below the menu
    /// could only ever succeed on a hand-built tree.
    #[must_use]
    pub fn list_box_handle(ui: &UiSystem, me: ElemHandle) -> Option<ElemHandle> {
        let menu = ui
            .node(me)?
            .behaviour
            .as_ref()?
            .as_any()?
            .downcast_ref::<Menu>()?;
        let list = menu.list_box?;
        ui.node(list).map(|_| list)
    }

    /// The popup root [`make_popup`] created, or `None`.
    #[must_use]
    pub fn popup_handle(ui: &UiSystem, me: ElemHandle) -> Option<ElemHandle> {
        let menu = ui
            .node(me)?
            .behaviour
            .as_ref()?
            .as_any()?
            .downcast_ref::<Menu>()?;
        let popup = menu.popup?;
        ui.node(popup).map(|_| popup)
    }

    fn with_menu<T>(ui: &UiSystem, me: ElemHandle, f: impl FnOnce(&Menu) -> T) -> Option<T> {
        Some(f(ui
            .node(me)?
            .behaviour
            .as_ref()?
            .as_any()?
            .downcast_ref::<Menu>()?))
    }

    fn with_menu_mut<T>(
        ui: &mut UiSystem,
        me: ElemHandle,
        f: impl FnOnce(&mut Menu) -> T,
    ) -> Option<T> {
        Some(f(ui
            .node_mut(me)?
            .behaviour
            .as_mut()?
            .as_any_mut()?
            .downcast_mut::<Menu>()?))
    }

    /// Lift the `Menu` **out of its arena slot** for the duration of `f`, exactly as
    /// [`crate::UiSystem`] does before it calls a widget's own handler.
    ///
    /// **This is why every operation below has a `&mut Menu` form.** A free function
    /// that reads the menu back out of `ui` cannot be called *from inside* the menu's own
    /// `listen_to_element_message`: the slot is empty for the duration of that call, so
    /// `list_box_handle` answers `None` and the whole body silently does nothing — the menu
    /// receives the selection and raises no `MENU_CHOSEN` — and it fails **silently** unless a
    /// test asserts that the receiving side actually observes the notice.
    fn with_taken<T>(
        ui: &mut UiSystem,
        me: ElemHandle,
        f: impl FnOnce(&mut Menu, &mut UiSystem) -> T,
    ) -> Option<T> {
        let mut b = ui.take_behaviour(me)?;
        let out = b
            .as_any_mut()
            .and_then(|a| a.downcast_mut::<Menu>())
            .map(|m| f(m, ui));
        ui.put_behaviour(me, b);
        out
    }

    fn with_list_mut<T>(
        ui: &mut UiSystem,
        list: ElemHandle,
        f: impl FnOnce(&mut super::listbox::ListBox, &mut UiSystem) -> T,
    ) -> Option<T> {
        let mut b = ui.take_behaviour(list)?;
        let out = b
            .as_any_mut()
            .and_then(|a| a.downcast_mut::<super::listbox::ListBox>())
            .map(|l| f(l, ui));
        ui.put_behaviour(list, b);
        out
    }

    /// The menu's popup construction — build the drop-down's popup out of the layout
    /// attribute [`ATTR_POPUP_LAYOUT`] names.
    ///
    /// An existing popup is queued for deletion. The client resolves layout attribute 7 with the
    /// menu's layout as fallback, reads root id attribute 6, and creates a root. A created popup
    /// starts hidden with activatable `0x33` and activate-on-show `0x34` set, is stored on the menu,
    /// and registers the menu for its element messages.
    ///
    /// The visibility write is `false`, and the two bool attributes `0x33` and `0x34` are both
    /// written `true`. \[verified\]
    ///
    /// **The popup is a root element, not a child of the menu.** That is what lets a drop-down
    /// draw over the window below it, and it is why the menu has to *register* for the popup's
    /// element messages: nothing would otherwise bubble from the list box back to the menu.
    ///
    /// Returns the popup, or `None` when the layout or the element id is missing — the client's
    /// two silent returns.
    pub fn make_popup(
        ui: &mut UiSystem,
        assets: &dyn dereth_primitives::AssetSource,
        me: ElemHandle,
    ) -> Option<ElemHandle> {
        if let Some(old) = with_menu_mut(ui, me, |m| m.popup.take()).flatten() {
            ui.add_to_delete_queue(old);
        }
        let own_layout = ui.node(me)?.layout_did;
        let (declared, root_id) = with_menu(ui, me, |m| (m.popup_layout, m.popup_id))?;
        let root_id = root_id?;
        let popup = declared
            .and_then(|did| ui.create_root_by_data_id(assets, did, root_id).ok())
            .or_else(|| ui.create_root_by_data_id(assets, own_layout, root_id).ok())?;
        ui.initialize_tree(popup);
        ui.set_visible(popup, false);
        ui.set_attribute_bool(popup, crate::props::attr::ACTIVATABLE, true);
        ui.set_attribute_bool(popup, crate::props::attr::ACTIVATE_ON_SHOW, true);
        with_menu_mut(ui, me, |m| m.popup = Some(popup));
        ui.register_for_element_messages(popup, crate::ListenerId::Element(me));
        Some(popup)
    }

    /// The client's popup-dependent half, run after [`make_popup`].
    ///
    /// After creating the popup, the client reads list id attribute 2 and finds that list beneath
    /// the popup. If available, selection attribute 1 seeds the selected row. It initializes the
    /// menu and stores the popup-minus-list width and height as borders.
    ///
    /// It is split out because `Element::initialize` in this crate has no asset source and cannot
    /// call [`make_popup`] (see `env::create_and_add_root_element` for why `UiSystem` holds none).
    /// The host calls [`make_popup`] and then this; everything after is the client's order.
    pub fn initialize_popup(ui: &mut UiSystem, me: ElemHandle) -> Option<ElemHandle> {
        let popup = popup_handle(ui, me)?;
        let list_id = with_menu(ui, me, |m| m.list_box_id)??;
        let list = ui.get_child_recursive(popup, list_id).filter(|h| {
            ui.node(*h)
                .is_some_and(|n| n.ty() == crate::factory::ty::LISTBOX)
        });
        with_menu_mut(ui, me, |m| m.list_box = list);
        let list = list?;
        let pb = ui.screen_box(popup);
        let lb = ui.screen_box(list);
        with_menu_mut(ui, me, |m| {
            m.border = (pb.width() - lb.width(), pb.height() - lb.height());
        });
        Some(list)
    }

    /// The menu's insert-text-item — create one row and put it in the list at
    /// `index`.
    ///
    /// It reads row element id attribute 9 and resolves layout attribute 10 with the menu's layout
    /// as fallback. It creates that child, requires a text element, sets the caption, and inserts it
    /// at `index`; any failed step deletes or rejects the row and returns nothing.
    ///
    /// The list's own insert is the two lines that make a row usable:
    /// make the row mouse-visible, and then
    /// the list box's insert. Without the first the row is drawn and cannot
    /// be clicked. \[verified\]
    ///
    /// The caption is taken as an already-resolved string rather than as a `StringInfo`, because
    /// every caller in this build resolves through the host's string service before it gets here
    /// (see `dereth_ui_screens::chat::mainchat::label`).
    pub fn insert_text_item(
        ui: &mut UiSystem,
        assets: &dyn dereth_primitives::AssetSource,
        me: ElemHandle,
        text: &str,
        index: usize,
    ) -> Option<ElemHandle> {
        let list = list_box_handle(ui, me)?;
        let own_layout = ui.node(me)?.layout_did;
        let (declared, item_id) = with_menu(ui, me, |m| (m.item_layout, m.item_id))?;
        let item_id = item_id?;
        let item = declared
            .and_then(|did| ui.create_child_by_data_id(assets, list, did, item_id).ok())
            .or_else(|| {
                ui.create_child_by_data_id(assets, list, own_layout, item_id)
                    .ok()
            })?;
        ui.initialize_tree(item);
        if ui.text_element_mut(item).is_none() {
            // The type-`0x0C` check failed: the description named something that is not a
            // `TextElement`, and the client destroys the element again.
            ui.remove_and_delete_root(item);
            return None;
        }
        if let Some(t) = ui.text_element_mut(item) {
            t.set_text(text);
        }
        // The menu's insert, which makes the row mouse-visible first.
        ui.set_mouse_visible(item, true);
        let ok =
            with_list_mut(ui, list, |l, ui| l.insert_item(ui, list, item, index)).unwrap_or(false);
        if !ok {
            ui.remove_and_delete_root(item);
            return None;
        }
        // The client sets the layout dirty bit `0x200`, and its own layout pass consumes it on the
        // next frame. This crate has no layout pass
        // (see the layout-pass note), so this method makes the two calls that pass would make:
        // it places the rows and grows the popup to hold them. Doing it per
        // insert rather than per batch is the same answer and keeps the caller free of it.
        layout_items(ui, me);
        Some(item)
    }

    /// The layout update's tail for a menu whose rows changed: place the rows, tell the
    /// scrollable how big its paper now is, and grow the popup to match. Returns
    /// `(cols, rows)`.
    ///
    /// The three steps are one function in the client — the layout update ends in the
    /// scrollable-area resize, whose `0x32` broadcast reaches
    /// the popup-size recalculation through the menu's element-message listener. **That
    /// broadcast cannot do the work here**: it arrives while the list's behaviour is out of its
    /// arena slot, so the menu's `0x32` arm reads a paper size of `(0, 0)` and shrinks the popup
    /// to nothing — a popup box of `(5,576)-(4,577)`, zero wide, with
    /// fourteen correctly placed rows inside it. So the resize is driven from here, where both
    /// objects are in their slots, and the `0x32` arm is kept because it is the client's own.
    pub fn layout_items(ui: &mut UiSystem, me: ElemHandle) -> (i32, i32) {
        let Some(list) = list_box_handle(ui, me) else {
            return (0, 0);
        };
        let grid = with_list_mut(ui, list, |l, ui| {
            let g = l.update_layout(ui, list);
            l.refresh_scroll(ui, list);
            g
        })
        .unwrap_or((0, 0));
        recalculate_popup_size(ui, me);
        grid
    }

    /// Behavior: insert a text item at the list box's item count,
    /// i.e. append.
    ///
    /// The client dereferences the list box and then reads its item count with **no null check
    /// on either**, so a menu with no list box faults; here it answers `None`, which is the same
    /// "no row was made" the caller has to handle anyway.
    ///
    /// This is the client's only row-making call, fourteen times.
    pub fn add_text_item(
        ui: &mut UiSystem,
        assets: &dyn dereth_primitives::AssetSource,
        me: ElemHandle,
        text: &str,
    ) -> Option<ElemHandle> {
        let n = num_items(ui, me);
        insert_text_item(ui, assets, me, text, n)
    }

    /// Behavior: flush the list box, or nothing when there is none.
    pub fn flush(ui: &mut UiSystem, me: ElemHandle) {
        let Some(list) = list_box_handle(ui, me) else {
            return;
        };
        with_list_mut(ui, list, |l, ui| l.flush(ui));
    }

    /// Behavior: the list box's item count, **0** with no
    /// list box.
    #[must_use]
    pub fn num_items(ui: &UiSystem, me: ElemHandle) -> usize {
        let Some(list) = list_box_handle(ui, me) else {
            return 0;
        };
        ui.node(list)
            .and_then(|n| {
                n.behaviour
                    .as_ref()?
                    .as_any()?
                    .downcast_ref::<super::listbox::ListBox>()
            })
            .map_or(0, |l| l.items.len())
    }

    /// The menu's item accessor — the list box's `get_item(i)`.
    #[must_use]
    pub fn get_item(ui: &UiSystem, me: ElemHandle, index: usize) -> Option<ElemHandle> {
        let list = list_box_handle(ui, me)?;
        ui.node(list)
            .and_then(|n| {
                n.behaviour
                    .as_ref()?
                    .as_any()?
                    .downcast_ref::<super::listbox::ListBox>()
            })
            .and_then(|l| l.get_item(index))
    }

    /// The menu's selected-item setter, whole:
    ///
    /// ```text
    /// if list_box: list_box.set_selected_item(item, broadcast)
    ///              new_selection(broadcast)
    /// ```
    ///
    /// `None` clears the selection, and the main chat window's
    /// message-7 arm calls it exactly that way — with no item and `false` — so that
    /// the menu does not keep a highlight on the row the player just chose. The trailing
    /// new-selection step is what puts the chosen row's text on the menu's own face, and it is the
    /// **only** caller of the free [`new_selection`]: the client's other one is
    /// the element-message listener's message-4 case, which cannot use it (see `with_taken`).
    pub fn set_selected_item(
        ui: &mut UiSystem,
        me: ElemHandle,
        item: Option<ElemHandle>,
        broadcast: bool,
    ) {
        let Some(list) = list_box_handle(ui, me) else {
            return;
        };
        // **`broadcast` is passed through.** The client's own body passes
        // the one flag to both the list box's selected-item setter and the new-selection step.
        // Dropping it here would make the main chat window's clearing call (no item, `false`)
        // still raise the list's own `4`.
        with_list_mut(ui, list, |l, ui| {
            l.set_selected_item(ui, list, item, broadcast)
        });
        new_selection(ui, me, broadcast);
    }

    /// Open the menu.
    ///
    /// ```text
    /// if open or no list box or the list box has no items or no popup: return
    /// above = bool attribute 5;  centred = bool attribute 3
    /// x = screen.x0
    /// if centred: x -= (popup.width - width) / 2
    /// y = above ? screen.y0 - popup.height : screen.y1
    /// move the popup to (x, y) in screen space
    /// make the popup visible
    /// open = true; bool attribute 0x0E = true; broadcast message 8 with (0, 0)
    /// ```
    ///
    /// **An empty item list is a hard return**: a talk-focus menu with no rows does not open, so
    /// clicking the Chat tab does nothing at all rather than opening an empty box. Returns whether
    /// it opened.
    pub fn open(ui: &mut UiSystem, me: ElemHandle) -> bool {
        with_taken(ui, me, |m, ui| m.open_now(ui, me)).unwrap_or(false)
    }

    /// Behavior: hide the popup, clear the open flag and `0x0E`, broadcast
    /// **9**. Returns whether it was open.
    pub fn close(ui: &mut UiSystem, me: ElemHandle) -> bool {
        with_taken(ui, me, |m, ui| m.close_now(ui, me)).unwrap_or(false)
    }

    /// Behavior: copy the chosen row's text onto the menu's own
    /// face and broadcast **7**.
    ///
    /// With a list present, it reads text-child attribute 8 and copies the selected row's text to
    /// that child, or clears it when nothing is selected. When `broadcast` is true it sends message
    /// 7 with the selected row's element id and handle, or two zeroes for no selection.
    ///
    /// `p2` is the **element**, which is the only thing that distinguishes one row from another:
    /// every row of one menu is created from the same description, so `p1` is the same id for all
    /// of them. The client reads `p2` for exactly
    /// that reason.
    pub fn new_selection(ui: &mut UiSystem, me: ElemHandle, broadcast: bool) {
        with_taken(ui, me, |m, ui| m.new_selection_now(ui, me, broadcast));
    }

    /// Behavior: grow the popup to the rows it holds.
    ///
    /// With both list and popup present, it starts from the popup size. A list pinned at both
    /// horizontal edges replaces width with scrollable width plus the saved border; a list pinned
    /// at both vertical edges does the same for height. The popup is resized to that result.
    ///
    /// The four `== 1` comparisons read the list's left, top, right, and bottom anchoring modes: the
    /// popup only follows the list on an axis where the list is pinned at **both** ends and
    /// therefore cannot stretch on its own. The measured scrollable width and height supply the
    /// replacement dimensions.
    /// \[verified\]
    ///
    /// It is reached from element message `0x32`, the client's own row-change broadcast, so adding
    /// rows resizes the popup
    /// without anyone asking.
    pub fn recalculate_popup_size(ui: &mut UiSystem, me: ElemHandle) {
        with_taken(ui, me, |m, ui| m.recalculate_popup_size_now(ui, me));
    }

    impl Menu {
        /// The list box's item count, off the fields this object already holds.
        fn items_len(&self, ui: &UiSystem) -> usize {
            let Some(list) = self.list_box else { return 0 };
            ui.node(list)
                .and_then(|n| {
                    n.behaviour
                        .as_ref()?
                        .as_any()?
                        .downcast_ref::<super::listbox::ListBox>()
                })
                .map_or(0, |l| l.items.len())
        }

        /// The body of [`open`], callable while this `Menu` is out of its slot.
        pub fn open_now(&mut self, ui: &mut UiSystem, me: ElemHandle) -> bool {
            let Some(popup) = self.popup.filter(|p| ui.node(*p).is_some()) else {
                return false;
            };
            if self.open || self.list_box.is_none() || self.items_len(ui) == 0 {
                return false;
            }
            let props = ui.node(me).map(crate::ElementNode::merged_properties);
            let above = props
                .as_ref()
                .and_then(|p| p.get_bool(ATTR_POPUP_ABOVE))
                .unwrap_or(false);
            let centred = props
                .as_ref()
                .and_then(|p| p.get_bool(ATTR_POPUP_CENTRED))
                .unwrap_or(false);
            let mine = ui.screen_box(me);
            let pb = ui.screen_box(popup);
            let mut x = mine.x0;
            if centred {
                x -= (pb.width() - mine.width()) / 2;
            }
            let y = if above {
                mine.y0 - pb.height()
            } else {
                mine.y1 + 1
            };
            ui.move_to(popup, x, y);
            ui.set_visible(popup, true);
            self.open = true;
            ui.set_attribute_bool(me, ATTR_OPEN, true);
            ui.broadcast_element_message(me, msgid::MENU_OPENED, 0, 0);
            true
        }

        /// The body of [`close`], callable while this `Menu` is out of its slot.
        pub fn close_now(&mut self, ui: &mut UiSystem, me: ElemHandle) -> bool {
            if !self.open {
                return false;
            }
            let Some(popup) = self.popup.filter(|p| ui.node(*p).is_some()) else {
                return false;
            };
            ui.set_visible(popup, false);
            self.open = false;
            ui.set_attribute_bool(me, ATTR_OPEN, false);
            ui.broadcast_element_message(me, msgid::MENU_CLOSED, 0, 0);
            true
        }

        /// The list box's selected item, read out of the list's behaviour.
        ///
        /// **Answers `None` while the list box is itself mid-dispatch**, because its behaviour is
        /// then out of its arena slot — which is exactly when message 4 arrives. That is what
        /// [`Menu::new_selection_with`]'s hint is for; getting it wrong produces a
        /// `MENU_CHOSEN` with `p2 = 0`: a message that says "a row was chosen" and does not
        /// say which.
        fn selected_item(&self, ui: &UiSystem) -> Option<ElemHandle> {
            let list = self.list_box?;
            ui.node(list)
                .and_then(|n| {
                    n.behaviour
                        .as_ref()?
                        .as_any()?
                        .downcast_ref::<super::listbox::ListBox>()
                })
                .and_then(|l| l.selected.and_then(|i| l.items.get(i).copied()))
        }

        /// The body of [`new_selection`], callable while this `Menu` is out of its slot — which is
        /// where the client calls it from: the element-message listener's message-4 case.
        pub fn new_selection_now(&mut self, ui: &mut UiSystem, me: ElemHandle, broadcast: bool) {
            let selected = self.selected_item(ui);
            self.new_selection_with(ui, me, selected, broadcast);
        }

        /// [`Menu::new_selection_now`] with the selected item supplied rather than read.
        ///
        /// Message 4 carries it in `p2` (`ListBox`'s own broadcast: `p1 = index`,
        /// `p2 = the item`), which is the only place it can be read from while the list is
        /// dispatching.
        pub fn new_selection_with(
            &mut self,
            ui: &mut UiSystem,
            me: ElemHandle,
            selected: Option<ElemHandle>,
            broadcast: bool,
        ) {
            if self.list_box.is_none() {
                return;
            }
            if let Some(text_id) = self.selection_text_id {
                if let Some(face) = ui.get_child_recursive(me, text_id) {
                    // The pre-parsed text is the tagged form, so a row carrying a link keeps it.
                    let chosen: Option<String> = selected
                        .and_then(|h| ui.text_element_mut(h).map(|t| t.glyphs.inq_text(true)));
                    if let Some(t) = ui.text_element_mut(face) {
                        // Clear all text when there is no selection — the client's else arm.
                        t.set_text(chosen.as_deref().unwrap_or(""));
                    }
                }
            }
            if !broadcast {
                return;
            }
            match selected {
                Some(h) => {
                    let id = ui.node(h).map_or(0, |n| n.element_id().0);
                    ui.broadcast_element_message(me, msgid::MENU_CHOSEN, id, h.raw());
                }
                None => ui.broadcast_element_message(me, msgid::MENU_CHOSEN, 0, 0),
            }
        }

        /// The body of [`recalculate_popup_size`], callable while this `Menu` is out of its slot.
        pub fn recalculate_popup_size_now(&mut self, ui: &mut UiSystem, me: ElemHandle) {
            let _ = me;
            let Some(popup) = self.popup.filter(|p| ui.node(*p).is_some()) else {
                return;
            };
            let Some(list) = self.list_box else { return };
            let Some(edges) = ui.node(list).map(|n| n.desc.edges) else {
                return;
            };
            let paper = ui
                .node(list)
                .and_then(|n| {
                    n.behaviour
                        .as_ref()?
                        .as_any()?
                        .downcast_ref::<super::listbox::ListBox>()
                })
                .map(|l| (l.scroll.width, l.scroll.height));
            let Some((pw, ph)) = paper else { return };
            let pb = ui.screen_box(popup);
            let pinned = |a: crate::layout::EdgeMode, b: crate::layout::EdgeMode| {
                a == crate::layout::EdgeMode::AnchorStart
                    && b == crate::layout::EdgeMode::AnchorStart
            };
            let w = if pinned(edges.left, edges.right) {
                pw + self.border.0
            } else {
                pb.width()
            };
            let h = if pinned(edges.top, edges.bottom) {
                ph + self.border.1
            } else {
                pb.height()
            };
            ui.resize_to(popup, w, h);
            // `resize_to` cascades `update_for_parent_size_change` through the popup tree. A menu
            // row is cloned from one layout description, so an anchored row template can put every
            // clone back at that description's Y during the cascade. Retail does not make this
            // second call directly in the popup-size recalculation: the insert set dirty bit 0x200
            // and a later layout consumption (also explicit in the item-index-at-point query)
            // re-places the rows. This crate has no global layout pass (see `layout_items`), so
            // consume the equivalent Rust scheduling point here. The paper dimensions are already
            // current, making the nested 0x32 path a no-op while this Menu is lifted.
            with_list_mut(ui, list, |l, ui| {
                l.update_layout(ui, list);
                l.refresh_scroll(ui, list);
            });
        }
    }

    /// The menu's item accessor followed by its selected-item setter — the pair
    /// the menu dialog's `set_data` runs for property `0xA4` and
    /// the confirmation-menu dialog's runs for `0xAB`.
    ///
    /// **The dialog's *initial* selection and its answer are the same property key.** The item
    /// accessor answers none for an index past the end and selecting none clears the selection,
    /// so an out-of-range seed leaves the selected index at -1 rather than at the old row; a menu
    /// with no list box ignores the call entirely.
    pub fn set_selected_index(ui: &mut UiSystem, me: ElemHandle, index: i32) {
        let Some(list_h) = list_box_handle(ui, me) else {
            return;
        };
        let n = ui
            .node(list_h)
            .and_then(|nd| {
                nd.behaviour
                    .as_ref()?
                    .as_any()?
                    .downcast_ref::<super::listbox::ListBox>()
            })
            .map_or(0, |l| l.items.len());
        match usize::try_from(index).ok().filter(|i| *i < n) {
            Some(i) => {
                if let Some(mut b) = ui.take_behaviour(list_h) {
                    if let Some(l) = b
                        .as_any_mut()
                        .and_then(|a| a.downcast_mut::<super::listbox::ListBox>())
                    {
                        l.select(ui, list_h, i);
                    }
                    ui.put_behaviour(list_h, b);
                }
            }
            None => {
                if let Some(l) = ui.node_mut(list_h).and_then(|nd| {
                    nd.behaviour
                        .as_mut()?
                        .as_any_mut()?
                        .downcast_mut::<super::listbox::ListBox>()
                }) {
                    l.selected = None;
                }
            }
        }
    }

    impl Element for Menu {
        fn as_any(&self) -> Option<&dyn std::any::Any> {
            Some(self)
        }

        /// The type-6 downcast, mutably — what [`make_popup`] and [`initialize_popup`] write the
        /// popup and the list box through. Without it both would be silent no-ops.
        fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
            Some(self)
        }

        /// The original mouse-visibility query always returns true for the five mouse-driven
        /// element types: button, menu, dragbar, resizebar, and scrollbar.
        fn should_be_mouse_visible(&self) -> bool {
            true
        }

        /// The original menu shares button, text, and scrolling mouse-down behavior, so pressing a
        /// menu moves the focus element.
        fn takes_focus_on_press(&self) -> bool {
            true
        }

        /// The client's two element-id reads, taken here rather than
        /// in `post_init` for the same reason `ListBox` takes `0x71`/`0x72` here: the
        /// value arrives through initialisation step 4's `on_set_attribute` sweep, one call earlier
        /// than `post_init`, and it is the same value out of the same collection.
        ///
        /// Without these reads a `Menu` could not open and could not report a selection.
        /// The shipped menus carry both keys: layout `0x21000043`'s `0x1000035B` has
        /// `2 = 0x10000360` (the `ListBox` inside the popup) and `6 = 0x1000035F` (the
        /// popup root), with `7 = 0x21000043` naming its own layout.
        ///
        /// Popup creation itself — creating that popup root out of the layout attribute 7
        /// names — is **not** done here: it needs an asset source and this crate's element manager
        /// has none. It is the free function [`make_popup`], which the host
        /// calls with the asset source it already holds, followed by [`initialize_popup`]. These
        /// six ids are what both of them read.
        fn on_set_attribute(
            &mut self,
            ctx: &mut ElemCtx<'_>,
            id: u32,
            v: Option<&crate::PropertyValue>,
        ) {
            let element = match v {
                Some(crate::PropertyValue::Enum(e)) => Some(ElementId(*e)),
                Some(crate::PropertyValue::Integer(i)) => u32::try_from(*i).ok().map(ElementId),
                _ => None,
            };
            let did = match v {
                Some(crate::PropertyValue::DataFile(d)) => Some(*d),
                _ => None,
            };
            match id {
                ATTR_LIST_BOX => self.list_box_id = element,
                ATTR_POPUP_ROOT => self.popup_id = element,
                ATTR_ITEM_ELEMENT => self.item_id = element,
                ATTR_SELECTION_TEXT => self.selection_text_id = element,
                ATTR_POPUP_LAYOUT => self.popup_layout = did,
                ATTR_ITEM_LAYOUT => self.item_layout = did,
                _ => {}
            }
            self.button.on_set_attribute(ctx, id, v);
        }

        fn as_text_mut(&mut self) -> Option<&mut crate::text::TextElement> {
            self.button.as_text_mut()
        }

        /// The client's tail, in its order:
        ///
        /// ```text
        /// if popup: stop listening to the popup's element messages
        /// if open and popup: hide the popup; open = false
        ///                    bool attribute 0x0E = false
        ///                    broadcast message 9 with (0, 0)
        /// if popup: queue the popup for deletion; popup = none
        /// ```
        ///
        /// The close-shaped middle is spelled out inline in the destructor rather than being a
        /// call to the close, which is why it is written out here too.
        fn on_destroy(&mut self, ctx: &mut ElemCtx<'_>) {
            let Some(popup) = self.popup.take() else {
                return;
            };
            let me = ctx.me;
            ctx.ui
                .unregister_from_element(popup, crate::ListenerId::Element(me));
            if self.open {
                ctx.ui.set_visible(popup, false);
                self.open = false;
                ctx.ui.set_attribute_bool(me, ATTR_OPEN, false);
                ctx.ui
                    .broadcast_element_message(me, msgid::MENU_CLOSED, 0, 0);
            }
            self.list_box = None;
            ctx.ui.add_to_delete_queue(popup);
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

        /// The menu's element-message handler, in the client's own order.
        ///
        /// ```text
        /// from the popup:
        ///     0x2A while open and the pointer is not over the menu: close; return Stop
        /// from the list box:
        ///     4:    new_selection(true); close          // a row was picked
        ///     0x32: recalculate_popup_size; return Stop // the paper grew
        ///     0x43: close                               // row activated
        ///     any other: return Stop
        ///     return Stop
        /// from a type-5 element that is the list box: ...row rollover states...
        /// from the menu itself:
        ///     1:    close if open, else open; return Stop
        ///     0x1B: update the state;             return Stop
        /// otherwise: the button base's handler
        /// ```
        ///
        /// The open/close toggle is message **1**
        /// (`BUTTON_CLICKED`), not `0x19` — a menu is a `Button` and its own click
        /// arrives the way every other button's does — and this arm **chains to the button**,
        /// which is what raises that 1 in the first place. Without the chain a `Menu` would be
        /// the one button in the build that never ran the inherited click path.
        ///
        /// The `0x2A` arm is the client's click-away close: the popup is a root element, so
        /// clicking anywhere else deactivates it and the menu shuts.
        fn listen_to_element_message(&mut self, ctx: &mut ElemCtx<'_>, m: &ElementMessage) -> R {
            // Every call below is the `_now` form: this object is **out of its arena slot** for
            // the duration of this function, so the free functions cannot see it. See
            // [`with_taken`].
            let me = ctx.me;
            if Some(m.source) == self.popup {
                let over_top = ctx
                    .ui
                    .node(me)
                    .is_some_and(|n| n.region.flags.mouse_over_top);
                if m.id == msgid::DEACTIVATED && self.open && !over_top {
                    self.close_now(ctx.ui, me);
                    return R::StopProcessing;
                }
                return R::Default;
            }
            if Some(m.source) == self.list_box {
                match m.id {
                    msgid::LIST_SELECTION_CHANGED => {
                        // `p2` is the selected item; the list cannot be asked for it here.
                        let picked = (m.p2 != 0).then(|| ElemHandle::from_raw(m.p2));
                        self.new_selection_with(ctx.ui, me, picked, true);
                        self.close_now(ctx.ui, me);
                    }
                    msgid::SCROLL_OFFSET => self.recalculate_popup_size_now(ctx.ui, me),
                    msgid::LIST_ITEM_ACTIVATED => {
                        self.close_now(ctx.ui, me);
                    }
                    _ => {}
                }
                return R::StopProcessing;
            }
            if m.source == ctx.me {
                if m.id == msgid::BUTTON_CLICKED {
                    if self.open {
                        self.close_now(ctx.ui, me);
                    } else {
                        self.open_now(ctx.ui, me);
                    }
                    return R::StopProcessing;
                }
                if m.id == msgid::MOUSE_OVER_TOP {
                    // The menu's state update: bool attribute `0x0E` = the open flag.
                    let open = self.open;
                    ctx.ui.set_attribute_bool(me, ATTR_OPEN, open);
                    return R::StopProcessing;
                }
            }
            self.button.listen_to_element_message(ctx, m)
        }
    }
}

/// `Meter` (7) — a progress or vital bar.
///
/// Two modes: continuous (a child image, id **2**, revealed a fraction at a time) and
/// frame meter (a frame index into a strip). Animates between the animation's start and end
/// positions over its start and end times using the shared UI easing table, and
/// broadcasts **0x2D** when the animation starts and **0x2E** when it ends.
///
/// # How a continuous meter actually fills
///
/// The fill is **not** a scaled child image. The
/// child stays in place and the meter's child-draw routine supplies a **narrowed
/// clip box**: for direction `1` the box becomes `x0 ..= x0 + trunc(w * position) - 1`, and the
/// four directions are the four edges the fill grows from. See `Meter::child_clip`.
///
/// The alternative arm is attribute `0x68` `UICore_Meter_move_fill`: when it is set,
/// the child draw draws normally and the child-update routine instead moves the child
/// out of the parent's clip box. **No shipped layout sets it** — every `Meter` in
/// `classic_gameplay` carries `0x68 = false` or nothing at all — so that arm is left unimplemented
/// rather than guessed at; see `Meter::child_clip`'s note.
///
/// The property names come from the retail `MasterProperty 0x39000001` and settle what each
/// attribute is: `0x66 UICore_Meter_goal_position`, `0x67 …_frame_meter`, `0x68 …_move_fill`,
/// `0x69 …_position`, `0x6A …_smooth_movement`, `0x6B …_smooth_movement_duration`,
/// `0x6F …_child_direction`.
pub mod meter {
    use super::*;
    use crate::region::Box2D;

    /// The meter's initialisation finds its child image, element id 2, recursively.
    ///
    /// The lookup is recursive, but the child draw only ever compares it against a **direct**
    /// child, so a meter whose id-2 element were a grandchild would clip nothing. Every shipped
    /// meter has it as a direct child.
    pub const CHILD_IMAGE: crate::ElementId = crate::ElementId(2);

    /// The fill direction, attribute `0x6F` — which edge the fill grows from.
    pub mod direction {
        /// Fill from the left: the child keeps `x0 ..= x0 + w*position - 1`.
        pub const LEFT: u32 = 1;
        /// Fill from the top.
        pub const TOP: u32 = 2;
        /// Fill from the right: `x0` moves in by `w*(1 - position)`.
        pub const RIGHT: u32 = 3;
        /// Fill from the bottom.
        pub const BOTTOM: u32 = 4;
    }

    /// The four meter attributes this widget reads, as named in `MasterProperty`.
    pub mod attr {
        /// `0x67 UICore_Meter_frame_meter`.
        pub const FRAME_METER: u32 = 0x67;
        /// `0x68 UICore_Meter_move_fill`.
        pub const MOVE_FILL: u32 = 0x68;
        /// `0x69 UICore_Meter_position`.
        pub const POSITION: u32 = 0x69;
        /// `0x6F UICore_Meter_child_direction`.
        pub const CHILD_DIRECTION: u32 = 0x6F;
    }

    #[derive(Debug)]
    pub struct Meter {
        /// The displayed position, 0..1. `position` is the attribute itself (`0x69`); the client
        /// re-reads it as a float attribute on every draw, and this mirror is written by
        /// [`Meter::on_set_attribute`] so the draw path needs no property lookup.
        pub position: f32,
        pub anim_start_pos: f32,
        pub anim_end_pos: f32,
        pub anim_start_time: f64,
        pub anim_end_time: f64,
        pub animating: bool,
        /// Frame-meter mode — index a strip instead of revealing a child.
        pub frame_meter: bool,
        /// Attribute `0x68` `UICore_Meter_move_fill`.
        pub move_fill: bool,
        pub current_frame: i32,
        pub frame_count: i32,
        /// The fill direction. The constructor sets **1**, and attribute `0x6F` has no
        /// default in the retail `MasterProperty`, so a meter that declares none fills leftwards —
        /// which is what the three vitals bars do.
        pub direction: u32,
    }

    impl Default for Meter {
        /// Behavior: the members it sets by hand.
        fn default() -> Self {
            Self {
                position: 0.0,
                anim_start_pos: 0.0,
                anim_end_pos: 0.0,
                anim_start_time: -1.0,
                anim_end_time: -1.0,
                animating: false,
                frame_meter: false,
                move_fill: false,
                current_frame: -1,
                frame_count: 0,
                direction: direction::LEFT,
            }
        }
    }

    pub fn create(_l: &crate::LayoutDesc, _d: &crate::ElementDesc) -> Box<dyn Element> {
        Box::new(Meter::default())
    }

    impl Meter {
        /// Start an animation to `to`, registering for the frame tick for exactly as long as it
        /// runs — the register-only-while-working idiom.
        pub fn animate_to(
            &mut self,
            ui: &mut UiSystem,
            me: ElemHandle,
            to: f32,
            now: f64,
            secs: f64,
        ) {
            self.anim_start_pos = self.position;
            self.anim_end_pos = to;
            self.anim_start_time = now;
            self.anim_end_time = now + secs;
            self.animating = true;
            ui.want_tick(me, true);
            ui.broadcast_element_message(me, msgid::METER_ANIM_START, 0, 0);
        }

        /// The eased position at `now`, using the UI animation's 0..1024 curve.
        #[must_use]
        pub fn eased(&self, table: &[i16; 100], now: f64) -> f32 {
            let span = self.anim_end_time - self.anim_start_time;
            #[allow(clippy::cast_possible_truncation)] // the original's curve input is a float
            let t = if span <= 0.0 {
                1.0_f32
            } else {
                ((now - self.anim_start_time) / span) as f32
            };
            let level = f32::from(crate::media::anim_level(table, t)) / 1024.0;
            self.anim_start_pos + (self.anim_end_pos - self.anim_start_pos) * level
        }
    }

    impl Meter {
        /// The clip box the meter's child draw gives the child image.
        ///
        /// The client's own switch, with `w = box.width()` and `h = box.height()` (inclusive, so
        /// `x1 - x0 + 1`), `l` = float attribute `0x69`, and `trunc` truncating toward zero:
        ///
        /// | direction | what it writes |
        /// |---|---|
        /// | 1 | `x1 = x0 + trunc(w·l) - 1` |
        /// | 2 | `y1 = y0 + trunc(h·l) - 1` |
        /// | 3 | `x0 += trunc(w·(1-l))`, then `x1 = x0 + trunc(w·l) - 1` |
        /// | 4 | `y0 += trunc(h·(1-l))`, then `y1 = y0 + trunc(h·l) - 1` |
        /// | other | the box is left alone |
        ///
        /// and the result is then intersected with the clip the child already had — so
        /// a level above 1.0 cannot make the fill bigger than its own graphic. That matters: a
        /// server can hand out `cur > max` (a recorded session reaches mana 111 against a computed
        /// maximum of 100), and the client does not clamp `cur/max` either.
        ///
        /// Returns `None` — no narrowing — for a frame meter (which has no child image at all),
        /// for any child that is not id 2, and for a `move_fill` meter, whose child-update
        /// arm moves the child instead. **`move_fill` is not implemented**: the exact arithmetic
        /// of that arm is not known, no shipped layout
        /// sets the attribute, and a guessed formula would be worse than an honest gap.
        /// `// UNVERIFIED:` the `move_fill` arithmetic.
        #[must_use]
        pub fn child_clip(&self, child: crate::ElementId, box_: Box2D) -> Option<Box2D> {
            if self.frame_meter || self.move_fill || child != CHILD_IMAGE || !box_.is_valid() {
                return None;
            }
            #[allow(clippy::cast_precision_loss)]
            // the client's own operands are `long`s in an FMUL
            let (w, h) = (box_.width() as f32, box_.height() as f32);
            let l = self.position;
            let mut b = box_;
            match self.direction {
                direction::LEFT => b.x1 = b.x0 + dereth_primitives::num::to_i32(w * l) - 1,
                direction::TOP => b.y1 = b.y0 + dereth_primitives::num::to_i32(h * l) - 1,
                direction::RIGHT => {
                    b.x0 += dereth_primitives::num::to_i32(w * (1.0 - l));
                    b.x1 = b.x0 + dereth_primitives::num::to_i32(w * l) - 1;
                }
                direction::BOTTOM => {
                    b.y0 += dereth_primitives::num::to_i32(h * (1.0 - l));
                    b.y1 = b.y0 + dereth_primitives::num::to_i32(h * l) - 1;
                }
                _ => return None,
            }
            Some(b.intersect(&box_))
        }
    }

    impl Element for Meter {
        fn listen_to_global_message(&mut self, ctx: &mut ElemCtx<'_>, id: MessageId, _p: u32) {
            if id != crate::msg::global::TICK || !self.animating {
                return;
            }
            // The tick carries no time; the host advances the meter through `advance`.
            if self.anim_end_time <= self.anim_start_time {
                self.position = self.anim_end_pos;
                self.animating = false;
                ctx.ui.want_tick(ctx.me, false);
                ctx.ui
                    .broadcast_element_message(ctx.me, msgid::METER_ANIM_END, 0, 0);
            }
        }

        /// The meter's attribute setter for `0x69`, plus the three members
        /// its initialisation reads once.
        ///
        /// Initialisation reads bool attribute `0x67` and enum attribute `0x6F` **once**, and
        /// `UiSystem::initialize` step 4 replays every merged property through here, which is the
        /// same read at the same moment. A property that later *disappears* (`v == None`) is
        /// ignored for those three, because the client caches them and never looks again.
        fn on_set_attribute(
            &mut self,
            _c: &mut ElemCtx<'_>,
            id: u32,
            v: Option<&crate::PropertyValue>,
        ) {
            match (id, v) {
                (attr::POSITION, Some(crate::PropertyValue::Float(f))) => self.position = *f,
                (attr::FRAME_METER, Some(crate::PropertyValue::Bool(b))) => self.frame_meter = *b,
                (attr::MOVE_FILL, Some(crate::PropertyValue::Bool(b))) => self.move_fill = *b,
                (attr::CHILD_DIRECTION, Some(crate::PropertyValue::Enum(e))) => self.direction = *e,
                _ => {}
            }
        }

        fn child_clip(&self, child: crate::ElementId, child_screen: Box2D) -> Option<Box2D> {
            Self::child_clip(self, child, child_screen)
        }
    }

    impl Meter {
        /// Advance to `now`; ends the animation and unregisters when it is done.
        pub fn advance(&mut self, ui: &mut UiSystem, me: ElemHandle, now: f64) {
            if !self.animating {
                return;
            }
            let table = ui.easing;
            self.position = self.eased(&table, now);
            if now >= self.anim_end_time {
                self.position = self.anim_end_pos;
                self.animating = false;
                ui.want_tick(me, false);
                ui.broadcast_element_message(me, msgid::METER_ANIM_END, 0, 0);
            }
        }
    }
}

/// `Scrollbar` (0x0B) — the track, the thumb and the two arrows.
///
/// **The position is a float, not an integer.** An integer `position` clamped into `0..=range`
/// with nothing writing `range` would clamp every position to 0, element message
/// `0x0A` could never carry a drag, and the stack splitter's slider and the char-gen attribute
/// sliders would be inert. The client has no such integer: **its position is the float attribute
/// `0x86`, in `0.0 ..= 1.0`**, and every one of the position setter,
/// the point-to-position, the position-to-thumb-origin, the stop-to-position
/// and the layout update is arithmetic over that one float. `Toolbar` writes it back
/// as float attribute `0x86` on the slider, `split size / max split size`.
///
/// # The messages it raises
///
/// | id | raised by | `p1` | `p2` |
/// |---|---|---|---|
/// | `0x0A` | the position setter / the stop setter | `(long)(position · 1000)` | the stop, or `-1` |
/// | `0x0D` / `0x0E` | an **arrow** with no stop locations (the move-steps handler) | `±1` | 0 |
/// | `0x0F` / `0x10` | a click on the **track** either side of the thumb (the page-click handler) | 0 | 0 |
///
/// **`p1` of `0x0A` is `position · 1000`, and that is established rather than invented.** The
/// barber panel's `0x10000321` arm is the consumer that confirms it — it takes `p1` as unsigned,
/// multiplies by `0.001`, clamps to `0 ..= 1` and sets the shade from it.
/// A thousandth of the track is the resolution the client ships.
///
/// **The two `0x0D`/`0x0E` and `0x0F`/`0x10` pairs are the other way round from the names in
/// [`crate::msg::element::id`]**, whose names are kept as labels. The ids here are the
/// client's: an arrow raises `0x0D`/`0x0E` (the ids named *page*) and a click on the empty track
/// raises `0x0F`/`0x10` (the ids named *line*). Recorded rather than renamed: the ids come from
/// the client's broadcasts, and the names are only labels.
pub mod scrollbar {
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
        /// mapping that returns every stop-to-position of `k` to `k`. `// UNVERIFIED:` the
        /// rounding.
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
        fn button_handle(
            &self,
            ui: &UiSystem,
            me: ElemHandle,
            increment: bool,
        ) -> Option<ElemHandle> {
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
        pub fn position_to_widget_x0y0(
            &self,
            ui: &UiSystem,
            me: ElemHandle,
            pos: f32,
        ) -> (i32, i32) {
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
            let p1 = dereth_primitives::num::to_i32_f64(f64::from(pos) * f64::from(POSITION_SCALE))
                as u32;
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
            let b = |v: Option<&crate::PropertyValue>| {
                matches!(v, Some(crate::PropertyValue::Bool(true)))
            };
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
}
/// `ColorPicker` (0x10) — a palette swatch grid. Broadcasts **0x30** with the chosen
/// colour.
pub mod colorpicker {
    use super::*;

    #[derive(Debug, Default)]
    pub struct ColorPicker {
        /// The currently selected colour.
        pub selected_rgba: u32,
        /// The currently selected swatch index.
        pub selection: i32,
        /// Whether the selection is displayed.
        pub display_selection: bool,
        pub swatches: Vec<u32>,
        pub columns: i32,
    }

    pub fn create(_l: &crate::LayoutDesc, _d: &crate::ElementDesc) -> Box<dyn Element> {
        Box::new(ColorPicker::default())
    }

    impl ColorPicker {
        pub fn choose(&mut self, ui: &mut UiSystem, me: ElemHandle, index: usize) {
            let Some(rgba) = self.swatches.get(index).copied() else {
                return;
            };
            self.selection = i32::try_from(index).unwrap_or(0);
            self.selected_rgba = rgba;
            ui.broadcast_element_message(me, msgid::COLOR_CHOSEN, rgba, 0);
        }
    }

    impl Element for ColorPicker {
        fn listen_to_element_message(&mut self, ctx: &mut ElemCtx<'_>, m: &ElementMessage) -> R {
            if m.source == ctx.me && m.id == msgid::MOUSE_CLICK && !self.swatches.is_empty() {
                let (ox, _) = (m.point.element.0, m.point.element.1);
                if self.columns > 0 {
                    let i = usize::try_from(ox / self.columns.max(1)).unwrap_or(0);
                    let me = ctx.me;
                    self.choose(ctx.ui, me, i.min(self.swatches.len() - 1));
                }
                return R::DontDoDefault;
            }
            R::Default
        }
    }
}

#[cfg(test)]
mod meter_tests {
    use super::meter::{direction, Meter, CHILD_IMAGE};
    use crate::region::Box2D;
    use crate::ElementId;

    /// The meter fill uses the client's four-arm direction switch, evaluated on the box
    /// the retail `classic_gameplay` gives the health bar's fill child `0x00000002` — 150 x 16,
    /// read off the live tree decoded from `client_local_English.dat`.
    ///
    /// The proportion is the whole point: a meter at 22/30 must show 22/30 of its width, not "not
    /// zero". `fixtures/packet-captures/first-login-walk-jump.jsonl` is where 22/30 comes from — the stamina the
    /// recorded ACE server sent after the player jumped.
    #[test]
    fn the_fill_is_clipped_to_its_own_fraction_of_the_box() {
        let box_ = Box2D::new(325, 5, 474, 20); // 150 wide, 16 high
        let clip = |l: f32| {
            let m = Meter {
                position: l,
                ..Meter::default()
            };
            m.child_clip(CHILD_IMAGE, box_)
        };
        // Direction 1 is the constructor's default and what the three vitals bars use.
        assert_eq!(
            clip(1.0),
            Some(box_),
            "a full meter shows the whole graphic"
        );
        assert_eq!(clip(0.5).unwrap().width(), 75);
        assert_eq!(
            clip(0.5).unwrap().x0,
            box_.x0,
            "it grows from the left edge"
        );
        // 22/30: the stamina in first-login-walk-jump after the jump.
        assert_eq!(clip(22.0 / 30.0).unwrap().width(), 110);
        assert_eq!(clip(28.0 / 30.0).unwrap().width(), 140);
        // Empty is *invalid*, which is how `draw_region` drops the fill entirely.
        assert!(!clip(0.0).unwrap().is_valid());
        // `cur > max` happens on a live ACE server (early-inventory-and-casting reaches mana
        // 111/100) and neither the client nor this clamps the ratio; the intersection with the
        // child's own box does.
        assert_eq!(clip(1.11), Some(box_));
    }

    /// Oracle: the same switch's other three arms.
    #[test]
    fn each_direction_grows_the_fill_from_its_own_edge() {
        let box_ = Box2D::new(0, 0, 99, 39); // 100 x 40
        let at = |d: u32, l: f32| {
            Meter {
                position: l,
                direction: d,
                ..Meter::default()
            }
            .child_clip(CHILD_IMAGE, box_)
        };
        assert_eq!(at(direction::LEFT, 0.25), Some(Box2D::new(0, 0, 24, 39)));
        assert_eq!(at(direction::TOP, 0.25), Some(Box2D::new(0, 0, 99, 9)));
        assert_eq!(at(direction::RIGHT, 0.25), Some(Box2D::new(75, 0, 99, 39)));
        assert_eq!(at(direction::BOTTOM, 0.25), Some(Box2D::new(0, 30, 99, 39)));
        // …and the bottom arm's y0 is `y0 + h*(1-l)`, i.e. 30, with the full width kept.
        let b = at(direction::BOTTOM, 0.25).unwrap();
        assert_eq!((b.x0, b.x1, b.y0, b.y1), (0, 99, 30, 39));
        // An unrecognised direction narrows nothing, as the switch's default does.
        assert_eq!(at(0, 0.25), None);
        assert_eq!(at(5, 0.25), None);
    }

    /// Oracle: the child draw's guard — the child image is the id-2 descendant, the `0x68`
    /// arm draws the child unclipped, and a frame meter has no child image at all.
    #[test]
    fn only_the_id_2_child_of_a_clipping_meter_is_narrowed() {
        let box_ = Box2D::new(0, 0, 99, 15);
        let m = Meter {
            position: 0.5,
            ..Meter::default()
        };
        assert!(m.child_clip(CHILD_IMAGE, box_).is_some());
        assert_eq!(
            m.child_clip(ElementId(0x1000_00E7), box_),
            None,
            "the trough is never clipped"
        );
        assert_eq!(
            m.child_clip(ElementId(0x1000_00EB), box_),
            None,
            "nor the label"
        );
        let moved = Meter {
            position: 0.5,
            move_fill: true,
            ..Meter::default()
        };
        assert_eq!(
            moved.child_clip(CHILD_IMAGE, box_),
            None,
            "0x68 moves the child instead"
        );
        let framed = Meter {
            position: 0.5,
            frame_meter: true,
            ..Meter::default()
        };
        assert_eq!(
            framed.child_clip(CHILD_IMAGE, box_),
            None,
            "a frame meter has no child image"
        );
    }

    /// Oracle: the meter's constructor and its member initialisers.
    #[test]
    fn the_constructed_meter_is_the_clients_constructed_meter() {
        let m = Meter::default();
        assert_eq!(m.direction, direction::LEFT, "the direction starts at 1");
        assert_eq!(m.current_frame, -1);
        assert!(!m.frame_meter && !m.animating);
        assert_eq!(
            m.anim_start_time, -1.0,
            "the animation start time starts at -1.0"
        );
        assert_eq!(m.anim_end_time, -1.0);
        assert_eq!(m.position, 0.0);
    }
}
