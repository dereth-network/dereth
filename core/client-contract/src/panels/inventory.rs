//! The three inventory names the world half spells.
//!
//! `dereth_ui_screens::panels::inventory` keeps the paper doll and the slot machinery. These three
//! cross the seam: `dereth_client_runtime::interaction` compares a drop target against
//! `PAPER_DOLL_DRAG_MASK` and speaks `CANNOT_PUT_THAT_ITEM_THERE` on the refusal, and
//! `dereth_client_shell::hud` reads `HERITAGE_GROUP_PROPERTY` off the qualities. An element id, a
//! retail literal and a property key.

use crate::ids::ElementId;

/// The transparent region over the doll that catches a drop
/// on the body itself. It hides and shows with the doll.
///
/// The paper-doll drop handler compares against this id, and it is the only
/// element of the panel that is not an
/// equipment slot and still accepts a drop. See `InventoryPanels::drop_target`.
pub const PAPER_DOLL_DRAG_MASK: ElementId = ElementId(0x1000_01D6);

/// The paper doll's refusal when a drag is offered to it, sent to the
/// feedback channel `0x1A` when the dragged item's valid locations have nothing in
/// `INVENTORY_LOC`'s wearable set (`0x0800_7FFF`).
///
/// Verbatim from retail, apostrophe included: `L"You can't put that item there"`. It is the one
/// refusal on any inventory drop path in this build that the client *speaks*; a drop refused by
/// the equipment slots' own gate is silent.
pub const CANNOT_PUT_THAT_ITEM_THERE: &str = "You can't put that item there";

/// `PropertyInt 0xBC` — `HeritageGroup`, the value the paper doll reads out of the player's
/// qualities when the player description arrives, and switches on before it redresses the doll
/// for the race.
pub const HERITAGE_GROUP_PROPERTY: u32 = 0xBC;
