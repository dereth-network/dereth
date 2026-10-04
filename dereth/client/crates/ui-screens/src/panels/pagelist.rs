//! `PageListPanel` — the **Page List tab**, the other half of the journal mechanism.
//!
//! The recovered toolbar and panel behavior gives element type `0x10000049`; the instance is
//! `0x10000564`, the third sub-panel of `<QUES>` `0x10000559`, and its tab is the text element
//! `0x10000561`.
//!
//! # It is one mechanism with [`super::journal`], not two panels
//!
//! Everything this panel shows comes out of the journal's page list, and both of its gestures that
//! do anything call straight into the journal panel: the client's `0x43` arm is `goto_page`
//! followed by showing the journal, and its delete arm is `delete_page`. So this module takes
//! `&mut JournalPanel` wherever the client reads the global journal, and owns only the **filtered,
//! sorted copy** the list box is built from ([`PageListPanel::pages`]).
//!
//! # The element-message arms
//!
//! The element-message handler is a switch over ten consecutive element ids starting at
//! `0x1000057F`. `0x10000588` and `0x10000585` are two cases, not one, and the fall-through
//! between them is real:
//!
//! ```text
//! 0x1000057F -> sort by page number (toggles reverse when already the criterion)
//! 0x10000580 -> sort by title
//! 0x10000581 -> sort by timer
//! 0x10000582 -> sort by label
//! 0x10000583 -> nothing (the list box itself)
//! 0x10000584 -> nothing
//! 0x10000585 -> delete the selected page
//! 0x10000586 -> rebuild_page_list(filter = true); sort_page_list
//! 0x10000587 -> nothing (the search box itself)
//! 0x10000588 -> clear the search box; rebuild_page_list(false); sort_page_list;
//!               ...and then **falls through into the delete arm**
//! ```
//!
//! **The fall-through is harmless and that is why it survived.** `rebuild_page_list` +
//! `sort_page_list` ends by rebuilding the list box, which first flushes it and clears its
//! selection — so by the time the delete arm reads the selected index it is `-1` and the arm
//! returns immediately. Reproduced as retail behaves, with the guard that makes it a no-op, rather
//! than "cleaned up" into a jump retail does not have.
//!
//! The sort criteria values follow the same arms: title is 1, timer 3, label 2 and page number 0,
//! i.e. [`JournalSortCriteria`]`::PageNumber = 0`, `Title = 1`, `Label = 2`, `Timer = 3`.
//!
//! Each row is added from the row template `0x10000589` and fills four children:
//! [`ROW_PAGE_NUMBER`] `"%d"`, [`ROW_TITLE`] the title, [`ROW_TIMER`] the timer text and
//! [`ROW_LABEL`] the label.
//!
//! # The list box selects the row, not the row itself
//!
//! **A pointer press on a row of this list hit-tests to the list box, never to the row**
//! (`0x10000583`). That is retail's behaviour: nothing makes a list row mouse-visible — a plain
//! container with no context menu and no tooltip is not a hit target. Retail therefore does not
//! route row clicks through the row at all:
//!
//! ```text
//! list box mouse-down -> base mouse-down, which broadcasts 0x1C
//!     list box 0x1C arm: on button 7 with the list's selectable bit set,
//!         select the item under the mouse, notifying
//!     tail of the mouse-down: on button 7 or 10, select the item under the mouse, notifying
//!
//! list box select(item, notify):
//!     if item is already selected: broadcast 0x43 (when notifying) and return
//!     otherwise change the selection and broadcast 4 (index, item)
//! ```
//!
//! So **`0x43` is "you pressed the row that was already selected"**, and the double-click
//! check's one-second window is the detector on top of it. And because one press selects the
//! item **twice** — once from the `0x1C` broadcast the base class raises and once from the
//! mouse-down's own tail — a real double-click is two presses: the first selects and then raises
//! the first `0x43` (which arms the detector), the second raises two more and the first of those
//! fires.
//!
//! **`dereth_ui::widgets::listbox::ListBox` carries both halves.** The widget's element-message
//! handler carries the `0x1C` arm and the mouse-down tail, and `set_selected_item` is retail's
//! whole — including the already-selected branch that is `0x43`'s only producer. So this panel
//! needs no selection code of its own, only the `0x43` arm below.
//!
//! `GamePlayScreen::panel_messages` forwards `0x43` from [`LIST`] to this panel, which is the one
//! wire that makes the arm reachable.
//!
//! # Two warts that look like defects and are retail's
//!
//! *"Search does not work"* and *"Reset appears unwired"* are what retail does, as the shipped
//! data shows; the captions in layout `0x21000067` are the thing to read first.
//!
//! **1. "Reset" is the SEARCH box's reset.** [`CLEAR_SEARCH_BUTTON`] `0x10000588` is captioned
//! **"Reset"**, and it clears the search box, rebuilds the list unfiltered and sorts it. It
//! never reads the selection and it deletes nothing, so with an empty search box it is a **visual
//! no-op** — which is exactly what "unwired" looks like from the outside. The Journal tab's Reset
//! is a different control: `super::journal::START_TIMER_BUTTON` `0x1000057D`, whose caption
//! flips between "Start" and "Reset".
//!
//! And [`SEARCH_BUTTON`] `0x10000586` is captioned **"Search:"** — with the colon. It reads as a
//! label for the box beside it and it is a *button*: all seven of the four sort headers, Delete,
//! "Search:" and "Reset" derive from the same base element (`0x1000057E` of `0x21000066`). There
//! is **no live filter** in this panel and **Enter cannot search**, because the handler maps
//! the edit box [`SEARCH_EDIT`] `0x10000587` to an arm that does nothing. A player who
//! types a needle and waits — or presses Return — sees a search that does not work. That is
//! retail.
//!
//! **2. A selected row draws differently after all — the highlight in the data is reached through
//! the `MasterProperty` default.** The row template [`ROW_TEMPLATE`] `0x10000589` authors a
//! **state 6** whose only content is a different image (`0x06001AAF` for the base's
//! `0x06004CCA`). It is reached, though the first two steps below suggest it is not:
//!
//! * [`LIST`] `0x10000583` declares `0x61` `SelectedItemStateChange` **true**, so
//!   the list's two state changes are enabled;
//! * it declares neither `0x5D` `UnselectedState` nor `0x5E` `SelectedState`, and the attribute
//!   lookup searches instance -> current state -> element description and nothing else — **but
//!   the enum-attribute read does not stop there.** When nothing is found it falls back to the
//!   property's master-row default value; only a property with no default reads as 0.
//!   `MasterProperty 0x39000001` row `0x5D` defaults to **1** and row `0x5E` to **6**;
//!   so the selected row enters **state 6** and blits
//!   `0x06001AAF`, and the row losing the selection is pushed state 1, which it does not author,
//!   and the state change records 0 for it.
//!
//! So a selected row *does* show a selection indicator in retail: `dereth_ui::widgets::
//! listbox::set_selected_item` reads the attribute through `UiSystem::get_attribute_enum`, which
//! carries the default, rather than skipping `set_state` when the list box names no state.

