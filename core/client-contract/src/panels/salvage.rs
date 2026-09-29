//! The salvage panel's three object-carrying notices.
//!
//! The panel stayed in `dereth_ui_screens::panels::salvage`; this enum is the transit
//! `dereth_client::{hud, interaction}` fill (`Hud::pending_salvage`) and the panel drains, which
//! makes it the contract rather than the drawing. Three `ObjectId`s.

use dereth_primitives::ObjectId;

/// The three notices registers that carry an object, in registration order.
///
/// The list's begin-drag hook is the fourth and is **not** here: it is the list's own pick-up
/// hook (it clears the row's trade state as the row leaves), and
/// in this build a drag out of the window is the screen's drag machinery rather than a notice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SalvageNotice {
    /// A used salvage tool opens the salvage panel.
    Open(ObjectId),
    /// An item is added to the open salvage panel; the client open-codes its
    /// add-item path here rather than calling it.
    Add(ObjectId),
    /// An item is removed from the open salvage panel.
    Remove(ObjectId),
}
