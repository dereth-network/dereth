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
