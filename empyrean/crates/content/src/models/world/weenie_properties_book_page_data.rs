// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/World/WeeniePropertiesBookPageData.cs
//! `WeeniePropertiesBookPageData`: one row of the world-database table `weenie_properties_book_page_data`.

/// Page Properties of Weenies
// ACE: WeeniePropertiesBookPageData
#[derive(Debug, Clone, PartialEq, Default)]
pub struct WeeniePropertiesBookPageData {
    /// Unique Id of this Property
    pub id: u32,
    /// Id of the Book object this page belongs to
    pub object_id: u32,
    /// Id of the page number for this page
    pub page_id: u32,
    /// Id of the Author of this page
    pub author_id: u32,
    /// Character Name of the Author of this page
    pub author_name: String,
    /// Account Name of the Author of this page
    pub author_account: String,
    /// if this is true, any character in the world can change the page
    pub ignore_author: bool,
    /// Text of the Page
    pub page_text: String,
    // ACE: WeeniePropertiesBookPageData.Object navigates back to the parent row, not carried.
}
