// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/StarterItem.cs
//! Port of `Source/ACE.Server/Entity/StarterItem.cs`.
//!
//! The data class is the generated starter-gear table's row type,
//! [`crate::factories::starter_gear_factory::StarterItem`] (re-exported here).

pub use crate::factories::starter_gear_factory::StarterItem;

impl Default for StarterItem {
    // ACE: StarterItem.StarterItem
    /// `new StarterItem()`: `StackSize = 1`, the value of an entry without `"stacksize"`.
    fn default() -> Self {
        Self {
            weenie_id: 0,
            stack_size: 1,
        }
    }
}
