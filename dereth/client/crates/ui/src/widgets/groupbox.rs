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
