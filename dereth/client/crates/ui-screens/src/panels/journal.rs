//! Journal widgets project shared pages and forward edits, page actions and visibility edges.
//! File parsing and formatting are shared; the host performs file access.

use crate::view::UiRequest;
use dereth_client_contract::journal::{JournalAction, JournalView};
use dereth_ui::{ElemHandle, ElementId, UiSystem};

use crate::view::GameView;

/// `<QUES>` — the quest page of `PanelStack`'s stack, and the `Panel` owning the tabs.
pub const PAGE: ElementId = ElementId(0x1000_0559);
/// The `JournalPanel` sub-panel.
pub const PANEL: ElementId = ElementId(0x1000_0563);
/// The **Journal tab**, paired with [`PANEL`] by the shipped `0x2E` table.
pub const TAB: ElementId = ElementId(0x1000_0560);

/// The label edit box.
pub const LABEL_EDIT: ElementId = ElementId(0x1000_0569);
/// The title edit box.
pub const TITLE_EDIT: ElementId = ElementId(0x1000_056B);
/// The notes edit box.
pub const NOTES_EDIT: ElementId = ElementId(0x1000_056D);
/// The page-number text.
pub const PAGE_NUMBER_TEXT: ElementId = ElementId(0x1000_0570);
/// The location text.
pub const LOCATION_TEXT: ElementId = ElementId(0x1000_0573);
/// The days edit box.
pub const DAYS_EDIT: ElementId = ElementId(0x1000_0576);
/// The days label.
pub const DAYS_TEXT: ElementId = ElementId(0x1000_0577);
/// The hours edit box.
pub const HOURS_EDIT: ElementId = ElementId(0x1000_0578);
/// The hours label.
pub const HOURS_TEXT: ElementId = ElementId(0x1000_0579);
/// The minutes edit box.
pub const MINUTES_EDIT: ElementId = ElementId(0x1000_057A);
/// The minutes label.
pub const MINUTES_TEXT: ElementId = ElementId(0x1000_057B);
/// The timer text.
pub const TIMER_TEXT: ElementId = ElementId(0x1000_057C);
/// The start-timer button — the one control that is both a button and a label.
pub const START_TIMER_BUTTON: ElementId = ElementId(0x1000_057D);

/// Save, then previous page.
pub const PREV_PAGE_BUTTON: ElementId = ElementId(0x1000_0565);
/// Save, then next page.
pub const NEXT_PAGE_BUTTON: ElementId = ElementId(0x1000_0566);
/// Save, append a page, go to it.
pub const NEW_PAGE_BUTTON: ElementId = ElementId(0x1000_0567);
/// Save, then page 1.
pub const FIRST_PAGE_BUTTON: ElementId = ElementId(0x1000_056F);
/// Save, then the last page.
pub const LAST_PAGE_BUTTON: ElementId = ElementId(0x1000_0571);
/// [`JournalPanel::reset_location`], i.e. "stamp where I am standing onto this page".
pub const STAMP_LOCATION_BUTTON: ElementId = ElementId(0x1000_0574);

/// The two journal-file stems and the path they build.
///
/// [`JournalIdentity`] is defined in [`dereth_client_contract::journal`], because
/// `GameView::journal_identity` returns it and the contract crate may not depend on this one;
/// its one inherent method builds a path, so [`create_journal_path`] and [`STEM`] live there too.
pub use dereth_presentation::journal::{
    create_journal_path, delta_time_to_string, JournalIdentity, STEM,
};

// The journal's captions, file format and page live in the shared presentation rules; they are
// re-exported here so their retail paths still resolve.
pub use dereth_presentation::journal::{
    location_text, page_number_text, parse_pages, save_pages_text, tag, timer_text, JournalPage,
    LOAD_COMPLAINT, LOAD_COMPLAINT_CHANNEL, NONE, READY, RESET, START, TICK_PERIOD,
};

/// `JournalPanel`.
#[derive(Debug, Default)]
pub struct JournalPanel {
    /// The `JournalPanel` element.
    pub panel: Option<ElemHandle>,
    label: Option<ElemHandle>,
    title: Option<ElemHandle>,
    notes: Option<ElemHandle>,
    page_number: Option<ElemHandle>,
    location: Option<ElemHandle>,
    days_edit: Option<ElemHandle>,
    days_text: Option<ElemHandle>,
    hours_edit: Option<ElemHandle>,
    hours_text: Option<ElemHandle>,
    minutes_edit: Option<ElemHandle>,
    minutes_text: Option<ElemHandle>,
    timer: Option<ElemHandle>,
    start_button: Option<ElemHandle>,

