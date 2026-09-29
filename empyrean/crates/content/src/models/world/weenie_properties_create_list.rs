// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/World/WeeniePropertiesCreateList.cs
//! `WeeniePropertiesCreateList`: one row of the world-database table `weenie_properties_create_list`.

/// CreateList Properties of Weenies
// ACE: WeeniePropertiesCreateList
#[derive(Debug, Clone, PartialEq, Default)]
pub struct WeeniePropertiesCreateList {
    /// Unique Id of this Property
    pub id: u32,
    /// Id of the object this property belongs to
    pub object_id: u32,
    /// Type of Destination the value applies to (DestinationType.????)
    pub destination_type: i8,
    /// Weenie Class Id of object to Create
    pub weenie_class_id: u32,
    /// Stack Size of object to create (-1 = infinite)
    pub stack_size: i32,
    /// Palette Color of Object
    pub palette: i8,
    /// Shade of Object&apos;s Palette
    pub shade: f32,
    /// Unused?
    pub try_to_bond: bool,
    // ACE: WeeniePropertiesCreateList.Object navigates back to the parent row, not carried.
}
