//! The readable-book model: the book the server opened and the pages it carries.
//!
//! This is the receiver for **`0x00B4 Writing_BookOpen`**: without it, using scrolls, letters
//! or signs opens no readable text. `dereth_protocol::trade::WritingBookOpen` decodes it (a
//! recorded message carries twelve pages) and `dereth_client_net::client_session` orders it onto
//! the UI queue.
//!
//! # Where the state lives, and why it is here rather than in the panel
//!
//! In the client it is state of panel `0x10000019` (page `0x10000182` of `<PANS>`): the panel keeps
//! the book id, maximum-page count, page-data list, current page, inscription, scribe id, and
//! scribe name, and UI queue arm `0xB4` reaches it by notice
//! (the open-book notice → the book panel's handler for it → its own open-book).
//!
//! This build's `dereth-ui-screens` cannot see the wire, so the same split every other panel here
//! already has applies: the message lands in `dereth_client_model::World`, the panel **pulls** it through
//! `GameView` and applies the open-book behavior on the frame that first sees a new [`BookState::opening`].
//! That is the shape `ExaminationPanel` and `VendorPanel` both use, and
//! the one difference from the client is the same one: the client's handler runs on the notice,
//! this one on the next frame.
//!
//! # Model and panel boundary
//!
//! This module models the server-authoritative path — open, the page list, the page text,
//! the title, and `0x00B8 Writing_BookPageDataResponse` filling in a page whose text the server
//! withheld.
//!
//! The authoring cursor and editable text live in `dereth-ui-screens::BookPanel`, matching
//! retail's book panel, which sends typed Add/Modify/Delete/PageData/BookData requests back through
//! `dereth_client_model::Request`. The world changes only when the authoritative `0x00B6`/`0x00B8` arrives;
//! the one deliberate native-local exception is the panel's blank-page removal before `0x00B7`.

use crate::world::World;
use crate::{Notice, NoticeSink};
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

/// `World`'s book slot. One book at a time, because `BookPanel` is one panel with one book id and
/// the open-book handler's first act is on whatever was there.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BookState {
    /// The book `0x00B4` opened, or `None` — `book_id == 0` in the client.
    pub open: Option<OpenBook>,
    /// How many `0x00B4` have been applied. **The panel's edge.**
    ///
    /// A second `0x00B4` for the *same* book is a real event in the client — the open-book
    /// handler runs whole, the page strip is rebuilt and the page the player was on is restored — so
    /// "the book changed" cannot be `book_id`-equality. This counter is what
    /// `BookPanel::update` compares, the same denominator `AppraisalCache::delivery` is for the
    /// examination panel.
    pub opening: u64,
    /// How many `0x00B8 Writing_BookPageDataResponse` have been applied to a page of the open
    /// book. A denominator: "no reply arrived" and "a reply arrived for a book we do not have" are
    /// different answers, and [`BookState::page_data_ignored`] is the other half.
    pub page_data_applied: u64,
    /// `0x00B8` replies whose object id was not the open book, or whose page number is past the
    /// list. The page-data response handler drops both.
    pub page_data_ignored: u64,

    // ---- the authoring reply -------------------------------------------------------------------
    /// The `0x00B6 Writing_BookAddPageResponse` the panel has not run yet, or `None`.
    ///
    /// The message is relayed rather than applied because **current page is panel state**, as is
    /// the page list the insert goes into; see
    /// the book add-page response path for the response split.
    pub add_page: Option<AddedPage>,
    /// How many `0x00B6` got past the two refusals makes
    /// before it looks at anything — **the panel's edge**, the same shape as [`Self::opening`].
    pub add_page_responses: u64,
    /// `0x00B6` for an object that is not the open book, or with no book open at all
    /// (`object_id != book_id`, `book_id == 0`) — the handler's two early returns.
    pub add_page_ignored: u64,
    /// `0x00B7 Writing_BookDeletePageResponse` arrivals. **A counter and nothing else**, because
    /// every registered native handler is a no-op for this response.
    pub delete_page_responses: u64,
}

/// One `0x00B6 Writing_BookAddPageResponse`, past the model's two refusals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AddedPage {
    /// The message's second dword. The add-page response's third test compares it with
    /// the current page, which is why this reaches the panel unjudged.
    pub page: u32,
    /// The message's third dword. `success == 0` is the server refusing the page.
    pub success: bool,
}

impl BookState {
    /// The book panel's close, minus the two requests retail's page close would send —
    /// see the module doc.
    pub fn close(&mut self) {
        self.open = None;
    }
}

