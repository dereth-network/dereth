//! The two identifier newtypes the contract carries.
//!
//! [`UiRequest`](crate::UiRequest) names both — `DropTarget::{ItemList, EquipSlot}` carry an
//! [`ElementId`] and `UiRequest::QueueMode` carries a [`UiMode`] — so a contract crate that may
//! not depend on `dereth-ui` has to own them. `dereth_ui::ElementId` and `dereth_ui::framework::UiMode`
//! are `pub use`s of these, so every existing path still resolves.
//!
//! `ElementId`'s hand-written `Debug` is `dereth-ui`'s `ui_id!` macro expansion, character for
//! character: the hex form is what every layout id in this tree is read and written in, and a
//! derived `Debug` would print `ElementId(268435858)` where the sources say `0x10000192`. The
//! other four `ui_id!` types (`ElementType`, `StateId`, …) stay in `dereth-ui`: the contract names
//! none of them.

/// Unique **inside a layout**, and the key of the whole message system.
///
/// The element lookup is a **linear** search of the layout's element list that returns the
/// *first* match, so ids must stay unique across simultaneously loaded layouts. The client
/// achieves that by partitioning the id space per screen into `0x10000xxx` blocks.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct ElementId(pub u32);

impl std::fmt::Debug for ElementId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, concat!(stringify!(ElementId), "(0x{:X})"), self.0)
    }
}

/// A UI mode id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UiMode(pub u32);
