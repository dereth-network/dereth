// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/World/WeeniePropertiesPalette.cs
//! `WeeniePropertiesPalette`: one row of the world-database table `weenie_properties_palette`.

/// Palette Changes (from PCAPs) of Weenies
// ACE: WeeniePropertiesPalette
#[derive(Debug, Clone, PartialEq, Default)]
pub struct WeeniePropertiesPalette {
    /// Unique Id of this Property
    pub id: u32,
    /// Id of the object this property belongs to
    pub object_id: u32,
    pub sub_palette_id: u32,
    pub offset: u16,
    pub length: u16,
    // ACE: WeeniePropertiesPalette.Object navigates back to the parent row, not carried.
}
