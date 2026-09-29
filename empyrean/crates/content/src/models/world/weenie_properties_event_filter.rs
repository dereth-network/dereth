// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/World/WeeniePropertiesEventFilter.cs
//! `WeeniePropertiesEventFilter`: one row of the world-database table `weenie_properties_event_filter`.

/// EventFilter Properties of Weenies
// ACE: WeeniePropertiesEventFilter
#[derive(Debug, Clone, PartialEq, Default)]
pub struct WeeniePropertiesEventFilter {
    /// Unique Id of this Property
    pub id: u32,
    /// Id of the object this property belongs to
    pub object_id: u32,
    /// Id of Event to filter
    pub event: i32,
    // ACE: WeeniePropertiesEventFilter.Object navigates back to the parent row, not carried.
}
