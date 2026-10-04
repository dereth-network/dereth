//! The server's open book and the shared editing cursor, draft and pending requests.

use crate::world::World;
use crate::{Notice, NoticeSink};
use dereth_client_contract::book::{text_is_blank, BookAction, BookSessionView};
use dereth_client_contract::UiRequest;
use dereth_primitives::ObjectId;
use dereth_protocol::trade::{PageData, PageDataList};

/// One open book: its object id, page limits and data, followed by the three fields copied after
/// the panel selects the current page.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OpenBook {
    /// Book id.
    pub book_id: ObjectId,
    /// Maximum pages: the **second dword of the message**,
    /// not `PageDataList`'s own maximum. Its current-page range test and next-button enable both read this one.
    pub max_num_pages: u32,
    /// Whole page-data list. Its `max_num_pages` and
    /// `max_num_chars_per_page` are kept as they arrived even though the panel reads neither.
    pub pages: PageDataList,
    /// Inscription.
    pub inscription: String,
    /// Scribe id.
    pub scribe_id: ObjectId,
    /// Scribe name.
    pub scribe_name: String,
}

impl OpenBook {
    /// Read page `n` from the page-data list. `None` means the page is past the end, which is
    /// **not** the same as past `max_num_pages`.
    #[must_use]
    pub fn page(&self, n: i32) -> Option<&PageData> {
        usize::try_from(n)
            .ok()
            .and_then(|n| self.pages.pages.get(n))
    }

    /// How many pages the server actually sent, which can be fewer than
    /// [`Self::max_num_pages`].
    #[must_use]
    pub fn num_pages(&self) -> i32 {
        i32::try_from(self.pages.pages.len()).unwrap_or(i32::MAX)
    }

    /// The client's editability test: `page.author_id == player || page.ignore_author`. The authoring panel
    /// uses this named condition to decide whether its text and page operations are enabled.
    #[must_use]
    pub fn editable_by(&self, page: &PageData, player: ObjectId) -> bool {
        page.author_id == player || page.ignore_author != 0
    }
}

/// One shared book session. Receipt counters distinguish repeated deliveries.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BookState {
    pub open: Option<OpenBook>,
    pub opening: u64,
    pub page_data_applied: u64,
    pub page_data_ignored: u64,
    pub add_page: Option<AddedPage>,
    pub add_page_responses: u64,
    pub add_page_ignored: u64,
    pub delete_page_responses: u64,
    pub session: BookSessionView,
    dirty: bool,
    saved: bool,
    requests: Vec<UiRequest>,
    discarded_pages: std::collections::BTreeSet<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AddedPage {
    pub page: u32,
    pub success: bool,
}

impl BookState {
    /// Discard an externally closed book without inventing a page save.
    pub fn close(&mut self) {
        self.open = None;
        self.session = BookSessionView {
            revision: self.session.revision.wrapping_add(1),
            ..Default::default()
        };
        self.dirty = false;
        self.saved = false;
    }

    fn changed(&mut self) {
        self.session.revision = self.session.revision.wrapping_add(1);
    }

    fn display(&mut self, player: ObjectId) {
        let page = self
            .open
            .as_ref()
            .and_then(|b| b.page(self.session.current_page));
        self.session.draft = page.and_then(|p| p.page_text.clone()).unwrap_or_default();
        self.session.editable = page.is_some_and(|p| p.author_id == player || p.ignore_author != 0);
        self.dirty = false;
        self.saved = false;
        self.changed();
    }