    /// The shared pages as last projected.
    pub pages: Vec<JournalPage>,
    /// The draft as last projected or captured from the controls.
    pub info: JournalPage,
    /// The current page, 1-based.
    pub current_page: u32,
    /// Whether the pages are loaded — every handler in the panel is a no-op until this is true.
    pub loaded: bool,
    pub shared: JournalView,
    /// When the running timer next redraws.
    time_next_update: f64,

    /// How many [`Self::goto_page`] calls actually moved, how many saves ran, how many timers
    /// were started. Denominators, so a test can tell "nothing happened" from "it happened and
    /// drew nothing".
    pub page_turns: u32,
    pub saves: u32,
    pub timer_starts: u32,

    // ---- persistence ---------------------------------------------------------------------------
    /// The file the host writes, once it has named one. `None` is a
    /// build with no preferences file, no world name or no character name — in which the journal
    /// is session-local and **nothing is written at all**, which is strictly safer than guessing
    /// at a path.
    pub file: Option<std::path::PathBuf>,
    /// How many page loads read a file, and how many page saves wrote one.
    /// Denominators, so a test can tell "nothing happened" from "it happened and read nothing".
    pub page_loads: u32,
    pub page_saves: u32,
}

#[cfg(test)]
mod shared_tests {
    //! Behaviour: none (journal widget projection and action forwarding; notebook transitions are tested in the model).
    use super::*;

    #[derive(Debug)]
    struct View;
    impl GameView for View {
        fn journal(&self) -> JournalView {
            let page = JournalPage {
                page_number: 2,
                title: "Shared title".into(),
                timer_running: true,
                timer_stamp: 900.0,
                ..Default::default()
            };
            JournalView {
                pages: vec![JournalPage::default(), page.clone()],
                draft: page,
                current_page: 2,
                generation: 4,
                revision: 8,
                loaded: true,
                ..Default::default()
            }
        }
    }

    #[test]
    fn rebuilding_the_journal_preserves_shared_page_and_timer_without_a_file_read() {
        let mut ui = UiSystem::new((800, 600));
        let mut panel = JournalPanel::default();
        panel.load(&mut ui, &View);
        assert_eq!(panel.current_page, 2);
        assert_eq!(panel.info.title, "Shared title");
        assert_eq!(panel.info.timer_stamp, 900.0);
        assert!(panel.info.timer_running);
        assert!(panel.file.is_none());
        panel.goto_page(&mut ui, 1);
        assert_eq!(
            ui.requests.take(),
            [UiRequest::Journal(JournalAction::Goto(1))]
        );
        assert_eq!(
            panel.current_page, 2,
            "the widgets do not own a second cursor"
        );
        panel.forget();
        panel.load(&mut ui, &View);
        assert_eq!(panel.current_page, 2);
        assert_eq!(panel.info.title, "Shared title");
        assert!(ui.requests.take().is_empty());
    }
}

impl JournalPanel {
    /// Bind the controls and clear the projection so the next update fills every box.
    pub fn post_init(&mut self, ui: &mut UiSystem, root: ElemHandle) {
        self.loaded = false;
        self.shared = JournalView::default();
        self.file = None;
        self.pages.clear();
        self.info = JournalPage::default();
        self.current_page = 0;
        self.panel = ui.get_child_recursive(root, PANEL);
        let Some(p) = self.panel else { return };
        let find = |ui: &UiSystem, id: ElementId| ui.get_child_recursive(p, id);
        self.start_button = find(ui, START_TIMER_BUTTON);
        self.label = find(ui, LABEL_EDIT);
        self.title = find(ui, TITLE_EDIT);
        self.notes = find(ui, NOTES_EDIT);
        self.days_edit = find(ui, DAYS_EDIT);
        self.hours_edit = find(ui, HOURS_EDIT);
        self.minutes_edit = find(ui, MINUTES_EDIT);
        self.location = find(ui, LOCATION_TEXT);
        self.timer = find(ui, TIMER_TEXT);
        self.days_text = find(ui, DAYS_TEXT);
        self.hours_text = find(ui, HOURS_TEXT);
        self.minutes_text = find(ui, MINUTES_TEXT);
        self.page_number = find(ui, PAGE_NUMBER_TEXT);
        self.show_editable_timer(ui);
    }

