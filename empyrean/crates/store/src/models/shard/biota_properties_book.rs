// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/Shard/BiotaPropertiesBook.cs
//! `BiotaPropertiesBook`: a row of the `shard` database (Entity Framework model).

// ACE: BiotaPropertiesBook
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BiotaPropertiesBook {
    // ACE: BiotaPropertiesBook.ObjectId
    pub object_id: u32,
    // ACE: BiotaPropertiesBook.MaxNumPages
    pub max_num_pages: i32,
    // ACE: BiotaPropertiesBook.MaxNumCharsPerPage
    pub max_num_chars_per_page: i32,
}
