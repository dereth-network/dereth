//! The cursor decision's item-use predicates: the `ITEM_USEABLE` bits and the two target-use
//! tests the cursor path and the HUD's view both read. Pure functions of an object's useability
//! word, which the object model owns with its three source-use predicates; the rest of the cursor
//! decision is the application's.

/// `ITEM_USEABLE` — the bits [`ItemUses`] answers in.
pub use dereth_client_model::weenie::item_useable;

/// All five item-use predicates, the two target-use ones this cursor path reads among them.
pub use dereth_client_model::inventory::use_object::ItemUses;
