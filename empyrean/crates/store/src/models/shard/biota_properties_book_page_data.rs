// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/Shard/BiotaPropertiesBookPageData.cs
//! `BiotaPropertiesBookPageData`: a row of the `shard` database (Entity Framework model).

// ACE: BiotaPropertiesBookPageData
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BiotaPropertiesBookPageData {
    // ACE: BiotaPropertiesBookPageData.Id
    pub id: u32,
    // ACE: BiotaPropertiesBookPageData.ObjectId
    pub object_id: u32,
    // ACE: BiotaPropertiesBookPageData.PageId
    pub page_id: u32,
    // ACE: BiotaPropertiesBookPageData.AuthorId
    pub author_id: u32,
    // ACE: BiotaPropertiesBookPageData.AuthorName
    pub author_name: Option<String>,
    // ACE: BiotaPropertiesBookPageData.AuthorAccount
    pub author_account: Option<String>,
    // ACE: BiotaPropertiesBookPageData.IgnoreAuthor
    pub ignore_author: bool,
    // ACE: BiotaPropertiesBookPageData.PageText
    pub page_text: Option<String>,
}
