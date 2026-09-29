// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/World/WeeniePropertiesAnimPart.cs
//! `WeeniePropertiesAnimPart`: one row of the world-database table `weenie_properties_anim_part`.

/// Animation Part Changes (from PCAPs) of Weenies
// ACE: WeeniePropertiesAnimPart
#[derive(Debug, Clone, PartialEq, Default)]
pub struct WeeniePropertiesAnimPart {
    /// Unique Id of this Property
    pub id: u32,
    /// Id of the object this property belongs to
    pub object_id: u32,
    pub index: u8,
    pub animation_id: u32,
    // ACE: WeeniePropertiesAnimPart.Object navigates back to the parent row, not carried.
}