    fn save(&mut self, player: ObjectId) -> bool {
        if self.session.pending || self.saved {
            return false;
        }
        let Some(b) = self.open.as_mut() else {
            return false;
        };
        let Ok(index) = usize::try_from(self.session.current_page) else {
            return false;
        };
        let Some(p) = b.pages.pages.get_mut(index) else {
            return false;
        };
        if p.author_id != player && p.ignore_author == 0 {
            return false;
        }
        let Ok(page) = u32::try_from(index) else {
            return false;
        };
        if p.author_id == player && text_is_blank(&self.session.draft) {
            self.requests.push(UiRequest::BookDeletePage {
                book: b.book_id,
                page,
            });
            self.saved = true;
            self.dirty = false;
            b.pages.pages.remove(index);
            self.discarded_pages.insert(page);
            self.session.editable = false;
            self.changed();
            true
        } else {
            p.page_text = Some(self.session.draft.clone());
            self.requests.push(UiRequest::BookModifyPage {
                book: b.book_id,
                page,
                text: self.session.draft.clone(),
            });
            self.saved = true;
            self.dirty = false;
            false
        }
    }

    fn turn(&mut self, mut page: i32, player: ObjectId, name: &str) {
        let Some(b) = self.open.as_ref() else {
            return;
        };
        if self.session.pending
            || page < 0
            || page as u32 >= b.max_num_pages
            || page == self.session.current_page
        {
            return;
        }
        let count = b.num_pages();
        let old = self.session.current_page;
        if page > count
            || (old == count - 1
                && old != -1
                && page > old
                && text_is_blank(&self.session.draft)
                && b.page(old).is_some_and(|p| p.author_id == player))
        {
            self.requests.push(UiRequest::DisplayChatText {
                channel: 0x1a,
                text: format!("The {name} is already open to a blank page"),
            });
            return;
        }
        if self.save(player) && page > old {
            page -= 1;
        }
        self.session.current_page = page;
        self.session.page_turns = self.session.page_turns.wrapping_add(1);
        self.saved = false;
        self.dirty = false;
        let b = self.open.as_ref().expect("open book");
        match b.page(page) {
            Some(p) if p.text_included != 0 => self.display(player),
            p => {
                if p.is_some() {
                    self.discarded_pages.remove(&(page as u32));
                }
                self.requests.push(if p.is_some() {
                    UiRequest::BookPageData {
                        book: b.book_id,
                        page: page as u32,
                    }
                } else {
                    UiRequest::BookAddPage { book: b.book_id }
                });
                self.session.draft.clear();
                self.session.editable = false;
                self.session.pending = true;
                self.changed();
            }
        }
    }
}

impl World {
    #[must_use]
    pub fn book_session_view(&self) -> BookSessionView {
        self.book.session.clone()
    }

    pub fn take_book_requests(&mut self) -> Vec<UiRequest> {
        std::mem::take(&mut self.book.requests)
    }