    /// Whether `post_init` found the `JournalPanel` element.
    #[must_use]
    pub fn bound(&self) -> bool {
        self.panel.is_some()
    }

    /// Whether all thirteen of the post-init's children are in the shipped layout.
    #[must_use]
    pub fn fully_bound(&self) -> bool {
        self.panel.is_some()
            && self.label.is_some()
            && self.title.is_some()
            && self.notes.is_some()
            && self.page_number.is_some()
            && self.location.is_some()
            && self.days_edit.is_some()
            && self.days_text.is_some()
            && self.hours_edit.is_some()
            && self.hours_text.is_some()
            && self.minutes_edit.is_some()
            && self.minutes_text.is_some()
            && self.timer.is_some()
            && self.start_button.is_some()
    }

    // ---- reading and writing the elements -----------------------------------------------------

    fn set_text(ui: &mut UiSystem, h: Option<ElemHandle>, s: &str) {
        if let Some(t) = h.and_then(|h| ui.text_element_mut(h)) {
            t.set_text(s);
        }
    }

    /// The element's text, with the tags stripped the way every other reader in this crate
    /// takes it.
    fn get_text(ui: &mut UiSystem, h: Option<ElemHandle>) -> String {
        h.and_then(|h| ui.text_element_mut(h))
            .map_or_else(String::new, |t| t.glyphs.inq_text(false))
    }

    /// One of the three timer boxes as a count; see
    /// [`dereth_presentation::journal::parse_count`].
    fn wcstoul(s: &str) -> u32 {
        dereth_presentation::journal::parse_count(s)
    }

    // ---- the timer ----------------------------------------------------------------------------

    /// The three edit boxes and their three labels
    /// visible, the countdown label hidden and blanked, the button captioned `"Start"`.
    pub fn show_editable_timer(&mut self, ui: &mut UiSystem) {
        for h in [
            self.days_edit,
            self.days_text,
            self.hours_edit,
            self.hours_text,
            self.minutes_edit,
            self.minutes_text,
        ]
        .into_iter()
        .flatten()
        {
            ui.set_visible(h, true);
        }
        if let Some(h) = self.timer {
            ui.set_visible(h, false);
        }
        Self::set_text(ui, self.timer, "");
        Self::set_text(ui, self.start_button, START);
    }

    /// The mirror image, captioned `"Reset"`, ending
    /// in [`Self::update_running_timer`].
    pub fn show_running_timer(&mut self, ui: &mut UiSystem) {
        for h in [
            self.days_edit,
            self.days_text,
            self.hours_edit,
            self.hours_text,
            self.minutes_edit,
            self.minutes_text,
        ]
        .into_iter()
        .flatten()
        {
            ui.set_visible(h, false);
        }
        if let Some(h) = self.timer {
            ui.set_visible(h, true);
        }
        Self::set_text(ui, self.start_button, RESET);
        self.update_running_timer(ui);
    }

    /// The journal panel's running timer update — the timer text becomes [`timer_text`] of the
    /// edited page's running flag and stamp.
    pub fn update_running_timer(&mut self, ui: &mut UiSystem) {
        let s = timer_text(self.info.timer_running, self.info.timer_stamp, ui.now.0);
        Self::set_text(ui, self.timer, &s);
    }

    /// Capture the timer controls and toggle the shared timer.
    pub fn reset_timer(&mut self, ui: &mut UiSystem) -> i32 {
        self.save_this_page(ui);
        ui.requests
            .emit(UiRequest::Journal(JournalAction::ToggleTimer));
        0
    }

    // ---- the page ------------------------------------------------------------------------------

    /// The journal panel's location update.
    pub fn update_location(&mut self, ui: &mut UiSystem) {
        let s = location_text(self.info.location_set, self.info.ns, self.info.ew);
        Self::set_text(ui, self.location, &s);
    }

    /// The journal panel's page number update.
    pub fn update_page_number(&mut self, ui: &mut UiSystem) {
        let s = page_number_text(self.current_page);
        Self::set_text(ui, self.page_number, &s);
    }

