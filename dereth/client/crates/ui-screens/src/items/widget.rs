//! `ItemListWidget` and `UiItemWidget` as **live elements**, not as a data model.
//!
//! # What was missing, and why nothing drew
//!
//! An empty quickbar and an empty backpack have one cause: a [`super::ItemList`] alone is a
//! *data model that creates no elements*. In the client an item
//! slot is a real subtree:
//!
//! ```text
//! item-list initialisation
//!   -> create one slot
//!        resolve layout enum 0x10000038 (type 0x23)   // the `ItemSlot` layout 0x21000037
//!        create the slot from that layout, rooted at   // a second layout, at run time
//!          the list's own enum attribute 0x1000000E
//!        clear the slot; set its state to 0x1000001C  // the empty-slot state
//! ```
//!
//! so a list whose `ItemListWidget` creates no children has nothing to draw — no icon element,
//! no quantity text, no ghost, no shortcut numeral. Every one of the shipped
//! `0x1000000E UI_ItemList_ItemSlotID` values is a **root element of `ItemSlot`**, one per slot,
//! and all 56 of them are `UiItemWidget` (type `0x10000032`) carrying the 23 icon-layer
//! children.
//!
//! # Why this is a plain struct and not an `Element` behaviour
//!
//! Everything an item slot displays is a fact about a game object — its icon, its stack size,
//! whether a move is outstanding — and a `dereth_ui::Element` handler is handed only
//! `&mut UiSystem`. So the *widget* here owns the tree surgery and the screen that owns the
//! subtree drives it from a [`GameView`](crate::view::GameView), as [`crate::panels`] and
//! [`crate::toolbar`] already do. The factory retains the inherited `ListBox` behavior while
//! the screen supplies game facts.
//!
//! Nothing here moves an item. A drag emits a
//! [`UiRequest::DragDrop`](crate::view::UiRequest) and the slot ghosts, and only the server's own
//! `Item_ServerSaysMoveItem` changes what a slot holds.

use dereth_primitives::{DataId, ObjectId};
use dereth_ui::framework::LayoutEnum;
use dereth_ui::{ElemHandle, ElementId, PropertyValue, StateId, UiSystem};

use crate::bind::{attr_bool, attr_enum, attr_int, set_attr_float};

mod ids;
pub use ids::*;
mod slot;
pub use slot::*;
mod list;
pub use list::*;

/// The constant grab offset [`ItemListWidget::begin_drag`] passes when it starts the drag, which
/// centres the 32×32 drag icon on the pointer.
pub const DRAG_GRAB: (i32, i32) = (0x10, 0x10);

/// What [`ItemListWidget::begin_drag`] picked up, which is what its two notices carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DragStart {
    /// The list the drag left — the item-list begin-drag notice's first argument.
    pub list: ElementId,
    /// The slot number, its second.
    pub slot: usize,
    /// The begin-drag notice's item id.
    pub item: Option<ObjectId>,
    /// The begin-drag notice's spell id.
    pub spell: Option<u32>,
    /// The element the drag start was handed; the proxy is a copy of it.
    pub drag_icon: ElemHandle,
    /// Whether the source slot took the ghost — false on a vendor, salvage or shortcut list.
    pub ghosted: bool,
}

/// Read the drag's payload back off the proxy.
///
/// The five properties [`ItemSlot::prepare_drag_icon`] wrote, folded into the drop-item flags
/// bitfield the client builds from the last four:
///
/// ```text
/// flags = salvage * 8 | shortcut * 4 | vendor * 2 | is_container * 1
/// ```
///
/// The drop is refused outright when the item id is 0 or when any of
/// bits 1..3 is set, i.e. **a vendor, shortcut or salvage drag is not an inventory move** and is
/// somebody else's to route.
#[must_use]
pub fn inq_drop_icon_info(ui: &UiSystem, proxy: ElemHandle) -> DropIconInfo {
    let mut info = DropIconInfo::default();
    let Some(n) = ui.node(proxy) else { return info };
    let p = n.merged_properties();
    let id = |k: u32| match p.get(k) {
        Some(PropertyValue::InstanceId(v)) => Some(*v),
        _ => None,
    };
    info.item = id(drag_attr::ITEM_ID).filter(|v| *v != 0).map(ObjectId);
    info.spell = id(drag_attr::SPELL_ID).filter(|v| *v != 0);
    let b = |k: u32| u32::from(p.get_bool(k).unwrap_or(false));
    // The client's combine matches the drop-item flags exactly
    // (`IS_CONTAINER 1, IS_VENDOR 2, IS_SHORTCUT 4, IS_SALVAGE 8`). Swapping vendor and shortcut
    // would be invisible to [`DropIconInfo::is_inventory_move`]'s `& 0xE`, which covers both bits
    // either way, so [`drag_flags`] states the four numbers as literals.
    info.flags = (b(drag_attr::IS_SALVAGE) * drag_flags::IS_SALVAGE)
        | (b(drag_attr::IS_SHORTCUT) * drag_flags::IS_SHORTCUT)
        | (b(drag_attr::IS_VENDOR) * drag_flags::IS_VENDOR)
        | (b(drag_attr::IS_CONTAINER) * drag_flags::IS_CONTAINER);
    info
}

