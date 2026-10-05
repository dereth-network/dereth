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
