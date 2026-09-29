// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Models/PropertiesBook.cs
//! `PropertiesBook`: a book's limits.

/// ACE: PropertiesBook. `Clone` copies every field; ACE's own `Clone()` is [`PropertiesBook::ace_clone`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PropertiesBook {
    // ACE: PropertiesBook.MaxNumPages
    pub max_num_pages: i32,
    // ACE: PropertiesBook.MaxNumCharsPerPage
    pub max_num_chars_per_page: i32,
}

impl PropertiesBook {
    /// ACE's `Clone()`: a copy of every field.
    // ACE: PropertiesBook.Clone
    #[must_use]
    pub fn ace_clone(&self) -> PropertiesBook {
        PropertiesBook {
            max_num_pages: self.max_num_pages,
            max_num_chars_per_page: self.max_num_chars_per_page,
        }
    }
}
