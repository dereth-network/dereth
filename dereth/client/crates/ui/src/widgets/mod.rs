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
    /// Parent bounds clamp the right and bottom edges before resize and move.
    /// `resize_to` runs before `move_to`. Child anchors are recomputed from their design
    /// rectangles against the final parent box, rather than adjusted incrementally.
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
            // Keep the resized edges within the parent.
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
pub mod field;

/// `Button` (1) — a push button. Derives from `TextElement`, so it carries a caption.
///
/// Broadcasts element message **1** with `p1 = 7` on an ordinary click and **2** for a hot click.
/// Mouse-down alone raises nothing unless auto-repeat is enabled. Supports
/// **hot-clicking**: auto-repeat while held, driven by global message 3
/// (the hot-clicking flag and the next hot-click time).
pub mod button;

/// The drag handle (2) — a title-bar grab handle. On mouse-down it calls `start_movement` on its
/// **parent**, which is how AC's windows move.
pub mod dragbar;

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
pub mod resizebar;

/// `Panel` (8) — a tabbed container. A tab-to-page table (tab element id → page element id)
/// and its page-to-tab inverse, with the open page and open tab. Broadcasts **0x2C** on page
/// change.
pub mod panel;

/// `GroupBox` (0x11) — a radio-button group driven by its selected-child attribute.
pub mod groupbox;

/// `Viewport` (0x0D) — the 3D preview: owns a viewport render object and carries a
/// `CreatureMode` for posing a rendered creature (the paper doll, char-gen and the barber).
///
/// The rendering is the renderer's and the creature posing is the animation system's; this side owns only the
/// element and the fact that it always owns its own render object.
pub mod viewport;

/// `ListBox` (5) — a multi-column list of child elements with composed scrolling behavior.
/// Broadcasts **4** on selection change (`p1 = index`, `p2 = item`) and
/// **0x43** on row activation.
pub mod listbox;

/// `Menu` (6) — a drop-down: a `Button` that owns a popup containing a
/// `ListBox`. Broadcasts **7** for selection, **8** when opened, and **9** when closed.
pub mod menu;

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
/// `0x21000005` carries `0x68 = false` or nothing at all — so that arm is left unimplemented
/// rather than guessed at; see `Meter::child_clip`'s note.
///
/// The property names come from the retail `MasterProperty 0x39000001` and settle what each
/// attribute is: `0x66 UICore_Meter_goal_position`, `0x67 …_frame_meter`, `0x68 …_move_fill`,
/// `0x69 …_position`, `0x6A …_smooth_movement`, `0x6B …_smooth_movement_duration`,
/// `0x6F …_child_direction`.
pub mod meter;

/// `ColorPicker` (0x10) — a palette swatch grid. Broadcasts **0x30** with the chosen
/// colour.
pub mod colorpicker;
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
pub mod scrollbar;

#[cfg(test)]
mod meter_tests;
