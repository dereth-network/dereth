//! Readable-book widgets project the shared session and forward editing actions.

use dereth_client_contract::book::{BookAction, BookSessionView};
use dereth_ui::{ElemHandle, ElementId, StateId, UiSystem};

use crate::view::{BookPageView, GameView, UiRequest};

/// `<BOOK>` — the `BookPanel` page of the panel stack, `0x10000182`.
pub const WINDOW: ElementId = ElementId(0x1000_0182);

/// The title text — the book panel's post-init's first child lookup.
pub const TITLE_TEXT: ElementId = ElementId(0x1000_010F);
/// The page text.
pub const PAGE_TEXT: ElementId = ElementId(0x1000_0111);
/// The previous-page button.
pub const PREV_BUTTON: ElementId = ElementId(0x1000_0114);
/// The next-page button.
pub const NEXT_BUTTON: ElementId = ElementId(0x1000_0115);
/// The page menu.
pub const PAGE_MENU: ElementId = ElementId(0x1000_0470);
/// The `"Page %d"` label nested in each runtime page-menu row.
pub const PAGE_MENU_ROW_NUMBER: ElementId = ElementId(0x1000_0479);
/// The menu-selection page-number text, bound off the menu and not off the window.
pub const MENU_SELECTION_PAGE_NUM_TEXT: ElementId = ElementId(0x1000_047B);

/// The state given to a paging button that can be pressed: 1.
pub const STATE_NORMAL: StateId = StateId(1);
/// The state given to one that cannot: `0x0D`.
pub const STATE_UNAVAILABLE: StateId = StateId(0x0D);

