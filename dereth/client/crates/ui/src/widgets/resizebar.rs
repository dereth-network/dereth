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