    /// Read the player's coordinates into the page, set the location flag to whether that
    /// succeeded, then redraw.
    ///
    /// The client zeroes both coordinates **before** the read, so a failed read
    /// leaves `(0, 0)` behind and not the previous stamp. Reproduced.
    pub fn reset_location(&mut self, ui: &mut UiSystem, _view: &dyn GameView) {
        self.save_this_page(ui);
        ui.requests
            .emit(UiRequest::Journal(JournalAction::StampLocation));
    }

    /// Forward changed controls with the identity generation and page they were drawn from.
    pub fn save_this_page(&mut self, ui: &mut UiSystem) {
        if !self.loaded || !self.bound() {
            return;
        }
        let mut draft = self.info.clone();
        draft.label = Self::get_text(ui, self.label);
        draft.title = Self::get_text(ui, self.title);
        draft.notes = Self::get_text(ui, self.notes);
        draft.days = Self::wcstoul(&Self::get_text(ui, self.days_edit));
        draft.hours = Self::wcstoul(&Self::get_text(ui, self.hours_edit));
        draft.minutes = Self::wcstoul(&Self::get_text(ui, self.minutes_edit));
        draft.page_number = self.current_page;
        if draft != self.info {
            self.info = draft.clone();
            ui.requests.emit(UiRequest::Journal(JournalAction::Edit {
                generation: self.shared.generation,
                page: self.current_page,
                draft,
            }));
            self.saves += 1;
        }
    }

    /// The tail [`Self::goto_page`], [`Self::clear_current_page`] and [`Self::update`] share: the
    /// three timer values into their boxes, the three redraws, and then whichever timer face the
    /// running flag selects.
    fn redraw(&mut self, ui: &mut UiSystem) {
        Self::set_text(ui, self.days_edit, &format!("{}", self.info.days));
        Self::set_text(ui, self.hours_edit, &format!("{}", self.info.hours));
        Self::set_text(ui, self.minutes_edit, &format!("{}", self.info.minutes));
        self.update_running_timer(ui);
        self.update_location(ui);
        self.update_page_number(ui);
        if self.info.timer_running {
            self.show_running_timer(ui);
        } else {
            self.show_editable_timer(ui);
        }
    }

    /// The journal panel's goto page.
    ///
    /// The guard is `page <= page count` **unsigned and with no lower bound**, so going to page 0
    /// indexes one before the first page. That is a real out-of-range read in the client and is
    /// the one place this file refuses rather than reproduces: nothing reaches it, because every
    /// call site passes `1`, the current page ± 1 behind an is-last-page / `!= 1` test, or a page
    /// number out of the vector.
    pub fn goto_page(&mut self, ui: &mut UiSystem, page: u32) {
        self.save_this_page(ui);
        ui.requests
            .emit(UiRequest::Journal(JournalAction::Goto(page)));
    }

    /// Clear both the stored page and
    /// [`Self::info`], then the same redraw. [`Self::delete_page`]'s one-page arm.
    pub fn clear_current_page(&mut self, ui: &mut UiSystem) {
        ui.requests
            .emit(UiRequest::Journal(JournalAction::Delete(self.current_page)));
    }

    /// Append a blank page numbered `size + 1`.
    pub fn new_page(&mut self, ui: &mut UiSystem) {
        self.save_this_page(ui);
        ui.requests.emit(UiRequest::Journal(JournalAction::NewPage));
    }

    /// The journal panel's is-last-page — the current page equals the page count.
    #[must_use]
    pub fn is_last_page(&self) -> bool {
        self.current_page as usize == self.pages.len()
    }

    /// With one page left it clears it in place; otherwise
    /// it erases, **renumbers every remaining page from 1**, and goes to page 1.
    pub fn delete_page(&mut self, ui: &mut UiSystem, page: u32) {
        self.save_this_page(ui);
        ui.requests
            .emit(UiRequest::Journal(JournalAction::Delete(page)));
    }

    /// The three redraws and the timer face, with no page move.
    /// Raised by the panel's own `0x18` when it becomes visible.
    pub fn update(&mut self, ui: &mut UiSystem) {
        if !self.loaded {
            return;
        }
        self.redraw(ui);
    }

    /// Project the shared notebook after capturing edits still in the controls.
    pub fn load(&mut self, ui: &mut UiSystem, view: &dyn GameView) {
        self.save_this_page(ui);
        let state = view.journal();
        if self.shared == state {
            return;
        }
        self.loaded = state.loaded;
        self.page_loads = state.page_loads;
        self.page_saves = state.page_saves;
        self.pages = state.pages.clone();
        self.info = state.draft.clone();
        self.current_page = state.current_page;
        self.file = view.journal_identity().map(|id| id.client_path());
        self.shared = state;
        Self::set_text(ui, self.label, &self.info.label.clone());
        Self::set_text(ui, self.title, &self.info.title.clone());
        Self::set_text(ui, self.notes, &self.info.notes.clone());
        self.redraw(ui);
    }