use crate::view::UiRequest;
use dereth_client_contract::journal::JournalAction;
use dereth_ui::{ElemHandle, ElementId, UiSystem};

use super::journal::{timer_text, JournalPage, JournalPanel};
use super::listbox::ListBoxWidget;

/// The `PageListPanel` sub-panel.
pub const PANEL: ElementId = ElementId(0x1000_0564);
/// The **Page List tab**, paired with [`PANEL`] by the shipped `0x2E` table of `<QUES>`.
pub const TAB: ElementId = ElementId(0x1000_0561);
/// The page list box.
pub const LIST: ElementId = ElementId(0x1000_0583);
/// The search edit box.
pub const SEARCH_EDIT: ElementId = ElementId(0x1000_0587);

/// The row template named by page-list insertion. It is the **only** entry
/// of the list box's template list in the shipped layout, and its layout is `0x21000067`.
pub const ROW_TEMPLATE: ElementId = ElementId(0x1000_0589);
/// The row's page-number cell — `"%d"` over the page number.
pub const ROW_PAGE_NUMBER: ElementId = ElementId(0x1000_058A);
/// The row's title cell.
pub const ROW_TITLE: ElementId = ElementId(0x1000_058B);
/// The row's timer cell — [`timer_text`], and the one cell `refresh_page_listbox` re-writes.
pub const ROW_TIMER: ElementId = ElementId(0x1000_058C);
/// The row's label cell.
pub const ROW_LABEL: ElementId = ElementId(0x1000_058D);

