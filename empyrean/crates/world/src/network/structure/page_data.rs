// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Structure/PageData.cs
//! Port of `Source/ACE.Server/Network/Structure/PageData.cs`.

use crate::network::game_messages::game_message::ace_str;
use crate::network::game_messages::game_message::write_record;

// ACE: PageData
/// The content of an individual page for parchment and tomes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageData {
    // ACE: PageData.AuthorGuid
    pub author_guid: u32,
    // ACE: PageData.AuthorName
    pub author_name: Option<String>,
    // ACE: PageData.AuthorAccount
    pub author_account: Option<String>,
    // ACE: PageData.Version
    /// if HIWORD is not 0xFFFF, this is textIncluded. For our purpose this should always be
    /// 0xFFFF0002
    pub version: u32,
    // ACE: PageData.HasText
    pub has_text: bool,
    // ACE: PageData.IgnoreAuthor
    pub ignore_author: bool,

    // ACE: PageData.PageText
    pub page_text: Option<String>,
}

impl Default for PageData {
    /// The C# field initializer `Version = 0xFFFF0002`.
    fn default() -> Self {
        PageData {
            author_guid: 0,
            author_name: None,
            author_account: None,
            version: 0xFFFF_0002,
            has_text: false,
            ignore_author: false,
            page_text: None,
        }
    }
}

// ACE: PageDataExtensions.Write
pub fn write(writer: &mut Vec<u8>, page: &PageData) {
    let strings = [
        page.author_name.as_deref().unwrap_or(""),
        page.author_account.as_deref().unwrap_or(""),
        page.page_text.as_deref().unwrap_or(""),
    ];
    write_record(writer, &strings, |w| record(page).write(w));
}

/// The dereth-protocol record the `Write` extension below writes, field for field. ACE writes `Version` (0xFFFF0002) whole: the record's version form with its two flags.
#[must_use]
pub fn record(page: &PageData) -> dereth_protocol::trade::PageData {
    assert!(
        page.version >> 16 == 0xFFFF
            && page.version & 0xFFFF
                == u32::from(dereth_protocol::trade::PageData::VERSION_WITH_FLAGS),
        "PageData.Version is 0xFFFF0002"
    );
    dereth_protocol::trade::PageData {
        author_id: dereth_primitives::ObjectId(page.author_guid),
        author_name: ace_str(page.author_name.as_deref()),
        author_account: ace_str(page.author_account.as_deref()),
        version: Some(dereth_protocol::trade::PageData::VERSION_WITH_FLAGS),
        text_included: i32::from(page.has_text),
        ignore_author: i32::from(page.ignore_author),
        page_text: page.has_text.then(|| ace_str(page.page_text.as_deref())),
    }
}
