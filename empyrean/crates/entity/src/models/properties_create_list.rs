// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Models/PropertiesCreateList.cs
//! `PropertiesCreateList`: one create-list entry.

use crate::enums::DestinationType;

/// ACE: PropertiesCreateList. `Clone` copies every field; ACE's own `Clone()` is [`PropertiesCreateList::ace_clone`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PropertiesCreateList {
    /// Only used to tie this record back to a specific database row.
    // ACE: PropertiesCreateList.DatabaseRecordId
    pub database_record_id: u32,
    // ACE: PropertiesCreateList.DestinationType
    pub destination_type: DestinationType,
    // ACE: PropertiesCreateList.WeenieClassId
    pub weenie_class_id: u32,
    // ACE: PropertiesCreateList.StackSize
    pub stack_size: i32,
    // ACE: PropertiesCreateList.Palette
    pub palette: i8,
    // ACE: PropertiesCreateList.Shade
    pub shade: f32,
    // ACE: PropertiesCreateList.TryToBond
    pub try_to_bond: bool,
}

impl PropertiesCreateList {
    /// ACE's `Clone()`: a copy with `DatabaseRecordId` reset to 0.
    // ACE: PropertiesCreateList.Clone
    #[must_use]
    pub fn ace_clone(&self) -> PropertiesCreateList {
        PropertiesCreateList {
            destination_type: self.destination_type,
            weenie_class_id: self.weenie_class_id,
            stack_size: self.stack_size,
            palette: self.palette,
            shade: self.shade,
            try_to_bond: self.try_to_bond,
            database_record_id: 0,
        }
    }
}
