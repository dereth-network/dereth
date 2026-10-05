//! The spell-component row's icon.
//!
//! One function out of `dereth_ui_screens::panels::spellcomponent`: `dereth_client_shell::hud` composes the
//! component rows and has to pick the same icon the panel would -- the row's component icon, not
//! the item's own icon.

use dereth_primitives::DataId;

/// The component icon a row draws, **not** the item's own icon.
#[must_use]
pub fn row_icon(icon: u32) -> Option<DataId> {
    (icon != 0).then_some(DataId(icon))
}
