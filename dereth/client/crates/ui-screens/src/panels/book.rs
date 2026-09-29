//! `BookPanel` — the window a scroll, a letter or a sign is read in.
//!
//! The recovered toolbar and panel behavior gives element type `0x10000019`, page
//! `0x10000182` of `<PANS>` (`0x100005FF`), under the `0x10000180` group.
//!
//! # The chain
//!
//! Using a scroll, letter or sign opens this window through four links:
//!
//! | link | where |
//! |---|---|
//! | the request — `0x0036 Inventory_UseEvent` | sent by the use gesture |
//! | the reply — `0x00B4 Writing_BookOpen` | decoded by `dereth_protocol::trade::WritingBookOpen`, ordered onto the UI queue by `dereth_client_net::client_session`, and dispatched to this panel |
//! | the model — the panel's page list | this file |
//! | the layout | **shipped and complete.** `0x10000182` is in `catalogue::PANEL_PAGES` and every one of the six children this file binds is in the tree — asserted, not assumed, by `post_init`'s `bound()` |
//!
//! A missing dispatch arm for the reply looks like "the reader window is missing", though the
//! cause is an inbound message with no receiver.
//!
//! # The client's own body, and the three things a plainer reading gets wrong
//!
//! The book panel's open book → the menu update → the cur page write →
//! The display page data, and:
//!
//! 1. **The page limit is the message's second dword, not the page list's.** The list carries its
//!    own max-pages / max-characters-per-page pair, which the original panel never reads. The value
//!    read from the message header before unpacking is the one that clamps the current page and
//!    pads the page strip.
//! 2. **The prev/next controls are not enabled and disabled — they change state.** Prev is
//!    in the normal state when the current page is `> 0`, next when it is `< last`, and the states are the
//!    authored `0x0D` (unavailable) and `0x01` (normal), so the buttons are grey rather than
//!    gone.
//! 3. **The title is the *inscription* when the book has a scribe, and the object's name when it
//!    does not** — the open-book tail uses the appropriate object name (`NAME_APPROPRIATE`) when
//!    the scribe id is 0, and the inscription otherwise. A signed letter therefore shows its inscription and an unsigned sign shows
//!    the sign's name; the scribe name is *not* the title.
//!
//! # Authoring
//!
//! The close-current-page and set-current-page paths produce the five typed writing
//! requests. The page-data display applies the native author rule (the author is the player, or the
//! page ignores its author),
//! and the panel still waits for `0x00B6`/`0x00B8` before showing an added or withheld page.
//!
//! The page-menu update's PSR branch decorates each author's name with the account in angle brackets. The
//! authoritative is-the-player-a-PSR projection reaches that branch; ordinary viewers
//! retain the name-only form.

use dereth_ui::{ElemHandle, ElementId, StateId, UiSystem};

use crate::view::{BookPageView, BookView, GameView, UiRequest};

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

