//! Shared readable-book editing actions and cursor projection.

use dereth_primitives::ObjectId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BookAction {
    Edit {
        book: ObjectId,
        page: i32,
        text: String,
    },
    Turn {
        book: ObjectId,
        page: i32,
    },
    Flush {
        book: ObjectId,
    },
    Close {
        book: ObjectId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookSessionView {
    pub current_page: i32,
    pub draft: String,
    pub pending: bool,
    pub editable: bool,
    pub revision: u64,
    pub page_turns: u32,
    pub pages_added: u32,
    pub book_data_refetches: u32,
}

impl Default for BookSessionView {
    fn default() -> Self {
        Self {
            current_page: -1,
            draft: String::new(),
            pending: false,
            editable: false,
            revision: 0,
            page_turns: 0,
            pages_added: 0,
            book_data_refetches: 0,
        }
    }
}

/// Empty names suppress the author label, including its account decoration.
#[must_use]
pub fn author_label(name: &str, account: &str, support: bool) -> String {
    if name.is_empty() {
        String::new()
    } else if support {
        format!("- {name} <{account}>")
    } else {
        format!("- {name}")
    }
}

#[must_use]
pub fn text_is_blank(text: &str) -> bool {
    text.bytes().all(|b| matches!(b, b' ' | b'\n' | 0))
}
