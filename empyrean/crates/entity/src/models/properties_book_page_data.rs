// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Models/PropertiesBookPageData.cs
//! `PropertiesBookPageData`: one book page.

/// ACE: PropertiesBookPageData. `Clone` copies every field; ACE's own `Clone()` is [`PropertiesBookPageData::ace_clone`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PropertiesBookPageData {
    // ACE: PropertiesBookPageData.AuthorId
    pub author_id: u32,
    // ACE: PropertiesBookPageData.AuthorName
    pub author_name: Option<String>,
    // ACE: PropertiesBookPageData.AuthorAccount
    pub author_account: Option<String>,
    // ACE: PropertiesBookPageData.IgnoreAuthor
    pub ignore_author: bool,
    // ACE: PropertiesBookPageData.PageText
    pub page_text: Option<String>,
}

impl PropertiesBookPageData {
    /// ACE's `Clone()`: a copy of every field.
    // ACE: PropertiesBookPageData.Clone
    #[must_use]
    pub fn ace_clone(&self) -> PropertiesBookPageData {
        PropertiesBookPageData {
            author_id: self.author_id,
            author_name: self.author_name.clone(),
            author_account: self.author_account.clone(),
            ignore_author: self.ignore_author,
            page_text: self.page_text.clone(),
        }
    }
}
