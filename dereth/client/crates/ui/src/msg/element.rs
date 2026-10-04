//! Element messages: the catalogue, the listener tables, the serial number and deferred
//! unregistration.
//!
//! Element-message registration and delivery live here.
//!
//! Two things here are load-bearing:
//!
//! * **The serial number.** Without it a listener registered on a child *and* on its parent
//!   receives the same message twice, and `Toolbar` and `ItemListWidget` double-handle
//!   every click.
//! * **Deferred unregistration.** Panels routinely delete themselves inside a message handler.

use crate::{ElemHandle, ElementId, MessageId};

/// Who receives messages. `External` identifies a [`crate::framework::UiFlow`] or screen: anything
/// that is not an element in this arena. See [`crate::msg::Delivery`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ListenerId {
    Element(ElemHandle),
    External(u32),
}

/// The element message's two mouse points: window-relative and element-relative.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MessagePoint {
    /// Screen coordinates.
    pub window: (i32, i32),
    /// Relative to the element's screen origin.
    pub element: (i32, i32),
}

/// One element message as the client delivers it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElementMessage {
    /// The **source element's** element id (from its description), not the receiver's.
    pub source_id: ElementId,
    /// The source element.
    pub source: ElemHandle,
    /// The message id.
    pub id: MessageId,
    pub p1: u32,
    pub p2: u32,
    pub point: MessagePoint,
    /// The serial number — the de-duplication token.
    pub serial: u32,
}

/// One listener recipient and its reentrant registration count: register twice, unregister twice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListenerReg {
    pub who: ListenerId,
    pub count: i32,
}

/// A deferred unregistration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Removal {
    ElementMessage {
        element: ElementId,
        message: MessageId,
        who: ListenerId,
    },
    GlobalMessage {
        message: MessageId,
        who: ListenerId,
    },
    AllMessages {
        who: ListenerId,
    },
    /// Unregistering a listener from one element's own listener table.
    FromElement {
        element: ElemHandle,
        who: ListenerId,
    },
}

/// The element-message id catalogue.
///
/// Ids are the client's; the names are this document's. Everything here is raised somewhere in the
/// shipped client — the table lists the raiser for each.
pub mod id {
    use crate::MessageId;