/// The four column headers, in jump-table order.
pub const SORT_BY_PAGE_NUMBER: ElementId = ElementId(0x1000_057F);
pub const SORT_BY_TITLE: ElementId = ElementId(0x1000_0580);
pub const SORT_BY_TIMER: ElementId = ElementId(0x1000_0581);
pub const SORT_BY_LABEL: ElementId = ElementId(0x1000_0582);
/// Delete the selected page. Captioned **"Delete"**.
pub const DELETE_BUTTON: ElementId = ElementId(0x1000_0585);
/// Apply the search box as a filter. Captioned **"Search:"** — it reads as a label for the box
/// and it is the only thing that filters; see the module header.
pub const SEARCH_BUTTON: ElementId = ElementId(0x1000_0586);
/// Empty the search box and drop the filter. Captioned **"Reset"**, and it is the *search box's*
/// reset — it does not touch the selection; see the module header.
pub const CLEAR_SEARCH_BUTTON: ElementId = ElementId(0x1000_0588);

/// How often the timer column redraws — every 0.5 s, the same period as the journal's.
pub const TICK_PERIOD: f64 = super::journal::TICK_PERIOD;

/// The double-click check's window — one second after the last click.
pub const DOUBLE_CLICK_WINDOW: f64 = 1.0;

pub use dereth_presentation::journal::{compare, page_contains_string, JournalSortCriteria};

/// `-1` When nothing is selected, and `-1` as
/// well when the element carries no list-box behaviour or that behaviour is currently lifted out
/// of the arena (a widget cannot be read back out of the arena from inside its own handler).
/// Both of those are the client's "no selection" answer.
#[must_use]
pub fn selected_index(ui: &UiSystem, list: ElemHandle) -> i32 {
    ui.node(list)
        .and_then(|n| n.behaviour.as_ref())
        .and_then(|b| (**b).as_any())
        .and_then(|a| a.downcast_ref::<dereth_ui::widgets::listbox::ListBox>())
        .and_then(|l| l.selected)
        .and_then(|i| i32::try_from(i).ok())
        .unwrap_or(-1)
}

/// `PageListPanel`.
#[derive(Debug, Default)]
pub struct PageListPanel {
    /// The `PageListPanel` element.
    pub panel: Option<ElemHandle>,
    list: Option<ListBoxWidget>,
    search: Option<ElemHandle>,

    /// The filtered, sorted copy the rows are built from. **Not** the journal's own page list: a
    /// row's identity is its [`JournalPage::page_number`], which is what both
    /// gestures pass back to the journal.
    pub pages: Vec<JournalPage>,
    /// The current sort criterion.
    pub sort: JournalSortCriteria,
    /// Whether the sort is reversed.
    pub reverse: bool,
    shared_revision: Option<(u64, u64)>,
    /// The last click's row index and time, for the double-click check.
    last_click_index: i32,
    last_click_time: f64,
    /// When the timer column next redraws.
    time_next_update: f64,

    /// Denominators: how many list rebuilds and how many double-click page jumps happened.
    pub rebuilds: u32,
    pub activations: u32,
}

