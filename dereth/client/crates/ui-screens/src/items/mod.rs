//! `ItemListWidget`, `UiItemWidget` and the drag-and-drop contract.
//!
//! **Nothing here moves an item.** An inventory move ghosts the icon (waiting state 1) and waits for
//! `Item_ServerSaysMoveItem`. A list that moves the item itself will look better than retail and be
//! wrong, so [`ItemList`] has no method that reorders its own slots.
//!
//! **The inventory request lock has no timeout.** A refused action is *"You can only move
//! or use one item at a time"*, and a lost server reply wedges the UI until an unrelated
//! `Item_ServerSaysMoveItem` arrives. `request_move` reproduces the wedge.

#[path = "ui_item.rs"]
pub mod runtime;
pub mod widget;

use dereth_primitives::ObjectId;
use dereth_ui::{ElementId, MessageId};

use crate::view::{DropTarget, UiRequest};

/// The attributes the item list reads at post-init.
pub const ITEM_LIST_ATTRIBUTES: &[u32] = &[
    0x1000_0011,
    0x1000_0012,
    0x1000_0013,
    0x1000_0014,
    0x1000_0015,
    0x1000_0016,
    0x1000_0017,
    0x1000_0052,
    0x1000_0053,
    0x1000_0054,
    0x1000_0057,
    0x1000_0059,
    0x1000_005A,
    0x1000_005B,
    0x1000_005C,
    0x1000_005D,
    0x5F,
    0x60,
];

/// The rows and columns attributes, named out of the list above.
pub mod attr {
    /// Rows.
    pub const ROWS: u32 = 0x5F;
    /// Columns.
    pub const COLS: u32 = 0x60;
    /// Slot spacing.
    pub const SPACING: u32 = 0x1000_005B;
}

/// The three element messages drag-and-drop is mediated by.
pub mod msg {
    use dereth_ui::MessageId;
    /// Drop release.
    pub const DROP: MessageId = MessageId(0x15);
    /// Rollover.
    pub const ROLLOVER: MessageId = MessageId(0x1C);
    /// Drag over.
    pub const DRAG_OVER: MessageId = MessageId(0x3E);
}

/// One slot of a `ItemListWidget`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Slot {
    pub item: Option<ObjectId>,
    /// Waiting state 1 — the ghosted icon while a move is outstanding.
    pub waiting: bool,
}

/// `ItemListWidget` — element type `0x10000031`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemList {
    pub element: ElementId,
    pub slots: Vec<Slot>,
}

/// The refusal text the client shows when a second move is attempted.
pub const MOVE_REFUSED: &str = "You can only move or use one item at a time";

/// The client-wide inventory request lock. **It has no timeout**: a lost server reply wedges the
/// inventory UI until an unrelated server move-item notice arrives, and that is the specification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RequestLock {
    pub outstanding: bool,
}

impl RequestLock {
    /// Take the lock, or refuse.
    pub fn take(&mut self) -> Result<(), &'static str> {
        if self.outstanding {
            return Err(MOVE_REFUSED);
        }
        self.outstanding = true;
        Ok(())
    }

    /// `Item_ServerSaysMoveItem` — releases the lock **whatever item it names**, which is the only
    /// way out of the wedge.
    pub fn server_says_move_item(&mut self) {
        self.outstanding = false;
    }
}

impl ItemList {
    /// An empty list of `n` slots.
    #[must_use]
    pub fn new(element: ElementId, n: usize) -> Self {
        Self {
            element,
            slots: vec![Slot::default(); n],
        }
    }

    /// Put an item in a slot. This is the *server telling us*, not a local move.
    pub fn set(&mut self, slot: usize, item: Option<ObjectId>) {
        if let Some(s) = self.slots.get_mut(slot) {
            s.item = item;
            s.waiting = false;
        }
    }

    /// The item under a slot.
    #[must_use]
    pub fn get(&self, slot: usize) -> Option<ObjectId> {
        self.slots.get(slot).and_then(|s| s.item)
    }

    /// A completed drag onto `target`.
    ///
    /// The list **ghosts the source icon and emits a request**; it does not move anything. Returns
    /// the refusal text when the lock is already held.
    pub fn request_move(
        &mut self,
        lock: &mut RequestLock,
        from_slot: usize,
        target: DropTarget,
    ) -> Result<UiRequest, &'static str> {
        let Some(item) = self.get(from_slot) else {
            return Err(MOVE_REFUSED);
        };
        lock.take()?;
        if let Some(s) = self.slots.get_mut(from_slot) {
            s.waiting = true;
        }
        Ok(UiRequest::DragDrop { item, target })
    }
}

