// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/PageData.cs
//! `PageData`: one book page as sent to the client (fields only). The strings are nullable in
//! ACE and default to `null`.

/// ACE: PageData
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PageData {
    // ACE: PageData.AuthorID
    pub author_id: u32,
    // ACE: PageData.AuthorName
    pub author_name: Option<String>,
    // ACE: PageData.AuthorAccount
    pub author_account: Option<String>,
    // ACE: PageData.IgnoreAuthor
    pub ignore_author: bool,
    // ACE: PageData.PageText
    pub page_text: Option<String>,
    /// 0 based.
    // ACE: PageData.PageIdx
    pub page_idx: u32,
}