impl PageListPanel {
    /// Rebuild only from the shared notebook's current content and list criteria.
    pub fn sync(&mut self, ui: &mut UiSystem, journal: &JournalPanel) {
        let state = &journal.shared;
        let edge = (state.generation, state.revision);
        if self.shared_revision == Some(edge) {
            return;
        }
        self.shared_revision = Some(edge);
        self.sort = match state.sort {
            1 => JournalSortCriteria::Title,
            2 => JournalSortCriteria::Label,
            3 => JournalSortCriteria::Timer,
            _ => JournalSortCriteria::PageNumber,
        };
        self.reverse = state.reverse;
        if let Some(t) = self.search.and_then(|h| ui.text_element_mut(h)) {
            t.set_text(&state.search);
        }
        self.rebuild_page_list(ui, journal, state.filtered);
        self.sort_page_list(ui);
    }

    /// Binds two children and registers for global message 3.
    pub fn post_init(&mut self, ui: &mut UiSystem, root: ElemHandle) {
        self.panel = ui.get_child_recursive(root, PANEL);
        let Some(p) = self.panel else { return };
        self.list = ui
            .get_child_recursive(p, LIST)
            .map(|h| ListBoxWidget::bind(ui, h));
        self.search = ui.get_child_recursive(p, SEARCH_EDIT);
        self.last_click_index = -1;
    }

    /// Whether `post_init` found the `PageListPanel` element.
    #[must_use]
    pub fn bound(&self) -> bool {
        self.panel.is_some()
    }

    /// Whether both of the children `post_init` binds are in the shipped layout.
    #[must_use]
    pub fn fully_bound(&self) -> bool {
        self.panel.is_some() && self.list.is_some() && self.search.is_some()
    }

    /// The rows as they read on screen, `(page number, title, timer, label)` per row, pulled back
    /// out of the live elements rather than out of [`Self::pages`] — so a test that asserts on
    /// this is asserting about what was drawn.
    #[must_use]
    pub fn rows(&self, ui: &mut UiSystem) -> Vec<(String, String, String, String)> {
        let Some(list) = self.list.as_ref() else {
            return Vec::new();
        };
        let cell = |ui: &mut UiSystem, row: ElemHandle, id: ElementId| -> String {
            ui.get_child_recursive(row, id)
                .and_then(|h| ui.text_element_mut(h))
                .map_or_else(String::new, |t| t.glyphs.inq_text(false))
        };
        list.items
            .clone()
            .into_iter()
            .map(|row| {
                (
                    cell(ui, row, ROW_PAGE_NUMBER),
                    cell(ui, row, ROW_TITLE),
                    cell(ui, row, ROW_TIMER),
                    cell(ui, row, ROW_LABEL),
                )
            })
            .collect()
    }

    /// The text in the search box.
    #[must_use]
    pub fn search_text(&self, ui: &mut UiSystem) -> String {
        self.search
            .and_then(|h| ui.text_element_mut(h))
            .map_or_else(String::new, |t| t.glyphs.inq_text(false))
    }

    /// The page list panel's page to listbox insert.
    fn add_page_to_listbox(&mut self, ui: &mut UiSystem, page: &JournalPage) {
        let Some(list) = self.list.as_mut() else {
            return;
        };
        let Some(index) = list.templates.iter().position(|(_, e)| *e == ROW_TEMPLATE) else {
            return;
        };
        let Some(row) = list.add_from_template(ui, index, None) else {
            return;
        };
        let timer = timer_text(page.timer_running, page.timer_stamp, ui.now.0);
        for (id, text) in [
            (ROW_PAGE_NUMBER, format!("{}", page.page_number)),
            (ROW_TITLE, page.title.clone()),
            (ROW_TIMER, timer),
            (ROW_LABEL, page.label.clone()),
        ] {
            if let Some(t) = ui
                .get_child_recursive(row, id)
                .and_then(|h| ui.text_element_mut(h))
            {
                t.set_text(&text);
            }
        }
    }