    /// 0x01 — **button clicked**. `p1 = 7`.
    ///
    /// Not "button *pressed*": 0x01 is the click and 0x02 the auto-repeat. The button's mouse-up is the only place a
    /// button raises 1, it raises it on the completed click (press and release on the same
    /// element, not disabled, not auto-repeating) with `p1 = 7`, and every
    /// screen's element-message handler in the client tests for message 1 from a button —
    /// the character-management screen does for all six of its own. 0x02 is the auto-repeat.
    pub const BUTTON_CLICKED: MessageId = MessageId(0x01);
    /// 0x02 — the **auto-repeat** ("hot") click. `p1 = the input action`.
    ///
    /// Raised by when attribute 0x0F is set, and then
    /// once per interval by while the mouse stays over the
    /// button. handles its increment and
    /// decrement buttons on **this** id, which is why holding an arrow scrolls.
    pub const BUTTON_HOT_CLICK: MessageId = MessageId(0x02);
    /// 0x04 — list selection changed. `p1 = index`, `p2 = selected item element`.
    pub const LIST_SELECTION_CHANGED: MessageId = MessageId(0x04);
    /// 0x07 — **a menu item was chosen**. `p1 = the item's element id`, `p2 = the item element`.
    ///
    /// **0x07 is the choice and 0x08 the open**, not the other way round. The menu's open (after
    /// showing the popup) raises 8, its close raises 9, and
    /// a new selection raises 7 with `p1` = the selected item's element id and `p2` = the selected
    /// item; the no-selection arm raises 7 with `p1 = p2 = 0`.
    ///
    /// The consumer settles it independently: the main chat window's message handler
    /// handles message 7 by reading **`p2` as the chosen item** and asking it for
    /// attribute `0x1000000B` — which is only meaningful on a row, never on an open/close.
    ///
    pub const MENU_CHOSEN: MessageId = MessageId(0x07);
    /// 0x08 — menu opened. See [`MENU_CHOSEN`] for why this is 8 and not 7.
    pub const MENU_OPENED: MessageId = MessageId(0x08);
    /// 0x09 — menu closed.
    pub const MENU_CLOSED: MessageId = MessageId(0x09);
    /// 0x0A — scrollbar position changed. `p1 = new position`.
    pub const SCROLL_POSITION: MessageId = MessageId(0x0A);
    /// 0x0D — scrollbar page down.
    ///
    /// **The names of these four and the client's use of them do not agree, and the ids win.**
    /// The scrollbar's move-steps handler raises `0x0D`/`0x0E` for an
    /// **arrow** click (`(delta < 1) + 0x0D`, with the step in `p1`) and
    /// its page-click handler raises `0x0F`/`0x10` for a click on the empty **track** either
    /// side of the thumb (`0x10 - (before the thumb)`). The names here are
    /// kept as labels; a reader who wants "which gesture raised this"
    /// needs the two handlers described above.
    pub const SCROLL_PAGE_DOWN: MessageId = MessageId(0x0D);
    /// 0x0E — scrollbar page up. See [`SCROLL_PAGE_DOWN`] for what actually raises it.
    pub const SCROLL_PAGE_UP: MessageId = MessageId(0x0E);
    /// 0x0F — scrollbar line down. See [`SCROLL_PAGE_DOWN`].
    pub const SCROLL_LINE_DOWN: MessageId = MessageId(0x0F);
    /// 0x10 — scrollbar line up. See [`SCROLL_PAGE_DOWN`].
    pub const SCROLL_LINE_UP: MessageId = MessageId(0x10);
    /// 0x12 — character typed. `p1 = UTF-16 code unit`. Fires for **every** character, including
    /// ones the filter rejected.
    pub const CHARACTER: MessageId = MessageId(0x12);
    /// 0x14 — drag proxy created. `p1 = the proxy element`.
    pub const DRAG_PROXY_CREATED: MessageId = MessageId(0x14);
    /// 0x15 — item dropped / drag-drop **failed**.
    pub const DROP_FAILED: MessageId = MessageId(0x15);
    /// 0x16 — drag-drop **succeeded** (`0x15 + success`).
    pub const DROP_SUCCEEDED: MessageId = MessageId(0x16);
    /// 0x18 — visibility changed. `p1 = new visibility`.
    pub const VISIBILITY_CHANGED: MessageId = MessageId(0x18);
    /// 0x19 — **mouse click** (press + release inside). `p1 = input action`.
    pub const MOUSE_CLICK: MessageId = MessageId(0x19);
    /// 0x1A — double click.
    pub const MOUSE_DOUBLE_CLICK: MessageId = MessageId(0x1A);
    /// 0x1B — mouse-over-top changed. `p1 = bool`.
    pub const MOUSE_OVER_TOP: MessageId = MessageId(0x1B);
    /// 0x1C — **mouse press**.
    pub const MOUSE_PRESS: MessageId = MessageId(0x1C);
    /// 0x1D — **mouse release**.
    pub const MOUSE_RELEASE: MessageId = MessageId(0x1D);
    /// 0x1E — mouse moved over element.
    pub const MOUSE_MOVE: MessageId = MessageId(0x1E);
    /// 0x1F — mouse-over changed. `p1 = bool`.
    pub const MOUSE_OVER: MessageId = MessageId(0x1F);
    /// 0x20 — element being resized by the mouse.
    pub const BEING_RESIZED: MessageId = MessageId(0x20);
    /// 0x21 — drag rejected: nobody up the chain is draggable.
    pub const DRAG_REJECTED: MessageId = MessageId(0x21);
    /// 0x24 — element resized (only when the element's notify-on-resize flag is set).
    pub const RESIZED: MessageId = MessageId(0x24);
    /// 0x26 — parent changed. `p1 = success`.
    pub const PARENT_CHANGED: MessageId = MessageId(0x26);
    /// 0x27 — **context-menu request**: the element's context-menu flag and input action 8.
    pub const CONTEXT_MENU: MessageId = MessageId(0x27);
    /// 0x29 — activated.
    pub const ACTIVATED: MessageId = MessageId(0x29);
    /// 0x2A — deactivated.
    pub const DEACTIVATED: MessageId = MessageId(0x2A);
    /// 0x2C — tab page changed.
    pub const TAB_PAGE_CHANGED: MessageId = MessageId(0x2C);
    /// 0x2D — meter animation started.
    pub const METER_ANIM_START: MessageId = MessageId(0x2D);
    /// 0x2E — meter animation finished.
    pub const METER_ANIM_END: MessageId = MessageId(0x2E);
    /// 0x2F — **focus changed**. `p1 = 1 gained / 0 lost`.
    pub const FOCUS_CHANGED: MessageId = MessageId(0x2F);
    /// 0x30 — colour chosen. `p1 = RGBA`.
    pub const COLOR_CHOSEN: MessageId = MessageId(0x30);
    /// 0x31 — visibility-toggle action. `p1 = input action id`.
    pub const VISIBILITY_TOGGLE: MessageId = MessageId(0x31);
    /// 0x32 — scroll offset changed. `p1`/`p2 = x`/`y`.
    ///
    /// Raised when scrollable content grows or shrinks. A menu answers from its own
    /// list box with the current content extent, which is how a drop-down grows to fit the
    /// rows it was given.
    pub const SCROLL_OFFSET: MessageId = MessageId(0x32);
    /// 0x3E — **the drag cursor entered or left this element**. `p1 = 0 leaving / 1 entering`.
    ///
    /// **This is not the activation alert.**
    /// The element manager's activation alert broadcasts **`0x2F`**
    /// ([`FOCUS_CHANGED`]), and only on its deactivation arm. `0x3E` has exactly
    /// **one producer in the whole client** — the element manager's mouse-over switch's
    /// drag tail, which broadcasts `0x3E` with `p1 = 0` to the old element and with `p1 = 1` to
    /// the new one, where the new one is the hit element's drag-and-drop catcher for the element
    /// being dragged. The retail client has no third `0x3E` broadcast.
    ///
    /// The UI item's `0x3E` arm is what puts the green
    /// arrow / red x / into-container hint on an inventory slot
    /// (the item list's drag-over on `p1 = 1`, hide
    /// the drag-accept icon on `p1 = 0`), so this id is the whole of the reported *"cursor
    /// hints when moving do not show up"*.
    ///
    /// See [`crate::UiSystem::mouse_move`] for the producer.
    pub const DRAG_CURSOR_OVER: MessageId = MessageId(0x3E);
    /// 0x40 — **mouse tap** (a click action that does not take capture).
    pub const MOUSE_TAP: MessageId = MessageId(0x40);
    /// 0x41 — key down. `p1 = key`.
    pub const KEY_DOWN: MessageId = MessageId(0x41);
    /// 0x42 — key up.
    pub const KEY_UP: MessageId = MessageId(0x42);
    /// 0x43 — list item activated (double-click / enter). `p1 = item element`.
    pub const LIST_ITEM_ACTIVATED: MessageId = MessageId(0x43);
    /// 0x44 — text content changed.
    pub const TEXT_CHANGED: MessageId = MessageId(0x44);
    /// 0x10000004 — a tracked quality changed. `p1 = StatType`, `p2 = new value`.
    /// Raised when a tracked quality changes on an info region.
    pub const QUALITY_CHANGED: MessageId = MessageId(0x1000_0004);