    /// Capture the outgoing controls and flush through the shared notebook.
    pub fn hand_over(&mut self, ui: &mut UiSystem) {
        self.save_this_page(ui);
        ui.requests
            .emit(UiRequest::Journal(JournalAction::Visibility(false)));
    }

    /// Discard this interface's projection; the shared notebook remains intact.
    pub fn forget(&mut self) {
        self.loaded = false;
        self.file = None;
        self.pages.clear();
        self.info = JournalPage::default();
        self.current_page = 0;
        self.shared = JournalView::default();
    }

    /// The client's `3` arm: while the panel is **visible** and at most
    /// twice a second, redraw the countdown.
    ///
    /// The visibility test is the panel's own visible flag, which is what
    /// stops a journal page counting down in a panel nobody has open — and it is why the
    /// countdown only moves once the player has actually opened the tab.
    pub fn tick(&mut self, ui: &mut UiSystem) -> bool {
        let visible = self
            .panel
            .and_then(|h| ui.node(h))
            .is_some_and(|n| n.region.flags.visible);
        if !visible || ui.now.0 < self.time_next_update {
            return false;
        }
        self.update_running_timer(ui);
        self.time_next_update = ui.now.0 + TICK_PERIOD;
        true
    }

    /// The journal panel's visibility-changed handler, whole, including the page save.
    ///
    /// After the base handler, nothing happens unless loaded; then, when shown, the pages are
    /// sorted by page number, and on either edge this page is saved and the pages are written
    /// with the stem `"Journal"`.
    ///
    /// The sort runs only on the *shown* edge; the save-page / save-pages pair runs on both,
    /// which is what commits an edit when the player switches away to the page list — and it is
    /// the **only** thing in the client that ever writes the file. There is no save on exit and no
    /// timer: close the panel, and the journal is on disk.
    pub fn on_visibility_changed(&mut self, ui: &mut UiSystem, visible: bool) {
        if !self.loaded {
            return;
        }
        self.save_this_page(ui);
        ui.requests
            .emit(UiRequest::Journal(JournalAction::Visibility(visible)));
    }