    pub fn book_action(&mut self, action: BookAction) {
        let book = match &action {
            BookAction::Edit { book, .. }
            | BookAction::Turn { book, .. }
            | BookAction::Flush { book }
            | BookAction::Close { book } => *book,
        };
        if !self.book.open.as_ref().is_some_and(|b| b.book_id == book) {
            return;
        }
        let player = self.player.unwrap_or_default();
        match action {
            BookAction::Edit { page, text, .. } => {
                if page == self.book.session.current_page
                    && !self.book.session.pending
                    && self.book.session.editable
                    && text != self.book.session.draft
                {
                    self.book.session.draft = text;
                    self.book.dirty = true;
                    self.book.saved = false;
                    self.book.changed();
                }
            }
            BookAction::Turn { page, .. } => {
                let name = self
                    .weenie(book)
                    .map(|w| {
                        w.display_name(
                            crate::weenie::NameType::Appropriate,
                            self.material_name(w.pwd.material_type.unwrap_or(0)),
                        )
                    })
                    .unwrap_or_default();
                let revision = self.book.session.revision;
                let current = self.book.session.current_page;
                self.book.turn(page, player, &name);
                if page != current && self.book.session.revision == revision {
                    self.book.changed();
                }
            }
            BookAction::Flush { .. } => {
                if self.book.save(player) {
                    let count = self.book.open.as_ref().expect("open book").num_pages();
                    let page = self.book.session.current_page.min(count - 1).max(0);
                    self.book.session.current_page = -1;
                    self.book.turn(page, player, "");
                }
            }
            BookAction::Close { .. } => {
                self.book.save(player);
                self.book.requests.push(UiRequest::UnregisterBookRange);
                self.book.close();
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn open_book(
        &mut self,
        book_id: ObjectId,
        max_num_pages: u32,
        pages: PageDataList,
        inscription: String,
        scribe_id: ObjectId,
        scribe_name: String,
        out: &mut dyn NoticeSink,
        now: dereth_primitives::ServerTime,
    ) {
        let same = self
            .book
            .open
            .as_ref()
            .is_some_and(|b| b.book_id == book_id);
        let remembered = if same {
            self.book.session.current_page
        } else {
            0
        };
        self.book.save(self.player.unwrap_or_default());
        self.book.open = Some(OpenBook {
            book_id,
            max_num_pages,
            pages,
            inscription,
            scribe_id,
            scribe_name,
        });
        let count = self.book.open.as_ref().expect("open book").num_pages();
        self.book.session = BookSessionView {
            revision: self.book.session.revision.wrapping_add(1),
            ..Default::default()
        };
        self.book.saved = false;
        self.book.discarded_pages.clear();
        self.book.turn(
            remembered.min(count - 1).max(0),
            self.player.unwrap_or_default(),
            "",
        );
        self.book.opening = self.book.opening.wrapping_add(1);
        self.register_book_range_check(book_id, now);
        out.emit(Notice::OpenBook(book_id));
    }

    pub fn book_page_data_response(&mut self, object: ObjectId, page: u32, data: PageData) -> bool {
        if self.book.discarded_pages.contains(&page) {
            self.book.page_data_ignored += 1;
            return false;
        }
        let Some(b) = self.book.open.as_mut().filter(|b| b.book_id == object) else {
            self.book.page_data_ignored += 1;
            return false;
        };
        let Some(slot) = b.pages.pages.get_mut(page as usize) else {
            self.book.page_data_ignored += 1;
            return false;
        };
        *slot = data;
        self.book.page_data_applied += 1;
        self.book.session.pending = false;
        if i32::try_from(page).ok() == Some(self.book.session.current_page) {
            self.book.display(self.player.unwrap_or_default());
        } else {
            self.book.changed();
        }
        true
    }

    pub fn book_add_page_response(&mut self, object: ObjectId, page: u32, success: bool) -> bool {
        if object == ObjectId(0) || !self.book.open.as_ref().is_some_and(|b| b.book_id == object) {
            self.book.add_page_ignored += 1;
            return false;
        }
        self.book.add_page = Some(AddedPage { page, success });
        self.book.add_page_responses += 1;
        if !success || i32::try_from(page).unwrap_or(i32::MAX) != self.book.session.current_page {
            if success {
                self.book.session.current_page = i32::try_from(page).unwrap_or(i32::MAX);
            }
            self.book
                .requests
                .push(UiRequest::BookData { book: object });
            self.book.session.book_data_refetches += 1;
            self.book.changed();
            return true;
        }
        let author = self.player.unwrap_or_default();
        let name = self
            .weenie(author)
            .map(|w| w.pwd.name.clone())
            .unwrap_or_default();
        let b = self.book.open.as_mut().expect("open book");
        b.pages.pages.insert(
            (page as usize).min(b.pages.pages.len()),
            PageData {
                author_id: author,
                author_name: name,
                ..Default::default()
            },
        );
        self.book.session.pending = false;
        self.book.session.pages_added += 1;
        self.book.display(author);
        true
    }

    /// Delete responses acknowledge the request without another local removal.
    pub fn book_delete_page_response(&mut self) {
        self.book.delete_page_responses += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open(world: &mut World, id: u32, count: usize, outer: u32) {
        world.open_book(
            ObjectId(id),
            outer,
            PageDataList {
                max_num_pages: 99,
                pages: (0..count)
                    .map(|n| PageData {
                        author_id: ObjectId(7),
                        page_text: Some(format!("Page {n}")),
                        ..Default::default()
                    })
                    .collect(),
                ..Default::default()
            },
            String::new(),
            ObjectId(0),
            String::new(),
            &mut crate::NullSink,
            dereth_primitives::ServerTime(0.0),
        );
    }

    /// Behaviour: reader.book.shared-session-receipts-and-drafts
    #[test]
    fn session_guards_receipts_and_saves_in_order() {
        let mut world = World::new();
        world.player = Some(ObjectId(7));
        open(&mut world, 5, 3, 4);
        assert!(world.take_book_requests().is_empty());
        let initial = world.book_session_view();
        world.book_action(BookAction::Turn {
            book: ObjectId(5),
            page: initial.current_page,
        });
        assert_eq!(
            world.book_session_view(),
            initial,
            "an unchanged selection does not invalidate the displayed page menu"
        );
        assert!(world.take_book_requests().is_empty());
        world.book_action(BookAction::Turn {
            book: ObjectId(5),
            page: 2,
        });
        assert!(matches!(
            world.take_book_requests().as_slice(),
            [UiRequest::BookModifyPage { page: 0, .. }]
        ));
        open(&mut world, 5, 1, 2);
        assert_eq!(
            world.book_session_view().current_page,
            0,
            "reopening clamps a remembered page to the received list"
        );
        world.take_book_requests();
        world.book_action(BookAction::Turn {
            book: ObjectId(5),
            page: 2,
        });
        assert!(
            world.take_book_requests().is_empty(),
            "the outer limit wins over the embedded maximum"
        );
        world.book_action(BookAction::Turn {
            book: ObjectId(5),
            page: 1,
        });
        assert!(matches!(
            world.take_book_requests().as_slice(),
            [
                UiRequest::BookModifyPage { page: 0, .. },
                UiRequest::BookAddPage { .. }
            ]
        ));
        assert!(world.book_session_view().pending);
        assert!(!world.book_add_page_response(ObjectId(6), 1, true));
        assert!(world.book_session_view().pending);
        assert!(world.book_add_page_response(ObjectId(5), 1, true));
        assert!(world.book_session_view().editable);
        world.book_action(BookAction::Edit {
            book: ObjectId(6),
            page: 1,
            text: "Stale".into(),
        });
        assert_eq!(world.book_session_view().draft, "");
        world.book_action(BookAction::Edit {
            book: ObjectId(5),
            page: 1,
            text: "Saved once".into(),
        });
        world.book_action(BookAction::Flush { book: ObjectId(5) });
        world.book_action(BookAction::Flush { book: ObjectId(5) });
        world.book_action(BookAction::Close { book: ObjectId(5) });
        assert!(
            matches!(world.take_book_requests().as_slice(), [UiRequest::BookModifyPage { page: 1, text, .. }, UiRequest::UnregisterBookRange] if text == "Saved once")
        );

        open(&mut world, 5, 2, 4);
        world.book_action(BookAction::Edit {
            book: ObjectId(5),
            page: 0,
            text: "Draft survives another page reply".into(),
        });
        assert!(world.book_page_data_response(ObjectId(5), 1, PageData::default()));
        assert_eq!(
            world.book_session_view().draft,
            "Draft survives another page reply"
        );
        assert!(!world.book_page_data_response(ObjectId(5), 9, PageData::default()));
        let before_refusal = world.book_session_view().revision;
        world.book_action(BookAction::Turn {
            book: ObjectId(5),
            page: 3,
        });
        assert_ne!(
            world.book_session_view().revision,
            before_refusal,
            "a refused gesture refreshes the displayed menu selection"
        );
        assert!(
            matches!(world.take_book_requests().as_slice(), [UiRequest::DisplayChatText { channel: 0x1a, text }] if text == "The  is already open to a blank page")
        );
        world.book_action(BookAction::Turn {
            book: ObjectId(5),
            page: 2,
        });
        world.take_book_requests();
        world.book_add_page_response(ObjectId(5), 1, true);
        assert_eq!(world.book_session_view().current_page, 1);
        assert!(matches!(
            world.take_book_requests().as_slice(),
            [UiRequest::BookData { book: ObjectId(5) }]
        ));
        let before = world.book.open.clone();
        world.book_delete_page_response();
        assert_eq!(world.book.open, before);

        open(&mut world, 8, 1, 2);
        assert_eq!(world.book_session_view().current_page, 0);
        world.take_book_requests();
        world.book_action(BookAction::Close { book: ObjectId(8) });
        assert!(
            matches!(
                world.take_book_requests().as_slice(),
                [
                    UiRequest::BookModifyPage { .. },
                    UiRequest::UnregisterBookRange
                ]
            ),
            "an initial untouched editable close still saves"
        );
        for (text, deleted) in [(" \n\0", true), ("\t", false), ("\r", false)] {
            open(&mut world, 8, 1, 2);
            world.book_action(BookAction::Edit {
                book: ObjectId(8),
                page: 0,
                text: text.into(),
            });
            world.book_action(BookAction::Close { book: ObjectId(8) });
            let requests = world.take_book_requests();
            assert_eq!(
                matches!(requests.first(), Some(UiRequest::BookDeletePage { .. })),
                deleted
            );
        }
        open(&mut world, 8, 1, 2);
        world.book_page_data_response(
            ObjectId(8),
            0,
            PageData {
                author_id: ObjectId(99),
                ..Default::default()
            },
        );
        assert!(!world.book_session_view().editable);
        world.book_action(BookAction::Edit {
            book: ObjectId(8),
            page: 0,
            text: "Denied".into(),
        });
        world.book_action(BookAction::Close { book: ObjectId(8) });
        assert_eq!(world.take_book_requests(), [UiRequest::UnregisterBookRange]);

        open(&mut world, 8, 1, 2);
        world.book_action(BookAction::Edit {
            book: ObjectId(8),
            page: 0,
            text: String::new(),
        });
        world.book_action(BookAction::Flush { book: ObjectId(8) });
        assert!(matches!(
            world.take_book_requests().as_slice(),
            [
                UiRequest::BookDeletePage { page: 0, .. },
                UiRequest::BookAddPage { .. }
            ]
        ));
        world.book_action(BookAction::Flush { book: ObjectId(8) });
        assert!(
            world.take_book_requests().is_empty(),
            "another interface change while waiting adds nothing"
        );
        world.book_add_page_response(ObjectId(8), 0, true);
        world.book_action(BookAction::Edit {
            book: ObjectId(8),
            page: 0,
            text: "Replacement page".into(),
        });
        assert!(!world.book_page_data_response(
            ObjectId(8),
            0,
            PageData {
                page_text: Some("Deleted page reply".into()),
                ..Default::default()
            }
        ));
        assert_eq!(world.book_session_view().draft, "Replacement page");
        open(&mut world, 9, 2, 3);
        world.take_book_requests();
        world.book.open.as_mut().unwrap().pages.pages[1].text_included = 0;
        world.book_action(BookAction::Turn {
            book: ObjectId(9),
            page: 1,
        });
        assert!(world.book_session_view().pending);
        world.take_book_requests();
        assert!(world.book_page_data_response(
            ObjectId(9),
            0,
            PageData {
                page_text: Some("Earlier page reply".into()),
                ..Default::default()
            }
        ));
        let state = world.book_session_view();
        assert!(
            !state.pending,
            "any accepted page-data reply clears the request latch"
        );
        assert_eq!(state.current_page, 1);
        assert!(
            state.draft.is_empty() && !state.editable,
            "an off-page reply does not display that page"
        );
    }
}