impl World {
    /// The book panel's open-book **model** half: the event dispatcher's `case 0xb4`.
    ///
    /// The client's handler is one sequence and this owns its first third. It shows the panel;
    /// if the book id equals the new id it remembers the current page; if an old book is open it
    /// closes it; it stores the book id, the page count (the message's **second** dword) and the
    /// page list; then it updates the menu, sets the current page, stores the scribe id,
    /// inscription and scribe name, and sets the title text.
    ///
    /// Everything from the menu update down is the panel's and lives in
    /// `dereth_ui_screens::panels::book`. The panel learns a book arrived from
    /// [`BookState::opening`] rather than from the notice, because a `dereth-ui-screens` panel is
    /// pulled and not pushed — the same one difference `ExaminationPanel` has.
    ///
    /// **Showing the panel is unconditional and comes first**: the client shows
    /// `0x10000182` on every `0x00B4`, before it has looked at a single page, so a book with no
    /// pages still opens an empty window rather than nothing happening.
    #[allow(clippy::too_many_arguments)] // one parameter per input the call takes
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
        self.book.open = Some(OpenBook {
            book_id,
            max_num_pages,
            pages,
            inscription,
            scribe_id,
            scribe_name,
        });
        self.book.opening += 1;
        self.register_book_range_check(book_id, now);
        out.emit(Notice::OpenBook(book_id));
    }

    /// The book panel's page-data response — `0x00B8`'s model half.
    ///
    /// The client's guard is `id == book_id && pages != null`, then
    /// the page-data lookup is bounds checked; a reply naming a missing page is dropped.
    /// Both refusals are **counted** rather than swallowed, so "no reply came" and "a reply came
    /// for the wrong book" are different answers.
    ///
    /// Returns `true` when the page was filled, which is what tells the panel to redraw.
    pub fn book_page_data_response(&mut self, object: ObjectId, page: u32, data: PageData) -> bool {
        let Some(b) = self.book.open.as_mut() else {
            self.book.page_data_ignored += 1;
            return false;
        };
        if b.book_id != object {
            self.book.page_data_ignored += 1;
            return false;
        }
        let Some(slot) = usize::try_from(page)
            .ok()
            .and_then(|n| b.pages.pages.get_mut(n))
        else {
            self.book.page_data_ignored += 1;
            return false;
        };
        *slot = data;
        self.book.page_data_applied += 1;
        true
    }

    /// The book panel's add-page response — `0x00B6`'s **model** third.
    ///
    /// Retail does nothing at all unless the object id equals the open book id and that id is
    /// non-zero. Then, if `success` is zero, it asks for the book again (`Request::BookData`) and
    /// nothing else. If the page is not the current page it turns to it and asks for the book.
    /// Otherwise it builds a page (the player's id and name, empty text, no flags), inserts it at
    /// the requested page, formats `"- %hs"` with the new page's author name, adds the page
    /// metadata to the book UI and displays it.
    ///
    /// **Only the first two tests are here.** Everything from `success` down reads or writes the
    /// panel's current-page and page-list state.
    /// On this side of the seam those values belong to
    /// `dereth_ui_screens::panels::book::BookPanel`, so the message is
    /// relayed to the panel exactly as `0x00B4` and `0x00B8` are. Splitting it anywhere else would
    /// put the current page in two places.
    ///
    /// `0x00B6` is ACE's answer to `0x00B5 Writing_BookAddPage`, which the page setter sends when
    /// the scribe pages past the end of their own book. The authoring panel now produces that
    /// request; this receiver keeps the authoritative response separate from the panel-local blank
    /// page it replaces.
    ///
    /// Returns whether the message got past the two refusals.
    pub fn book_add_page_response(&mut self, object: ObjectId, page: u32, success: bool) -> bool {
        let ours = self.book.open.as_ref().is_some_and(|b| b.book_id == object);
        if !ours {
            self.book.add_page_ignored += 1;
            return false;
        }
        self.book.add_page = Some(AddedPage { page, success });
        self.book.add_page_responses += 1;
        true
    }

    /// `0x00B7 Writing_BookDeletePageResponse` — **a counter, because retail's receiver is an
    /// empty stub.**
    ///
    /// The event dispatcher's `case 0xb7` reads the same three dwords `case 0xb6` does and raises
    /// the delete-page-response notice, whose receiver is an empty stub in every UI element that
    /// has one; nothing in retail overrides it.
    ///
    /// The add-page response is the counter-example that makes this a measurement rather than an
    /// assumption: it has the same empty default everywhere except the book panel, which
    /// implements it. So retail *can* receive the add and genuinely ignores the delete. An arm here that removed a page would be
    /// inventing behaviour.
    pub fn book_delete_page_response(&mut self) {
        self.book.delete_page_responses += 1;
    }
}
