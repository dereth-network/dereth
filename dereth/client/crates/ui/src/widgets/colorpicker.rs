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