    /// Flush, clear the selection, one row per
    /// entry of [`Self::pages`].
    pub fn rebuild_page_listbox(&mut self, ui: &mut UiSystem) {
        if let Some(list) = self.list.as_mut() {
            list.flush(ui);
        }
        for page in self.pages.clone() {
            self.add_page_to_listbox(ui, &page);
        }
        if let Some(list) = self.list.as_mut() {
            list.update_layout(ui);
        }
        self.rebuilds += 1;
    }

    /// The tick's cheap redraw: **only** the timer
    /// cell, and **only** while the row count still matches the page count.
    ///
    /// The guard is retail's own: the row count equals the page count and neither is zero, which
    /// is what stops a stale row set being written through after a delete.
    pub fn refresh_page_listbox(&mut self, ui: &mut UiSystem) {
        let Some(list) = self.list.as_ref() else {
            return;
        };
        if list.items.len() != self.pages.len() || self.pages.is_empty() {
            return;
        }
        for (row, page) in list.items.clone().into_iter().zip(self.pages.clone()) {
            let s = timer_text(page.timer_running, page.timer_stamp, ui.now.0);
            if let Some(t) = ui
                .get_child_recursive(row, ROW_TIMER)
                .and_then(|h| ui.text_element_mut(h))
            {
                t.set_text(&s);
            }
        }
    }

    /// The page list panel's page list rebuild — copy the journal's pages into
    /// [`Self::pages`], filtered by the search box when `filter` is set.
    pub fn rebuild_page_list(&mut self, ui: &mut UiSystem, journal: &JournalPanel, filter: bool) {
        let needle = if filter {
            self.search_text(ui)
        } else {
            String::new()
        };
        self.pages.clear();
        for page in &journal.pages {
            if !filter || page_contains_string(page, &needle) {
                self.pages.push(page.clone());
            }
        }
    }

    /// Sort, then rebuild the rows.
    pub fn sort_page_list(&mut self, ui: &mut UiSystem) {
        let by = self.sort;
        let reverse = self.reverse;
        self.pages.sort_by(|a, b| {
            let o = compare(a, b, by);
            if reverse {
                o.reverse()
            } else {
                o
            }
        });
        self.rebuild_page_listbox(ui);
    }

    /// The page list's double-click check: a second activation of the same row within the
    /// window. The module header describes the list box's already-selected broadcast it detects.
    pub fn check_for_double_click(&mut self, index: i32, now: f64) -> bool {
        if index == self.last_click_index && now <= self.last_click_time + DOUBLE_CLICK_WINDOW {
            self.last_click_time = 0.0;
            self.last_click_index = -1;
            return true;
        }
        self.last_click_time = now;
        self.last_click_index = index;
        false
    }

    /// The page list panel's global-message handler's `3` arm.
    pub fn tick(&mut self, ui: &mut UiSystem) -> bool {
        let visible = self
            .panel
            .and_then(|h| ui.node(h))
            .is_some_and(|n| n.region.flags.visible);
        if !visible || ui.now.0 < self.time_next_update {
            return false;
        }
        self.refresh_page_listbox(ui);
        self.time_next_update = ui.now.0 + TICK_PERIOD;
        true
    }

    /// On the **shown** edge only, rebuild the
    /// list with the filter on iff the search box has any glyphs in it, then sort.
    pub fn on_visibility_changed(
        &mut self,
        ui: &mut UiSystem,
        journal: &JournalPanel,
        visible: bool,
    ) {
        if !visible {
            return;
        }
        let search = self.search_text(ui);
        ui.requests
            .emit(UiRequest::Journal(JournalAction::Search(search.clone())));
        self.rebuild_page_list(ui, journal, !search.is_empty());
        self.sort_page_list(ui);
    }

