//! The one `panels::spellbook` constant `GameView` names.
//!
//! `GameView::spell_filters`'s default body is this number, so it has to be reachable from a
//! crate that may not depend on `dereth-ui-screens`. The panel's element ids, its `School` table
//! and its sorting stay where they are.
//! `dereth_ui_screens::panels::spellbook::DEFAULT_SPELL_FILTERS` is a `pub use` of this constant.

/// The default spell-filter setting when section `0x0020` is absent — every
/// school and every level. The protocol decoder uses the same
/// number; it is repeated here because this crate does not depend on `dereth-protocol`.
pub const DEFAULT_SPELL_FILTERS: u32 = 0x3FFF;

// The raw component array's first slot, shared with
// `dereth_ui_screens::panels::spellbook`. `dereth_client::hud` reads it off `raw_comps[0]` and the
// key when it composes a spell's component list; it is one subtraction.

/// The **first** formula slot, decrypted — subtracts the key
/// from every non-zero slot in place, so slot 0 keeps its position and a zero slot stays zero.
///
/// `dereth_assets::tables::SpellBase::comps` drops the zero slots, which loses that position; the
/// power component has to be read off `raw_comps[0]` and the key.
#[must_use]
pub const fn power_component(raw_comp0: u32, comp_key: u32) -> u32 {
    if raw_comp0 == 0 {
        0
    } else {
        raw_comp0.wrapping_sub(comp_key)
    }
}
