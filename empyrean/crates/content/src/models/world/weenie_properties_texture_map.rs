// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/World/WeeniePropertiesTextureMap.cs
//! `WeeniePropertiesTextureMap`: one row of the world-database table `weenie_properties_texture_map`.

/// Texture Map Changes (from PCAPs) of Weenies
// ACE: WeeniePropertiesTextureMap
#[derive(Debug, Clone, PartialEq, Default)]
pub struct WeeniePropertiesTextureMap {
    /// Unique Id of this Property
    pub id: u32,
    /// Id of the object this property belongs to
    pub object_id: u32,
    pub index: u8,
    pub old_id: u32,
    pub new_id: u32,
    // ACE: WeeniePropertiesTextureMap.Object navigates back to the parent row, not carried.
}