    /// The journal panel's element-message handler, whole.
    ///
    /// Returns true when an arm consumed the message. The `0x18` arm is first and is **not**
    /// exclusive with the `1` arm in the client either — it is a separate `if`, not an `else`.
    pub fn on_element_message(
        &mut self,
        ui: &mut UiSystem,
        m: &dereth_ui::ElementMessage,
        view: &dyn GameView,
    ) -> bool {
        use dereth_ui::msg::element::id;
        if !self.loaded {
            return false;
        }
        if m.id == id::VISIBILITY_CHANGED && m.source_id == PANEL {
            // The message is the panel's own, and `dwParam1 != 0` means shown.
            if m.p1 != 0 {
                self.update(ui);
            }
            self.on_visibility_changed(ui, m.p1 != 0);
            return true;
        }
        if m.id != id::BUTTON_CLICKED {
            return false;
        }
        match m.source_id {
            PREV_PAGE_BUTTON | NEXT_PAGE_BUTTON => {
                self.save_this_page(ui);
                let delta = if m.source_id == PREV_PAGE_BUTTON {
                    -1
                } else {
                    1
                };
                ui.requests
                    .emit(UiRequest::Journal(JournalAction::Turn(delta)));
            }
            NEW_PAGE_BUTTON => self.new_page(ui),
            FIRST_PAGE_BUTTON => self.goto_page(ui, 1),
            LAST_PAGE_BUTTON => {
                self.goto_page(ui, u32::try_from(self.pages.len()).unwrap_or(u32::MAX))
            }
            STAMP_LOCATION_BUTTON => self.reset_location(ui, view),
            START_TIMER_BUTTON => {
                self.reset_timer(ui);
            }
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The five formats each have a trailing space; the unconditional seconds run ends by
    /// dropping the last character, which eats the last space.
    #[test]
    fn delta_time_to_string_matches_the_five_formats_and_drops_the_trailing_space() {
        assert_eq!(delta_time_to_string(0), "0s");
        assert_eq!(delta_time_to_string(50), "50s");
        assert_eq!(delta_time_to_string(60), "1m 0s");
        assert_eq!(delta_time_to_string(90), "1m 30s");
        assert_eq!(
            delta_time_to_string(3600),
            "1h 0s",
            "minutes is zero and is skipped"
        );
        assert_eq!(delta_time_to_string(3661), "1h 1m 1s");
        assert_eq!(delta_time_to_string(86_400), "1d 0s");
        assert_eq!(delta_time_to_string(2_592_000), "1mo 0s");
        assert_eq!(
            delta_time_to_string(2_592_000 + 86_400 + 3661),
            "1mo 1d 1h 1m 1s"
        );
        assert!(!delta_time_to_string(1).ends_with(' '));
    }

    /// The three formatting arms and their subtraction of the current time establish the timer
    /// behavior.
    #[test]
    fn the_timer_text_has_exactly_three_arms() {
        assert_eq!(timer_text(false, 1000.0, 0.0), NONE, "not running");
        assert_eq!(timer_text(true, 0.0, 0.0), NONE, "stamp <= 0 is also None");
        assert_eq!(timer_text(true, -5.0, 0.0), NONE);
        assert_eq!(timer_text(true, 100.0, 100.0), READY, "remaining <= 0");
        assert_eq!(timer_text(true, 99.0, 100.0), READY);
        assert_eq!(timer_text(true, 190.0, 100.0), "1m 30s");
    }

    /// Oracle: the client's argument order and its `== 0` arm. `ns` first, `ew`
    /// second; an exactly-zero coordinate uses an empty direction suffix.
    #[test]
    fn the_location_line_is_north_south_first_and_zero_has_no_letter() {
        assert_eq!(location_text(false, 42.2, 33.8), NONE);
        assert_eq!(location_text(true, 42.2, 33.8), "42.2N, 33.8E");
        assert_eq!(location_text(true, -42.2, -33.8), "42.2S, 33.8W");
        assert_eq!(location_text(true, 0.0, 0.0), "0.0, 0.0");
    }

    /// Oracle: the client's page-number label — a zero or negative current page prints 1.
    #[test]
    fn the_page_number_label_is_tilde_wrapped_and_never_prints_zero() {
        assert_eq!(page_number_text(1), "~ 1 ~");
        assert_eq!(page_number_text(12), "~ 12 ~");
        assert_eq!(page_number_text(0), "~ 1 ~");
    }

    /// Oracle: the client's `wcstoul(str, NULL, 0)` — base 0, no error checking.
    #[test]
    fn the_timer_boxes_are_parsed_the_way_wcstoul_base_zero_parses_them() {
        assert_eq!(JournalPanel::wcstoul("12"), 12);
        assert_eq!(JournalPanel::wcstoul(""), 0);
        assert_eq!(
            JournalPanel::wcstoul("abc"),
            0,
            "unparseable contributes nothing"
        );
        assert_eq!(
            JournalPanel::wcstoul("12abc"),
            12,
            "strtoul stops at the first non-digit"
        );
        assert_eq!(
            JournalPanel::wcstoul("0x10"),
            16,
            "base 0 means a 0x prefix is hex"
        );
    }

    /// Oracle: `crate::panels::catalogue` — the page is a stack page, the panel and the tab are
    /// not, and the thirteen bound ids are the catalogue's.
    #[test]
    fn the_thirteen_children_are_the_catalogued_ones() {
        assert!(crate::panels::catalogue::PANEL_PAGES.contains(&PAGE.0));
        assert!(!crate::panels::catalogue::PANEL_PAGES.contains(&PANEL.0));
        let spec = crate::panels::catalogue::spec("JournalPanel").expect("catalogued");
        let ids: Vec<u32> = spec.children.iter().map(|c| c.id.0).collect();
        for id in [
            LABEL_EDIT,
            TITLE_EDIT,
            NOTES_EDIT,
            PAGE_NUMBER_TEXT,
            LOCATION_TEXT,
            DAYS_EDIT,
            DAYS_TEXT,
            HOURS_EDIT,
            HOURS_TEXT,
            MINUTES_EDIT,
            MINUTES_TEXT,
            TIMER_TEXT,
            START_TIMER_BUTTON,
        ] {
            assert!(
                ids.contains(&id.0),
                "{id:?} is not in the JournalPanel binding table"
            );
        }
        assert_eq!(ids.len(), 13);
    }

    /// Oracle: the client's four path arguments, in `sprintf` order: directory, stem, world,
    /// character.
    #[test]
    fn the_journal_path_is_the_directory_the_stem_the_world_and_the_character() {
        let id = JournalIdentity {
            directory: std::path::PathBuf::from("C:/ac"),
            world: "Frostfell".into(),
            character: "Kupo".into(),
        };
        assert_eq!(
            id.client_path()
                .file_name()
                .and_then(std::ffi::OsStr::to_str),
            Some("Journal-Frostfell-Kupo.txt"),
            "the path's second part is the literal \"Journal\", which this client writes too"
        );
        assert_eq!(
            STEM, "Journal",
            "no prefix; the settings directory is the separation"
        );
    }

    /// Oracle: the twelve `fwrite`s of the page save and the `else` ladder of
    /// the page load. Everything a page carries goes out and comes back.
    #[test]
    fn every_field_of_a_page_survives_the_round_trip() {
        let pages = vec![
            JournalPage {
                label: "a label with spaces".into(),
                title: "Title".into(),
                notes: "one\ntwo\nthree".into(),
                page_number: 1,
                timer_stamp: 1234.5,
                days: 1,
                hours: 2,
                minutes: 3,
                ns: -42.25,
                ew: 33.5,
                timer_running: true,
                location_set: true,
            },
            JournalPage {
                page_number: 2,
                ..JournalPage::default()
            },
        ];
        let text = save_pages_text(&pages);
        assert!(
            !text.contains(tag::PAGE_NUMBER),
            "the save writes twelve records, not thirteen"
        );
        let back = parse_pages(&text).expect("the file opens with <NEWP>");
        assert_eq!(back, pages);
    }

    /// Oracle: the client -- `strtok` / `sscanf("%d")`, then on to the next line with **no store**.
    /// The page number comes from the `<NEWP>` arm's page count, so a file whose `<PNUM>` disagrees is
    /// read in file order and the tag is thrown away.
    #[test]
    fn pnum_is_accepted_and_discarded() {
        let text = "<NEWP>\n<PNUM> 97\n<TITL> first\n\n<NEWP>\n<PNUM> 4\n<TITL> second\n\n";
        let pages = parse_pages(text).expect("it opens with <NEWP>");
        assert_eq!(pages.len(), 2);
        assert_eq!(
            (pages[0].page_number, pages[1].page_number),
            (1, 2),
            "position, not <PNUM>"
        );
        assert_eq!(pages[1].title, "second");
    }

    /// Oracle: the page load's first arm -- the only refusal the function has, and the only caller of
    /// the in-scroll report. An unknown tag *inside* a file is the ladder's fall-through
    /// and is silently ignored.
    #[test]
    fn a_file_that_does_not_open_with_new_page_is_the_one_complaint() {
        assert!(parse_pages("<TITL> orphan\n").is_err());
        assert!(
            parse_pages("   \n\n<TITL> orphan\n").is_err(),
            "blank lines do not count"
        );
        let pages = parse_pages("<NEWP>\n<WHAT> ?\n<TITL> kept\n").expect("it opens with <NEWP>");
        assert_eq!(
            pages[0].title, "kept",
            "an unknown tag falls through the ladder"
        );
        assert_eq!(
            LOAD_COMPLAINT_CHANNEL, 0x1A,
            "the complaint goes to chat channel 0x1A"
        );
    }

    /// Oracle: `<LABE>`/`<TITL>`/`<NOTE>` are `strtok(NULL, "\n\r")` -- the rest of the line,
    /// spaces and all -- while `<LOC?>`/`<TIM?>` compare against the literal `TRUE` and everything
    /// else is false.
    #[test]
    fn the_line_rest_arms_keep_their_spaces_and_the_boolean_arms_only_accept_true() {
        let text = "<NEWP>\n<LABE>  two  spaces \n<LOC?> TRUE\n<TIM?> true\n";
        let p = &parse_pages(text).expect("opens")[0];
        assert_eq!(
            p.label, " two  spaces ",
            "only the one delimiter strtok overwrote is eaten"
        );
        assert!(p.location_set);
        assert!(
            !p.timer_running,
            "the comparison is with \"TRUE\" and it is case sensitive"
        );
    }
}