    /// The page list's element-message handler, whole — the switch in the module
    /// header, plus the `0x43` arm.
    ///
    /// Returns true when an arm consumed the message.
    pub fn on_element_message(
        &mut self,
        ui: &mut UiSystem,
        m: &dereth_ui::ElementMessage,
        journal: &mut JournalPanel,
    ) -> bool {
        use dereth_ui::msg::element::id;
        if m.id == id::VISIBILITY_CHANGED && m.source_id == PANEL {
            self.on_visibility_changed(ui, journal, m.p1 != 0);
            return true;
        }
        if m.id == id::LIST_ITEM_ACTIVATED {
            let Some(list) = self.list.as_ref() else {
                return false;
            };
            let index = selected_index(ui, list.handle);
            if index == -1 {
                return false;
            }
            return self.item_activated(ui, index, journal);
        }
        // There is no `0x1C` arm here, and there never was one in `PageListPanel`: the switch
        // maps the list box `0x10000583` to an arm that does nothing. The press is the **list
        // box's** business, and the widget does it. See the module header.
        if m.id != id::BUTTON_CLICKED {
            return false;
        }
        // The four sort headers: naming the criterion that is already in force **toggles the
        // direction**; naming a different one selects it forwards.
        let criterion = match m.source_id {
            SORT_BY_PAGE_NUMBER => Some(JournalSortCriteria::PageNumber),
            SORT_BY_TITLE => Some(JournalSortCriteria::Title),
            SORT_BY_TIMER => Some(JournalSortCriteria::Timer),
            SORT_BY_LABEL => Some(JournalSortCriteria::Label),
            _ => None,
        };
        if let Some(c) = criterion {
            let by = match c {
                JournalSortCriteria::PageNumber => 0,
                JournalSortCriteria::Title => 1,
                JournalSortCriteria::Label => 2,
                JournalSortCriteria::Timer => 3,
            };
            ui.requests
                .emit(UiRequest::Journal(JournalAction::Sort(by)));
            return true;
        }
        match m.source_id {
            SEARCH_BUTTON => {
                let search = self.search_text(ui);
                ui.requests
                    .emit(UiRequest::Journal(JournalAction::Search(search)));
            }
            CLEAR_SEARCH_BUTTON => {
                if let Some(t) = self.search.and_then(|h| ui.text_element_mut(h)) {
                    t.set_text("");
                }
                ui.requests
                    .emit(UiRequest::Journal(JournalAction::ResetSearch));
            }
            DELETE_BUTTON => self.delete_selected_page(ui, journal),
            _ => return false,
        }
        true
    }

    /// The client's `0x43` arm, entered by index rather than by message —
    /// the index is the list box's selected index, which is what the client reads there.
    /// Returns true when the page was opened.
    fn item_activated(
        &mut self,
        ui: &mut UiSystem,
        index: i32,
        journal: &mut JournalPanel,
    ) -> bool {
        if !self.check_for_double_click(index, ui.now.0) {
            return false;
        }
        let Some(page) = usize::try_from(index).ok().and_then(|i| self.pages.get(i)) else {
            return false;
        };
        let number = page.page_number;
        journal.goto_page(ui, number);
        // Showing the journal panel: its own `0x18` listener is what then pulls the Journal tab open, which is how a
        // double-click in this list switches tabs.
        if let Some(h) = journal.panel {
            ui.set_visible(h, true);
        }
        self.activations += 1;
        true
    }

