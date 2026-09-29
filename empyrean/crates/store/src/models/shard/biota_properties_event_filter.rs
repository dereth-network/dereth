// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/Shard/BiotaPropertiesEventFilter.cs
//! `BiotaPropertiesEventFilter`: a row of the `shard` database (Entity Framework model).

// ACE: BiotaPropertiesEventFilter
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BiotaPropertiesEventFilter {
    // ACE: BiotaPropertiesEventFilter.ObjectId
    pub object_id: u32,
    // ACE: BiotaPropertiesEventFilter.Event
    pub event: i32,
}