    /// The four ids whose registration makes an element hit-testable automatically, taken from the
    /// client's own table of auto-mouse message ids. See
    /// [`crate::UiSystem::create_element_recursive_from_full_desc`] step 4.
    pub const AUTO_MOUSE_VISIBLE: [MessageId; 4] =
        [MOUSE_CLICK, MOUSE_PRESS, MOUSE_RELEASE, MOUSE_TAP];
}

/// Insert or bump a registration.
pub(crate) fn register(list: &mut Vec<ListenerReg>, who: ListenerId) {
    if let Some(r) = list.iter_mut().find(|r| r.who == who) {
        r.count += 1;
    } else {
        list.push(ListenerReg { who, count: 1 });
    }
}

/// Decrement, removing at zero..
pub(crate) fn unregister(list: &mut Vec<ListenerReg>, who: ListenerId) {
    if let Some(i) = list.iter().position(|r| r.who == who) {
        list[i].count -= 1;
        if list[i].count <= 0 {
            list.remove(i);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: each listener registration has a count, making registration re-entrant:
    /// registering twice requires unregistering twice.
    #[test]
    fn registration_is_reference_counted() {
        let mut l = Vec::new();
        let a = ListenerId::External(1);
        register(&mut l, a);
        register(&mut l, a);
        assert_eq!(l, vec![ListenerReg { who: a, count: 2 }]);
        unregister(&mut l, a);
        assert_eq!(l.len(), 1);
        unregister(&mut l, a);
        assert!(l.is_empty());
        // Unregistering something that was never registered is a no-op, not a panic.
        unregister(&mut l, a);
        assert!(l.is_empty());
    }
}