/// The out-parameters of [`inq_drop_icon_info`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DropIconInfo {
    pub item: Option<ObjectId>,
    pub spell: Option<u32>,
    /// The drop-item flags: bit 0 container, 1 vendor, 2 shortcut, 3 salvage.
    pub flags: u32,
}

impl DropIconInfo {
    /// The client's `(flags & 0xE) == 0` — an inventory move and not a
    /// vendor, shortcut or salvage transaction.
    #[must_use]
    pub const fn is_inventory_move(&self) -> bool {
        self.flags & drag_flags::NOT_AN_INVENTORY_MOVE == 0
    }

    /// The container bit — the dragged object is itself a pack. This is the flag the drop
    /// acceptance receives (the drop release passes `flags & 1`), and it chooses between the items
    /// capacity and list and the containers capacity and list throughout.
    #[must_use]
    pub const fn is_container(&self) -> bool {
        self.flags & drag_flags::IS_CONTAINER != 0
    }
}

/// The ui item element's element-message handler's `0x21` arm.
///
/// On `0x21` (the generic drag was rejected) it finds the nearest ancestor of type `0x10000031`
/// above the slot's parent and, if there is one, starts that list's drag at the message's
/// window point.
///
/// **This is the whole join.** Generic drag dispatch raises `0x21` when nothing from the
/// pressed element up to the root carries `0x3A`, and neither an item slot nor its item list
/// does — slot initialization sets `0x3A = false` on purpose. An item slot is therefore picked
/// up *by being refused* the generic drag, and its list starts a second, deliberate drag with
/// its own drag icon.
///
/// `source` is the element the message names; `lists` is every live `ItemListWidget` the screen
/// owns, in place of the client's ancestor walk.
pub fn begin_drag_from_rejected(
    ui: &mut UiSystem,
    lists: &mut [&mut ItemListWidget],
    source: ElemHandle,
    x: i32,
    y: i32,
) -> Option<DragStart> {
    // **The `0x21` must name a slot.** In the client this is the *`UiItemWidget`*
    // listener's arm: it starts from the pressed slot and finds the list by walking up from its
    // parent to type `0x10000031`. A `0x21` naming the **list** reaches
    // the item list's own element-message handler, which tests only `0x1C` and `0x15`
    // and has no `0x21` arm at all, so retail starts no drag from one. Also accepting
    // `w.handle == source` would be a second, re-entrant drag start, because the walk raises the
    // message at every level.
    let owner = lists.iter().position(|w| w.slot_of(source).is_some())?;
    lists[owner].begin_drag(ui, x, y)
}

/// One `Array` attribute of `UI_ItemList_ShortcutOverlay` entries, flattened to its `DataID`s.
///
/// The original reader extracts a `DataID` at each array index. Here the array is a
/// `Vec<BaseProperty>` whose every member carries id [`attr::SHORTCUT_OVERLAY`] and a `DataFile` value. A member of
/// another type is skipped rather than defaulted, because a defaulted `DataID` would paint an
/// arbitrary surface into a slot.
fn shortcut_overlay_array(ui: &UiSystem, h: ElemHandle, id: u32) -> Option<Vec<DataId>> {
    let props = ui.node(h)?.merged_properties();
    let dereth_assets::ui::PropertyValue::Array(members) = props.get(id)? else {
        return None;
    };
    Some(
        members
            .iter()
            .filter_map(|b| match &b.value {
                dereth_assets::ui::PropertyValue::DataFile(d) => Some(*d),
                _ => None,
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests;
