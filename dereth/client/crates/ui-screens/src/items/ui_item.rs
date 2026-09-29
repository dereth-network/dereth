//! The arena-owned identity of an item element, read by object lookup.
//!
//! ItemSlot still owns presentation. These are actual runtime fields, not DAT attributes or
//! drag-proxy payloads: item and spell-shortcut initialization and clearing write them.
//! Original-client lookup casts to type `0x10000032` and reads the item id at offset `0x5FC`.

use dereth_primitives::ObjectId;
use dereth_ui::{ElemHandle, Element, UiSystem};

#[derive(Debug, Default)]
pub struct UiItem {
    pub item: ObjectId,
    pub spell: u32,
}

impl Element for UiItem {
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }
    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
}

pub fn create(_: &dereth_ui::LayoutDesc, _: &dereth_ui::ElementDesc) -> Box<dyn Element> {
    Box::new(UiItem::default())
}

/// Downcast the exact element to [`UiItem`]. No ancestor search: an ordinary child is not an item.
#[must_use]
pub fn identity(ui: &UiSystem, handle: ElemHandle) -> Option<(ObjectId, u32)> {
    let item = ui
        .node(handle)?
        .behaviour
        .as_ref()?
        .as_any()?
        .downcast_ref::<UiItem>()?;
    Some((item.item, item.spell))
}

pub(super) fn set_identity(ui: &mut UiSystem, handle: ElemHandle, item: ObjectId, spell: u32) {
    let Some(node) = ui.node_mut(handle) else {
        return;
    };
    let Some(state) = node
        .behaviour
        .as_mut()
        .and_then(|b| b.as_any_mut())
        .and_then(|a| a.downcast_mut::<UiItem>())
    else {
        return;
    };
    state.item = item;
    state.spell = spell;
}