/// The label puts on one page's row of the strip.
///
/// Three branches, in the client's order:
///
/// * a page that ignores its author — the **null string**, so an anonymous page's row is blank;
/// * a PSR looking at a page with a real author — `"- author <account>"`;
/// * everyone else — `"- author"`.
///
/// Retail does not test whether the author's account is empty: an empty account therefore produces
/// the literal `<>` decoration for a PSR.
#[must_use]
pub fn page_menu_label(page: &BookPageView, is_psr: bool) -> String {
    if page.ignore_author == 1 {
        String::new()
    } else if is_psr {
        format!("- {} <{}>", page.author_name, page.author_account)
    } else {
        format!("- {}", page.author_name)
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
    /// [`BookView::opening`] as of the last open, so the pull can tell a new book from the same
    /// one being offered again.
    last_opening: u64,
    /// [`BookView::page_data_applied`] likewise, for the `0x00B8` redraw.
    last_page_data: u64,
    /// [`BookView::add_page_responses`] likewise, for the `0x00B6` insert.
    last_add_page: u64,
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
            return false;
        };
        // Native asks PlayerDesc at each page-menu update. Keep the projected answer current for
        // both the open edge and an authoritative AddPage response that rebuilds the strip.
        self.viewer_is_psr = b.viewer_is_psr;
        if b.opening != self.last_opening {
            self.last_opening = b.opening;
            self.last_page_data = b.page_data_applied;
            self.open_book(ui, &b);
            return true;
        }
        if b.page_data_applied != self.last_page_data {
            self.last_page_data = b.page_data_applied;
            // The page-data response event: the reply clears the pending request and, when
            // it is the page on screen, draws it.
            self.request_pending = false;
            self.pages = b.pages.clone();
            let page = self.cur_page;
            self.display_page(ui, page);
            return true;
        }
        if b.add_page_responses != self.last_add_page {
            self.last_add_page = b.add_page_responses;
            if let Some(a) = b.add_page.clone() {
                self.add_page_response(ui, &a);
                return true;
            }
        }
        false
    }

    /// The book's add-page response event, from `success` down.
    ///
    /// The two tests above it (the object is this book, and the book id is non-zero) are
    /// `book_add_page_response`'s, because they are the only two the model can
    /// take; everything here reads the current page or writes the page list, both of which are this
    /// panel's members in the client too.
    ///
    /// In order: on failure, ask for the whole book again ([`UiRequest::BookData`]) and stop. If
    /// `page` is not the current page, make it the current page, ask for the whole book again and
    /// stop. Otherwise build an empty page authored by the player (name from string property 1),
    /// insert it into the page list at `page`, add its menu label (`"- "` then the author's name),
    /// display it (it is the current page, so always the display arm), and clear the pending
    /// request.
    ///
    /// Note the shape of the third test: retail **moves** the current page on a mismatch and then asks
    /// for the whole book back rather than inserting at the page it was told about. That is the
    /// one branch a plainer "insert the page" reading loses.
    pub fn add_page_response(&mut self, ui: &mut UiSystem, a: &crate::view::AddedPageView) {
        if !a.success {
            self.book_data_refetches += 1;
            if let Some(book) = self.book_id {
                ui.requests.emit(UiRequest::BookData { book });
            }
            return;
        }
        let page = i32::try_from(a.page).unwrap_or(i32::MAX);
        if page != self.cur_page {
            self.cur_page = page;
            self.book_data_refetches += 1;
            if let Some(book) = self.book_id {
                ui.requests.emit(UiRequest::BookData { book });
            }
            return;
        }
        // An **empty** page authored by the player, whose text is present here (the client's
        // text-included flag is 0, so `set_cur_page` would ask for it; the page-data display is
        // called directly here instead).
        let new = BookPageView {
            author_id: a.author_id,
            author_name: a.author_name.clone(),
            author_account: String::new(),
            text: Some(String::new()),
            ignore_author: 0,
        };
        let at = usize::try_from(a.page)
            .unwrap_or(usize::MAX)
            .min(self.pages.len());
        self.pages.insert(at, new);
        self.pages_added += 1;
        self.update_menu(ui);
        self.display_page(ui, page);
        self.select_current_menu_page(ui);
        self.request_pending = false;
    }

    /// The book panel's open book.
    pub fn open_book(&mut self, ui: &mut UiSystem, b: &BookView) {
        // unconditional, before a single page has been looked at.
        if let Some(w) = self.window {
            ui.set_visible(w, true);
        }
        // The page to come back to, and only for the *same* book.
        let same_book = self.book_id == Some(b.book_id);
        let remembered = if same_book { self.cur_page } else { 0 };
        if self.book_id.is_some() {
            self.close_book(ui);
        }
        self.book_id = Some(b.book_id);
        self.player_id = b.player_id;
        self.viewer_is_psr = b.viewer_is_psr;
        self.object_name.clone_from(&b.object_name);
        self.max_num_pages = i32::try_from(b.max_num_pages).unwrap_or(i32::MAX);
        self.pages = b.pages.clone();
        self.update_menu(ui);
        // Start with the page count minus one. If that is nonnegative, use the remembered page
        // instead; finally replace any remaining negative value with zero.
        let last = self.num_pages() - 1;
        let want = if last < 0 { last } else { remembered };
        self.set_cur_page(ui, want.max(0));
        // The tail, in the client's order: inscription, scribe id, scribe name, then the title.
        self.title_text = b.title.clone();
        if let Some(t) = self.title.and_then(|h| ui.text_element_mut(h)) {
            t.set_text(&self.title_text);
        }
        self.opens += 1;
    }

    /// The book panel's close book, including `close_cur_page`'s commit.
    pub fn close_book(&mut self, ui: &mut UiSystem) {
        self.close_cur_page(ui);
        self.book_id = None;
        self.player_id = dereth_primitives::ObjectId(0);
        self.viewer_is_psr = false;
        self.object_name.clear();
        self.max_num_pages = 0;
        self.pages.clear();
        self.cur_page = -1;
        self.request_pending = false;
    }

    /// Hide only when the handler names the book this
    /// window currently owns. A stale watch from a previously opened book is harmless.
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
        if self.book_id.is_none()
            || self.request_pending
            || page < 0
            || page >= self.max_num_pages
            || page == self.cur_page
        {
            return;
        }
        let mut page = page;
        let old_page = self.cur_page;
        let num_pages = self.num_pages();
        let past_last_page = num_pages < page;
        let advancing_past_owned_blank = !past_last_page
            && old_page == num_pages - 1
            && old_page >= 0
            && old_page < page
            && self.page_text_blank(ui)
            && self
                .pages
                .get(old_page as usize)
                .is_some_and(|p| p.author_id == self.player_id);
        if refuses_page_change(page, num_pages, old_page, advancing_past_owned_blank) {
            ui.requests.emit(UiRequest::DisplayChatText {
                channel: 0x1A,
                text: ALREADY_OPEN_TO_BLANK_PAGE.replace("%s", &self.object_name),
            });
            self.select_current_menu_page(ui);
            return;
        }
        if self.close_cur_page(ui) && page > old_page {
            page -= 1;
        }
        self.cur_page = page;
        self.page_turns += 1;
        match usize::try_from(page).ok().and_then(|n| self.pages.get(n)) {
            Some(p) if p.text.is_some() => self.display_page(ui, page),
            Some(_) => {
                self.set_page_text(ui, "", false);
                if let Some(book) = self.book_id {
                    ui.requests.emit(UiRequest::BookPageData {
                        book,
                        page: u32::try_from(page).unwrap_or(0),
                    });
                    self.request_pending = true;
                }
            }
            None if page == self.num_pages() => {
                self.set_page_text(ui, "", false);
                if let Some(book) = self.book_id {
                    ui.requests.emit(UiRequest::BookAddPage { book });
                    self.request_pending = true;
                }
            }
            None => return,
        }
        self.select_current_menu_page(ui);
        self.update_paging_buttons(ui);
    }

    /// Close the current page. True means the player's blank page was deleted; `set_cur_page`
    /// uses that return to adjust a forward destination after the local removal.
    fn close_cur_page(&mut self, ui: &mut UiSystem) -> bool {
        let (Some(book), Ok(page), Ok(index)) = (
            self.book_id,
            u32::try_from(self.cur_page),
            usize::try_from(self.cur_page),
        ) else {
            return false;
        };
        if self.request_pending {
            return false;
        }
        let Some(data) = self.pages.get(index) else {
            return false;
        };
        let author_id = data.author_id;
        let ignore_author = data.ignore_author;
        if author_id != self.player_id && ignore_author == 0 {
            return false;
        }
        let text = self.page_text_value(ui);
        if page_text_blank(&text) && author_id == self.player_id {
            ui.requests.emit(UiRequest::BookDeletePage { book, page });
            self.pages.remove(index);
            self.update_menu(ui);
            return true;
        }
        if let Some(data) = self.pages.get_mut(index) {
            data.text = Some(text.clone());
        }
        ui.requests
            .emit(UiRequest::BookModifyPage { book, page, text });
        false
    }

    fn page_text_value(&mut self, ui: &mut UiSystem) -> String {
        self.page_text
            .and_then(|h| ui.text_element_mut(h))
            .map_or_else(|| self.page_body.clone(), |t| t.glyphs.inq_text(false))
    }

    fn page_text_blank(&mut self, ui: &mut UiSystem) -> bool {
        page_text_blank(&self.page_text_value(ui))
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

    /// The page-data display, and the two lines `set_cur_page` runs instead when the page has
    /// no text: clear the text, deselect, make it non-editable, and then ask for the page
    /// ([`UiRequest::BookPageData`]).
    ///
    /// A page with no text is shown blank and non-editable while the page request (`0x00AE`)
    /// is pending; `0x00B8` replaces it and clears [`BookPanel::request_pending`].
    fn display_page(&mut self, ui: &mut UiSystem, page: i32) {
        let text = usize::try_from(page)
            .ok()
            .and_then(|n| self.pages.get(n))
            .and_then(|p| p.text.clone());
        let editable = usize::try_from(page)
            .ok()
            .and_then(|n| self.pages.get(n))
            .is_some_and(|p| p.author_id == self.player_id || p.ignore_author != 0);
        self.set_page_text(ui, &text.unwrap_or_default(), editable);
        // The client writes the 1-based page number into the strip's
        // own label.
        self.menu_selection_text = format!("{}", page + 1);
        if let Some(t) = self
            .menu_selection_page_num
            .and_then(|h| ui.text_element_mut(h))
        {
            t.set_text(&self.menu_selection_text);
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
            ui.requests.emit(UiRequest::UnregisterBookRange);
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

/// The "is the page text blank" test: only space, line-feed and NUL count as blank.
fn page_text_blank(text: &str) -> bool {
    text.bytes().all(|b| matches!(b, b' ' | b'\n' | 0))
}

/// The client's pre-close OR, isolated so the strict `page count < requested page`
/// boundary stays pinned even while this build's book menu has no constructed runtime rows.
fn refuses_page_change(
    requested_page: i32,
    num_pages: i32,
    current_page: i32,
    advancing_past_owned_blank: bool,
) -> bool {
    num_pages < requested_page || (current_page < requested_page && advancing_past_owned_blank)
}

#[cfg(test)]
mod tests {
    use super::refuses_page_change;

    #[test]
    fn skip_guard_is_strictly_past_the_first_addable_page() {
        assert!(
            !refuses_page_change(2, 2, 1, false),
            "page == numPages is AddPage"
        );
        assert!(
            refuses_page_change(3, 2, 1, false),
            "page > numPages is refused"
        );
        assert!(
            refuses_page_change(2, 2, 1, true),
            "owned blank refuses the forward AddPage"
        );
        assert!(
            !refuses_page_change(0, 2, 1, true),
            "the owned-blank rule is forward-only"
        );
    }
}