    /// Read the selected index, check the journal panel exists, and delete the page with the
    /// selected row's page number.
    fn delete_selected_page(&mut self, ui: &mut UiSystem, journal: &mut JournalPanel) {
        let Some(list) = self.list.as_ref() else {
            return;
        };
        let index = selected_index(ui, list.handle);
        let Some(page) = usize::try_from(index).ok().and_then(|i| self.pages.get(i)) else {
            return;
        };
        let number = page.page_number;
        journal.delete_page(ui, number);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(n: u32, label: &str, title: &str, notes: &str) -> JournalPage {
        JournalPage {
            page_number: n,
            label: label.to_owned(),
            title: title.to_owned(),
            notes: notes.to_owned(),
            ..JournalPage::default()
        }
    }

    /// Oracle: retail's page search — the empty-needle early return and the three lower-cased
    /// `wcsstr` probes over label, title and notes.
    #[test]
    fn an_empty_search_matches_everything_and_the_match_is_case_insensitive() {
        let p = page(1, "Aluvian", "The Hollow Minion", "ask Nuhmudira");
        assert!(
            page_contains_string(&p, ""),
            "an empty needle returns true before anything else"
        );
        assert!(page_contains_string(&p, "aluv"), "label");
        assert!(
            page_contains_string(&p, "HOLLOW"),
            "title, case-insensitively"
        );
        assert!(page_contains_string(&p, "nuhmudira"), "notes");
        assert!(!page_contains_string(&p, "banderling"));
    }

    /// Oracle: the page info sort timer's three arms.
    #[test]
    fn the_timer_sort_puts_running_pages_first_then_by_stamp_then_by_page_number() {
        use std::cmp::Ordering;
        let running = |stamp: f64, n: u32| JournalPage {
            page_number: n,
            timer_running: true,
            timer_stamp: stamp,
            ..JournalPage::default()
        };
        let stopped = |n: u32| JournalPage {
            page_number: n,
            ..JournalPage::default()
        };
        let by = JournalSortCriteria::Timer;
        assert_eq!(compare(&running(10.0, 9), &stopped(1), by), Ordering::Less);
        assert_eq!(
            compare(&stopped(1), &running(10.0, 9), by),
            Ordering::Greater
        );
        assert_eq!(
            compare(&running(10.0, 9), &running(20.0, 1), by),
            Ordering::Less
        );
        assert_eq!(compare(&stopped(1), &stopped(2), by), Ordering::Less);
    }

    /// Oracle: retail's title sort and contract-name sort — both lower-case both sides before
    /// `wcscmp`.
    #[test]
    fn the_title_and_label_sorts_are_case_insensitive() {
        use std::cmp::Ordering;
        let a = page(1, "zeta", "Apple", "");
        let b = page(2, "Alpha", "banana", "");
        assert_eq!(compare(&a, &b, JournalSortCriteria::Title), Ordering::Less);
        assert_eq!(
            compare(&a, &b, JournalSortCriteria::Label),
            Ordering::Greater
        );
        assert_eq!(
            compare(&a, &b, JournalSortCriteria::PageNumber),
            Ordering::Less
        );
    }

    /// Oracle: retail's double-click check — same index inside one second, and the reset that
    /// stops a third activation counting as another pair.
    #[test]
    fn the_double_click_needs_two_activations_of_one_row_inside_a_second() {
        let mut p = PageListPanel {
            last_click_index: -1,
            ..PageListPanel::default()
        };
        assert!(
            !p.check_for_double_click(2, 100.0),
            "the first is never a double click"
        );
        assert!(
            p.check_for_double_click(2, 100.5),
            "the second, inside the window"
        );
        assert!(
            !p.check_for_double_click(2, 100.6),
            "the state was reset by the hit"
        );
        assert!(
            !p.check_for_double_click(2, 102.0),
            "and a late repeat re-arms instead"
        );
        assert!(
            !p.check_for_double_click(3, 102.1),
            "a different row re-arms too"
        );
    }

    /// Oracle: `crate::panels::catalogue`'s `PageListPanel` row and its template table.
    #[test]
    fn the_bound_ids_and_the_four_row_cells_are_the_catalogued_ones() {
        let spec = crate::panels::catalogue::spec("PageListPanel").expect("catalogued");
        let ids: Vec<u32> = spec.children.iter().map(|c| c.id.0).collect();
        assert_eq!(ids, vec![LIST.0, SEARCH_EDIT.0]);
        let templates: Vec<u32> = spec.templates.iter().map(|c| c.id.0).collect();
        assert_eq!(
            templates,
            vec![ROW_PAGE_NUMBER.0, ROW_TITLE.0, ROW_TIMER.0, ROW_LABEL.0],
            "the catalogue's template row is the four cells, not the 0x10000589 container"
        );
    }
}