/// `UiItemWidget` — element type `0x10000032`. "23 child ids (`0x1000033B`…`0x10000558`)
/// provide the icon layers: base icon, overlay, underlay, selection ring, stack-count text, burden
/// text and the 'pending' spinner. The burden meter uses attribute `0x69`."
pub mod ui_item {
    /// The first and last of the 23 child ids the document gives as a range.
    pub const FIRST_CHILD: u32 = 0x1000_033B;
    /// See [`FIRST_CHILD`].
    pub const LAST_CHILD: u32 = 0x1000_0558;
    /// How many children the range names.
    pub const CHILD_COUNT: usize = 23;
    /// The burden meter's fill attribute.
    pub const BURDEN_METER_ATTR: u32 = 0x69;
}

/// The element message that means "this row was activated" on a list box, used by the inventory
/// panels to open a container.
pub const LIST_ITEM_ACTIVATED: MessageId = MessageId(0x43);

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the toolbar and panel behavior's attribute and notice lists for
    /// `ItemListWidget`.
    #[test]
    fn the_item_list_reads_the_documented_attributes_and_notices() {
        assert_eq!(ITEM_LIST_ATTRIBUTES.len(), 18);
        assert!(ITEM_LIST_ATTRIBUTES.contains(&attr::ROWS));
        assert!(ITEM_LIST_ATTRIBUTES.contains(&attr::COLS));
        assert!(ITEM_LIST_ATTRIBUTES.contains(&attr::SPACING));
        assert_eq!(msg::DROP, MessageId(0x15));
        assert_eq!(msg::ROLLOVER, MessageId(0x1C));
        assert_eq!(msg::DRAG_OVER, MessageId(0x3E));
        assert_eq!(ui_item::CHILD_COUNT, 23);
        assert_eq!(ui_item::BURDEN_METER_ATTR, crate::bind::attr::METER_LEVEL);
    }

    /// An inventory move
    /// ghosts the icon (waiting state 1) and waits for `Item_ServerSaysMoveItem`.
    ///
    /// The assertion is what the list did **not** do: the item is still in its old slot.
    #[test]
    fn a_drag_between_two_lists_emits_one_request_and_moves_nothing() {
        let mut lock = RequestLock::default();
        let mut a = ItemList::new(ElementId(0x1000_01C6), 4);
        let b = ItemList::new(ElementId(0x1000_01CA), 4);
        a.set(1, Some(ObjectId(0x8000_0001)));

        let r = a
            .request_move(
                &mut lock,
                1,
                DropTarget::ItemList {
                    list: b.element,
                    slot: 0,
                },
            )
            .unwrap();
        assert_eq!(
            r,
            UiRequest::DragDrop {
                item: ObjectId(0x8000_0001),
                target: DropTarget::ItemList {
                    list: b.element,
                    slot: 0
                },
            }
        );
        assert_eq!(
            a.get(1),
            Some(ObjectId(0x8000_0001)),
            "the client predicts nothing"
        );
        assert!(a.slots[1].waiting, "the icon is ghosted instead");
        assert_eq!(b.get(0), None);

        // Only the server's own reply puts the item in its new place.
        let mut b = b;
        a.set(1, None);
        b.set(0, Some(ObjectId(0x8000_0001)));
        lock.server_says_move_item();
        assert!(!a.slots[1].waiting);
    }

    /// The inventory request lock has no timeout: a lost
    /// server reply wedges the inventory UI until an unrelated server move-item notice arrives. Do
    /// not add a spinner-with-timeout.
    #[test]
    fn a_second_move_is_refused_and_the_wedge_only_clears_on_a_server_reply() {
        let mut lock = RequestLock::default();
        let mut a = ItemList::new(ElementId(1), 2);
        a.set(0, Some(ObjectId(1)));
        a.set(1, Some(ObjectId(2)));

        assert!(a.request_move(&mut lock, 0, DropTarget::World).is_ok());
        assert_eq!(
            a.request_move(&mut lock, 1, DropTarget::World),
            Err(MOVE_REFUSED)
        );
        // …and it stays refused for as long as the reply never comes. There is no timeout to wait
        // out, so the only assertion available is that repeating it changes nothing.
        for _ in 0..1000 {
            assert_eq!(
                a.request_move(&mut lock, 1, DropTarget::World),
                Err(MOVE_REFUSED)
            );
        }
        // An *unrelated* server reply releases it.
        lock.server_says_move_item();
        assert!(a.request_move(&mut lock, 1, DropTarget::World).is_ok());
    }

    /// a drag from an empty slot has nothing to move.
    #[test]
    fn an_empty_slot_has_nothing_to_drag_and_does_not_take_the_lock() {
        let mut lock = RequestLock::default();
        let mut a = ItemList::new(ElementId(1), 2);
        assert_eq!(
            a.request_move(&mut lock, 0, DropTarget::World),
            Err(MOVE_REFUSED)
        );
        assert!(!lock.outstanding, "a refused drag must not wedge the UI");
    }
}