#[must_use]
pub fn page_menu_label(page: &BookPageView, is_psr: bool) -> String {
    dereth_client_contract::book::author_label(&page.author_name, &page.author_account, is_psr)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (book widget projection and action forwarding; session rules are tested in the model).
    use super::*;
    use dereth_primitives::ObjectId;

    #[derive(Debug)]
    struct View;
    impl GameView for View {
        fn open_book(&self) -> Option<crate::view::BookView> {
            Some(crate::view::BookView {
                book_id: ObjectId(5),
                max_num_pages: 3,
                opening: 1,
                viewer_is_psr: true,
                pages: vec![
                    BookPageView {
                        author_name: "Writer".into(),
                        ignore_author: 1,
                        text: Some("Cached page".into()),
                        ..Default::default()
                    },
                    BookPageView {
                        author_account: "Hidden".into(),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            })
        }
        fn book_session(&self) -> BookSessionView {
            BookSessionView {
                current_page: 1,
                draft: "Shared draft".into(),
                revision: 9,
                ..Default::default()
            }
        }
    }

    /// Behaviour: reader.book.author-labels-follow-the-name
    #[test]
    fn author_labels_ignore_editability_flags_and_hide_empty_names() {
        #[derive(Debug)]
        struct Authors(bool);
        impl GameView for Authors {
            fn open_book(&self) -> Option<crate::view::BookView> {
                Some(crate::view::BookView {
                    book_id: ObjectId(5),
                    max_num_pages: 2,
                    opening: 1,
                    viewer_is_psr: self.0,
                    pages: vec![
                        BookPageView {
                            author_name: "Writer".into(),
                            author_account: "Account".into(),
                            ignore_author: 1,
                            ..Default::default()
                        },
                        BookPageView {
                            author_account: "Hidden".into(),
                            ignore_author: 0,
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                })
            }
        }
        for support in [false, true] {
            let mut ui = UiSystem::new((800, 600));
            let mut panel = BookPanel::default();
            panel.update(&mut ui, &Authors(support));
            assert_eq!(
                panel.menu_labels,
                [
                    if support {
                        "- Writer <Account>"
                    } else {
                        "- Writer"
                    },
                    ""
                ]
            );
            assert!(ui.requests.take().is_empty());
        }
    }

    #[test]
    fn reader_projects_shared_cursor_draft_and_labels_without_replaying_receipts() {
        let mut ui = UiSystem::new((800, 600));
        let mut panel = BookPanel::default();
        assert!(panel.update(&mut ui, &View));
        assert_eq!(
            (panel.cur_page, panel.page_body.as_str()),
            (1, "Shared draft")
        );
        assert!(!panel.editable);
        assert_eq!(panel.menu_labels, ["- Writer <>", "", "(blank)"]);
        assert!(
            ui.requests.take().is_empty(),
            "projecting an opened book must not send another add or page request"
        );
        panel.set_cur_page(&mut ui, 0);
        assert_eq!(
            ui.requests.take(),
            [UiRequest::Book(BookAction::Turn {
                book: ObjectId(5),
                page: 0
            })]
        );
        assert_eq!(panel.cur_page, 1, "the renderer waits for shared state");
        assert!(!panel.update(&mut ui, &View));
        assert!(ui.requests.take().is_empty());
    }
}

/// The padding label the page-menu update gives every row from the page count up to the page limit.
pub const BLANK_PAGE_LABEL: &str = "(blank)";

/// The page-label format used when setting the current page.
pub const ALREADY_OPEN_TO_BLANK_PAGE: &str = "The %s is already open to a blank page";

/// `BookPanel`.
#[derive(Debug, Default)]
pub struct BookPanel {
    /// `<BOOK>` itself; `SetVisible` is called on this.
    pub window: Option<ElemHandle>,
    title: Option<ElemHandle>,
    page_text: Option<ElemHandle>,
    prev_button: Option<ElemHandle>,
    next_button: Option<ElemHandle>,
    page_menu: Option<ElemHandle>,
    menu_selection_page_num: Option<ElemHandle>,

    /// The open book's id — `None` is the client's `0`.
    pub book_id: Option<dereth_primitives::ObjectId>,
    /// The player's own id, projected with the open-book view for the editability test.
    pub player_id: dereth_primitives::ObjectId,
    /// Whether the viewer is a PSR, used by the page-menu update.
    pub viewer_is_psr: bool,
    /// The book's appropriate name (`NAME_APPROPRIATE = 2`), retained separately from the title.
    pub object_name: String,
    /// The message-header page limit, kept separately from the page list's limit pair.
    pub max_num_pages: i32,
    /// The current page. Its original initial value is **-1**, which lets the first request for
    /// page 0 pass the unchanged-page guard.
    pub cur_page: i32,
    /// The pages as the last `0x00B4`/`0x00B8` left them.
    pub pages: Vec<BookPageView>,
    /// The strip the page-menu update built, in row order — the labels rather than the elements, so a test
    /// can read the pane back without walking a menu whose rows this build does not construct.
    pub menu_labels: Vec<String>,
    /// What the title text was last set to.
    pub title_text: String,
    /// What was last written into the page text.
    pub page_body: String,
    /// What last wrote into
    /// the menu-selection page-number text -- the 1-based page number on the strip.
    pub menu_selection_text: String,
    /// Whether the page text is currently editable.
    pub editable: bool,
    /// Request pending: a page with no text was asked for and
    /// the answer has not come. It gates `set_cur_page` whole.
    pub request_pending: bool,

    /// How many books this panel has opened — the denominator that separates "the panel never
    /// opened" from "it opened and drew nothing".
    pub opens: u32,
    /// [`crate::view::BookView::opening`] as of the last open, so the pull can tell a new book from the same
    /// one being offered again.
    last_opening: u64,
    session: BookSessionView,
    /// How many pages have been inserted into [`Self::pages`].
    pub pages_added: u32,
    /// `0x00B6`s that ended in a whole-book request ([`UiRequest::BookData`]) instead of an insert: the
    /// `success == 0` arm and the page-is-not-current arm.
    ///
    /// Each count is one whole-book request (`0x00AA`).
    pub book_data_refetches: u32,
    /// How many `set_cur_page` calls actually moved the page.
    pub page_turns: u32,
}

impl BookPanel {
    /// The book panel's post-init, off the gameplay screen root.
    ///
    /// The client binds five children off the panel and the sixth (the menu-selection page-number
    /// text) off the **menu**, only when the menu exists. Kept in that shape so a layout that lost the menu
    /// loses exactly the one child the client would.
    pub fn post_init(&mut self, ui: &mut UiSystem, root: ElemHandle) {
        self.window = ui.get_child_recursive(root, WINDOW);
        let Some(w) = self.window else { return };
        self.title = ui.get_child_recursive(w, TITLE_TEXT);
        self.page_text = ui.get_child_recursive(w, PAGE_TEXT);
        self.prev_button = ui.get_child_recursive(w, PREV_BUTTON);
        self.next_button = ui.get_child_recursive(w, NEXT_BUTTON);
        self.page_menu = ui.get_child_recursive(w, PAGE_MENU);
        self.menu_selection_page_num = self
            .page_menu
            .and_then(|m| ui.get_child_recursive(m, MENU_SELECTION_PAGE_NUM_TEXT));
        // The book panel's constructor's last block.
        self.book_id = None;
        self.player_id = dereth_primitives::ObjectId(0);
        self.viewer_is_psr = false;
        self.object_name.clear();
        self.max_num_pages = 0;
        self.cur_page = -1;
        self.request_pending = false;
    }

    /// Whether `post_init` found the window at all.
    #[must_use]
    pub fn bound(&self) -> bool {
        self.window.is_some()
    }

    /// Whether every one of the six children post-init binds is in the shipped layout.
    #[must_use]
    pub fn fully_bound(&self) -> bool {
        self.window.is_some()
            && self.title.is_some()
            && self.page_text.is_some()
            && self.prev_button.is_some()
            && self.next_button.is_some()
            && self.page_menu.is_some()
            && self.menu_selection_page_num.is_some()
    }

    /// The frame's pull — the open-book notice and
    /// the book-page-data-response notice, on the far side of the seam.
    ///
    /// Returns `true` when anything was written, which is the frame's own denominator.
    pub fn update(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> bool {
        let Some(b) = view.open_book() else {
            self.book_id = None;
            return false;
        };
        self.capture_draft(ui);
        let state = view.book_session();
        let opened = self.book_id != Some(b.book_id) || self.last_opening != b.opening;
        if !opened && self.session == state {
            return false;
        }
        self.book_id = Some(b.book_id);
        self.player_id = b.player_id;
        self.viewer_is_psr = b.viewer_is_psr;
        self.object_name = b.object_name.clone();
        self.max_num_pages = i32::try_from(b.max_num_pages).unwrap_or(i32::MAX);
        self.pages = b.pages.clone();
        self.cur_page = state.current_page;
        self.request_pending = state.pending;
        self.page_turns = state.page_turns;
        self.pages_added = state.pages_added;
        self.book_data_refetches = state.book_data_refetches;
        self.title_text = b.title.clone();
        self.last_opening = b.opening;
        if opened {
            self.opens += 1;
            if let Some(w) = self.window {
                ui.set_visible(w, true);
            }
        }
        if let Some(t) = self.title.and_then(|h| ui.text_element_mut(h)) {
            t.set_text(&self.title_text);
        }
        self.set_page_text(ui, &state.draft, state.editable && !state.pending);
        self.session = state;
        self.update_menu(ui);
        self.menu_selection_text = format!("{}", self.cur_page + 1);
        if let Some(t) = self
            .menu_selection_page_num
            .and_then(|h| ui.text_element_mut(h))
        {
            t.set_text(&self.menu_selection_text);
        }
        self.select_current_menu_page(ui);
        self.update_paging_buttons(ui);
        true
    }

    /// Capture edits before another action or interface consumes the shared draft.
    pub fn capture_draft(&mut self, ui: &mut UiSystem) {
        let Some(book) = self.book_id else {
            return;
        };
        if !self.editable || self.request_pending {
            return;
        }
        let text = self.page_text_value(ui);
        if text != self.page_body {
            self.page_body = text.clone();
            ui.requests.emit(UiRequest::Book(BookAction::Edit {
                book,
                page: self.cur_page,
                text,
            }));
        }
    }

    /// Hide only when the handler names the book this
    /// window currently owns. A stale watch from a previously opened book is harmless.
    pub fn close_book(&mut self, ui: &mut UiSystem) {
        self.capture_draft(ui);
        if let Some(book) = self.book_id.take() {
            ui.requests
                .emit(UiRequest::Book(BookAction::Close { book }));
        }
    }

    pub fn recv_object_range_exit(
        &mut self,
        ui: &mut UiSystem,
        book: dereth_primitives::ObjectId,
    ) -> bool {
        if self.book_id != Some(book) {
            return false;
        }
        if let Some(window) = self.window {
            ui.set_visible(window, false);
        }
        true
    }

    /// The page count.
    #[must_use]
    pub fn num_pages(&self) -> i32 {
        i32::try_from(self.pages.len()).unwrap_or(i32::MAX)
    }

    /// One row per page, then `"(blank)"` up to the page limit.
    fn update_menu(&mut self, ui: &mut UiSystem) {
        self.menu_labels.clear();
        for p in &self.pages {
            self.menu_labels
                .push(page_menu_label(p, self.viewer_is_psr));
        }
        for _ in self.num_pages()..self.max_num_pages {
            self.menu_labels.push(BLANK_PAGE_LABEL.to_owned());
        }
        let Some(menu) = self.page_menu else { return };
        if dereth_ui::widgets::menu::list_box_handle(ui, menu).is_none() {
            let made = ui.env().cloned().map(|e| {
                e.with_assets(|assets| dereth_ui::widgets::menu::make_popup(ui, assets, menu))
            });
            if made.flatten().is_none() {
                return;
            }
            dereth_ui::widgets::menu::initialize_popup(ui, menu);
        }
        dereth_ui::widgets::menu::flush(ui, menu);
        let rows = ui.env().cloned().map(|e| {
            e.with_assets(|assets| {
                self.menu_labels
                    .iter()
                    .enumerate()
                    .filter_map(|(index, label)| {
                        dereth_ui::widgets::menu::insert_text_item(ui, assets, menu, label, index)
                            .map(|row| (index, row))
                    })
                    .collect::<Vec<_>>()
            })
        });
        let Some(rows) = rows else { return };
        for (index, row) in rows {
            if let Some(number) = ui.get_child_recursive(row, PAGE_MENU_ROW_NUMBER) {
                if let Some(text) = ui.text_element_mut(number) {
                    text.set_text(&format!("Page {}", index + 1));
                }
            }
        }
    }

    /// The two identical "read the selected index, take the current page's item, select it"
    /// blocks: one after a refusal and one after a successful turn.
    fn select_current_menu_page(&self, ui: &mut UiSystem) {
        let Some(menu) = self.page_menu else { return };
        if dereth_ui::widgets::menu::selected_index(ui, menu) == self.cur_page {
            return;
        }
        let item = usize::try_from(self.cur_page)
            .ok()
            .and_then(|page| dereth_ui::widgets::menu::get_item(ui, menu, page));
        dereth_ui::widgets::menu::set_selected_item(ui, menu, item, true);
    }

    /// The book panel's set-current-page, including its request-producing authoring half.
    pub fn set_cur_page(&mut self, ui: &mut UiSystem, page: i32) {
        self.capture_draft(ui);
        if let Some(book) = self.book_id {
            ui.requests
                .emit(UiRequest::Book(BookAction::Turn { book, page }));
        }
    }

    fn page_text_value(&mut self, ui: &mut UiSystem) -> String {
        self.page_text
            .and_then(|h| ui.text_element_mut(h))
            .map_or_else(|| self.page_body.clone(), |t| t.glyphs.inq_text(false))
    }

    fn set_page_text(&mut self, ui: &mut UiSystem, text: &str, editable: bool) {
        self.page_body = text.to_owned();
        self.editable = editable;
        if let Some(h) = self.page_text {
            if let Some(t) = ui.text_element_mut(h) {
                t.set_text(text);
            }
            ui.set_attribute_bool(h, dereth_ui::props::attr::TEXT_EDITABLE, editable);
        }
    }

    /// `set_cur_page`'s tail: the two state writes. See the module doc
    /// for why these are states and not an enable flag.
    fn update_paging_buttons(&mut self, ui: &mut UiSystem) {
        if let Some(h) = self.prev_button {
            ui.set_state(
                h,
                if self.cur_page > 0 {
                    STATE_NORMAL
                } else {
                    STATE_UNAVAILABLE
                },
            );
        }
        if let Some(h) = self.next_button {
            let more = self.cur_page < self.max_num_pages - 1;
            ui.set_state(
                h,
                if more {
                    STATE_NORMAL
                } else {
                    STATE_UNAVAILABLE
                },
            );
        }
    }

    /// The client's two paging arms, and
    /// The visibility-changed handler's close edge.
    ///
    /// Message 1 on `0x10000114` goes to the previous page and on `0x10000115` to the next; message
    /// 7 on the page menu `0x10000470` goes to the menu's selected index.
    ///
    /// The third arm, message `0x19` on the page text, is the inscription/edit path and is not
    /// here.
    pub fn on_element_message(
        &mut self,
        ui: &mut UiSystem,
        m: &dereth_ui::msg::element::ElementMessage,
    ) {
        use dereth_ui::msg::element::id;
        if self.book_id.is_none() {
            return;
        }
        //  tests the initialized element flag, then tests `visible == false`
        // before unregistering its range handler and calling. An open
        // `book_id` is this panel's initialized/live-book gate; `close_book` supplies the existing
        // `close_cur_page` commit before clearing it.
        if m.id == id::VISIBILITY_CHANGED && m.source_id == WINDOW && m.p1 == 0 {
            self.close_book(ui);
        } else if m.id == id::BUTTON_CLICKED {
            if m.source_id == PREV_BUTTON {
                self.set_cur_page(ui, self.cur_page - 1);
            } else if m.source_id == NEXT_BUTTON {
                self.set_cur_page(ui, self.cur_page + 1);
            }
        } else if m.id == id::MENU_CHOSEN && m.source_id == PAGE_MENU {
            if let Some(menu) = self.page_menu {
                self.set_cur_page(ui, dereth_ui::widgets::menu::selected_index(ui, menu));
            }
        }
    }
}
