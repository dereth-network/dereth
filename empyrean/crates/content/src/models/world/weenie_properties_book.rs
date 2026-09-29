// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/World/WeeniePropertiesBook.cs
//! `WeeniePropertiesBook`: one row of the world-database table `weenie_properties_book`.

/// Book Properties of Weenies
// ACE: WeeniePropertiesBook
#[derive(Debug, Clone, PartialEq, Default)]
pub struct WeeniePropertiesBook {
    /// Unique Id of this Property
    pub id: u32,
    /// Id of the object this property belongs to
    pub object_id: u32,
    /// Maximum number of pages per book
    pub max_num_pages: i32,
    /// Maximum number of characters per page
    pub max_num_chars_per_page: i32,
    // ACE: WeeniePropertiesBook.Object navigates back to the parent row, not carried.
}
