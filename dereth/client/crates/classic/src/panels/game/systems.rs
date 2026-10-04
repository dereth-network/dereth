//! The classic interface's pages for the systems the game gained after its 2005 redesign: the
//! character's titles, the contract tracker and the journal. Each is drawn in the classic side
//! pages' style (their background, title bar and rows) and shows only what the world's era has: a
//! world without titles, contracts or the journal shows a page that says so.
use super::super::*;
use super::common::*;
use crate::int::i32_from;

/// The page's list of rows: 32 pixels each, under the title bar.
const ROW: i32 = 32;
const TOP: i32 = 25;

/// A side page's frame: background, title bar, title and close button.
fn page(title: &str) -> (PanelFrame, i32) {
    let (mut f, h) = bare_page();
    image(&mut f, 0x0600127b, rect(0, 0, 276, 25), None, true, false);
    text(
        &mut f,
        rect(2, 2, 272, 21),
        title,
        "16-7",
        CREAM,
        1,
        false,
        None,
    );
    (f, h)
}

/// A side page's background and close button, for a page whose top row is its tabs.
pub(super) fn bare_page() -> (PanelFrame, i32) {
    let height = crate::panels::side_height();
    let mut f = PanelFrame::new(300, height);
    let h = height as i32;
    image(&mut f, 0x06001398, rect(0, 0, 300, h), None, true, false);
    close(&mut f, 0x06001393, 0x06001394);
    (f, h)
}

/// A row of tabs across the top of a page, `(id, caption)` each, the one at `shown` held down.
pub(super) fn page_tabs(f: &mut PanelFrame, tabs: &[(&str, &str)], shown: usize) {
    let width = 276 / i32_from(tabs.len().max(1));
    for (i, (id, caption)) in tabs.iter().enumerate() {
        art(
            f.button(*id, rect(i32_from(i) * width, 0, width, 25), *caption, true),
            if i == shown { 0x06000f76 } else { 0x06000f77 },
            0x06000f76,
            0x06000f77,
        );
    }
}

/// One list row: the classic row background, highlighted when selected, and its text.
fn list_row(f: &mut PanelFrame, y: i32, name: &str, detail: &str, selected: bool, clip: [i32; 4]) {
    image(
        f,
        if selected { 0x06001397 } else { 0x06001396 },
        rect(0, y, 300, ROW),
        Some(clip),
        false,
        false,
    );
    text(
        f,
        rect(8, y, 200, ROW),
        name,
        "16-7",
        CREAM,
        0,
        true,
        Some(clip),
    );
    text(
        f,
        rect(204, y, 74, ROW),
        detail,
        "15-6",
        CREAM,
        2,
        false,
        Some(clip),
    );
}

/// The character's titles: the earned ones in name order, the displayed one marked, and a button
/// that makes the selected one the title shown under the character's name.
#[derive(Debug, Default)]
pub struct Titles {
    selected: Option<u32>,
    scroll: i32,
}

fn sorted_titles(game: &dyn GameView) -> (u32, Vec<(u32, String)>) {
    let t = game.character_titles();
    (t.display, dereth_presentation::stats::title_rows(&t.titles))
}

pub(super) fn era_has(
    game: &dyn GameView,
    which: fn(&dereth_primitives::era::EraFeatures) -> bool,
) -> bool {
    which(&game.era_features())
}

impl Panel for Titles {
    fn id(&self) -> &'static str {
        "titles"
    }
    fn frame(&self, c: &Context<'_>) -> PanelFrame {
        let (mut f, h) = page("Titles");
        self.body(&mut f, h, c);
        f
    }
    fn event(&mut self, e: ControlEvent, c: &Context<'_>) -> Vec<PanelAction> {
        self.body_event(e, c)
    }
}

impl Titles {
    /// The page under its top row of `f`, `h` tall.
    pub(super) fn body(&self, f: &mut PanelFrame, h: i32, c: &Context<'_>) {
        if !era_has(c.game, |e| e.titles) {
            text(
                f,
                rect(10, TOP + 10, 280, 40),
                "This world has no titles.",
                "16-7",
                CREAM,
                0,
                true,
                None,
            );
            return;
        }
        let (display, titles) = sorted_titles(c.game);
        let clip = [0, TOP, 280, h - 44];
        let max = (i32_from(titles.len()) * ROW - (h - 44 - TOP)).max(0);
        let scroll = self.scroll.clamp(0, max);
        for (i, (id, name)) in titles.iter().enumerate() {
            let y = TOP + i32_from(i) * ROW - scroll;
            if y + ROW <= TOP || y >= h - 44 {
                continue;
            }
            let shown = if *id == display { "shown" } else { "" };
            list_row(f, y, name, shown, self.selected == Some(*id), clip);
        }
        f.control(
            "rows",
            rect(0, TOP, 280, h - 44 - TOP),
            ControlKind::HitList {
                row_count: titles.len(),
                row_height: ROW,
                selected: titles.iter().position(|(id, _)| Some(*id) == self.selected),
                offset: scroll,
            },
            true,
        );
        f.control(
            "scroll",
            rect(280, TOP, 20, h - 44 - TOP),
            ControlKind::ScrollBar {
                min: 0,
                max,
                value: scroll,
                page: h - 44 - TOP,
                step: ROW,
                vertical: true,
                arrow_size: 16,
                thumb_size: 16,
            },
            true,
        );
        let can_set = dereth_presentation::stats::can_set_title(self.selected, display, &titles);
        f.button("set", rect(80, h - 40, 140, 36), "Set as Title", can_set);
    }
    pub(super) fn body_event(&mut self, e: ControlEvent, c: &Context<'_>) -> Vec<PanelAction> {
        match e {
            ControlEvent::Activate(id) if id == "close" => return vec![PanelAction::Close],
            ControlEvent::Select { id, index } if id == "rows" => {
                let (_, titles) = sorted_titles(c.game);
                if let Some((title, _)) = titles.get(index) {
                    self.selected = Some(*title);
                }
            }
            ControlEvent::Scroll { id, value } if id == "scroll" => self.scroll = value.max(0),
            ControlEvent::Activate(id) if id == "set" => {
                let (display, rows) = sorted_titles(c.game);
                if let Some(title_id) = self.selected.filter(|_| {
                    dereth_presentation::stats::can_set_title(self.selected, display, &rows)
                }) {
                    return vec![PanelAction::Game(UiRequest::SetDisplayCharacterTitle {
                        title_id,
                    })];
                }
            }
            _ => {}
        }
        vec![]
    }
}

/// The contract tracker: the character's contracts with their progress, the selected one's notes
/// and contact, and a button that abandons it.
#[derive(Debug, Default)]
pub struct Contracts {
    selected: Option<u32>,
    scroll: i32,
}

impl Panel for Contracts {
    fn id(&self) -> &'static str {
        "contracts"
    }
    fn frame(&self, c: &Context<'_>) -> PanelFrame {
        let (mut f, h) = page("Contracts");
        self.body(&mut f, h, c);
        f
    }
    fn event(&mut self, e: ControlEvent, c: &Context<'_>) -> Vec<PanelAction> {
        self.body_event(e, c)
    }
}

impl Contracts {
    /// The page under its top row of `f`, `h` tall.
    fn body(&self, f: &mut PanelFrame, h: i32, c: &Context<'_>) {
        if !era_has(c.game, |e| e.contracts) {
            text(
                f,
                rect(10, TOP + 10, 280, 40),
                "This world has no contracts.",
                "16-7",
                CREAM,
                0,
                true,
                None,
            );
            return;
        }
        let contracts = c.game.contracts();
        // The list takes the top half; the selected contract's notes the rest.
        let list_bottom = TOP + (h - TOP - 44) / 2;
        let clip = [0, TOP, 280, list_bottom];
        let max = (i32_from(contracts.len()) * ROW - (list_bottom - TOP)).max(0);
        let scroll = self.scroll.clamp(0, max);
        for (i, contract) in contracts.iter().enumerate() {
            let y = TOP + i32_from(i) * ROW - scroll;
            if y + ROW <= TOP || y >= list_bottom {
                continue;
            }
            list_row(
                f,
                y,
                &contract.name,
                &contract.status,
                self.selected == Some(contract.contract_id),
                clip,
            );
        }
        f.control(
            "rows",
            rect(0, TOP, 280, list_bottom - TOP),
            ControlKind::HitList {
                row_count: contracts.len(),
                row_height: ROW,
                selected: contracts
                    .iter()
                    .position(|k| Some(k.contract_id) == self.selected),
                offset: scroll,
            },
            true,
        );
        f.control(
            "scroll",
            rect(280, TOP, 20, list_bottom - TOP),
            ControlKind::ScrollBar {
                min: 0,
                max,
                value: scroll,
                page: list_bottom - TOP,
                step: ROW,
                vertical: true,
                arrow_size: 16,
                thumb_size: 16,
            },
            true,
        );
        if let Some(contract) = contracts
            .iter()
            .find(|k| Some(k.contract_id) == self.selected)
        {
            let notes = if contract.contact.is_empty() {
                contract.description.clone()
            } else {
                format!("{}\n\nContact: {}", contract.description, contract.contact)
            };
            text(
                f,
                rect(10, list_bottom + 6, 280, h - 44 - list_bottom - 6),
                notes,
                "16-7",
                CREAM,
                0,
                true,
                Some([0, list_bottom, 300, h - 44]),
            );
        }
        f.button(
            "abandon",
            rect(80, h - 40, 140, 36),
            "Abandon",
            self.selected.is_some(),
        );
    }
    fn body_event(&mut self, e: ControlEvent, c: &Context<'_>) -> Vec<PanelAction> {
        match e {
            ControlEvent::Activate(id) if id == "close" => return vec![PanelAction::Close],
            ControlEvent::Select { id, index } if id == "rows" => {
                if let Some(k) = c.game.contracts().get(index) {
                    self.selected = Some(k.contract_id);
                }
            }
            ControlEvent::Scroll { id, value } if id == "scroll" => self.scroll = value.max(0),
            ControlEvent::Activate(id) if id == "abandon" => {
                if let Some(contract_id) = self.selected.take() {
                    return vec![PanelAction::Game(UiRequest::AbandonContract {
                        contract_id,
                    })];
                }
            }
            _ => {}
        }
        vec![]
    }
}

/// The journal: the character's notebook, one page at a time, in the file the retail interface
/// keeps it in, so either interface shows the same pages.
///
/// The page's label, title and notes are edited in place; Stamp writes where the character stands,
/// and Start counts down the days, hours and minutes in the three boxes (Reset stops it). The
/// arrows turn the pages, New adds one at the end and Delete takes the shown one out. Every change
/// is written to the file at once, so nothing waits on the page being closed.
#[derive(Debug)]
pub struct Journal {
    pages: Vec<JournalPage>,
    /// The page shown, from 1.
    current: u32,
    /// The last shared notebook projection.
    shared: dereth_client_contract::journal::JournalView,
    /// The three timer boxes as typed.
    boxes: [String; 3],
    /// On a world with contracts as well, the window's Contracts tab.
    contracts: Contracts,
    /// The selected system or the journal's page list; resolved against this world's systems.
    tab: Option<QuestTab>,
    /// The Page List tab.
    list: PageList,
}

impl Default for Journal {
    fn default() -> Self {
        Self {
            pages: Vec::new(),
            current: 0,
            shared: Default::default(),
            boxes: Default::default(),
            contracts: Contracts::default(),
            tab: Some(QuestTab::Contracts),
            list: PageList::default(),
        }
    }
}

/// The journal window's tabs on a world with the journal: Contracts first where the world has
/// them, then the journal's page and its page list.
fn quest_tabs(game: &dyn GameView) -> Vec<(&'static str, &'static str, QuestTab)> {
    let mut tabs = vec![];
    if era_has(game, |e| e.contracts) {
        tabs.push(("tab-contracts", "Contracts", QuestTab::Contracts));
    }
    if game.era_features().journal {
        tabs.push(("tab-journal", "Journal", QuestTab::Journal));
        tabs.push(("tab-pages", "Page List", QuestTab::Pages));
    }
    tabs
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum QuestTab {
    Contracts,
    Journal,
    Pages,
}

/// The journal's Page List tab: every page with its number, title, timer and label, sorted by
/// the column header last pressed (pressed again, the order reverses), cut to the pages holding
/// the search text when Search is pressed (Reset empties the search). A second press on the
/// chosen row turns the journal to that page; Delete takes the chosen page out.
#[derive(Debug, Default)]
struct PageList {
    /// The pages listed, filtered and sorted.
    pages: Vec<JournalPage>,
    sort: rules::JournalSortCriteria,
    reverse: bool,
    /// The search box as typed.
    search: String,
    /// The chosen row, and when it was last pressed.
    selected: Option<usize>,
    pressed: f64,
    scroll: i32,
}

/// A Page List row's height.
const LIST_ROW: i32 = 20;

/// The Page List's columns: the header button, its caption, its left edge and width.
const COLUMNS: [(&str, &str, i32, i32); 4] = [
    ("sort-number", "#", 0, 34),
    ("sort-title", "Title", 34, 104),
    ("sort-timer", "Timer", 138, 70),
    ("sort-label", "Label", 208, 72),
];

impl PageList {
    /// List `pages` again, cut to the search text when `filter` is set, in the chosen order.
    fn rebuild(&mut self, pages: &[JournalPage], filter: bool) {
        let needle = if filter {
            self.search.clone()
        } else {
            String::new()
        };
        self.pages = pages
            .iter()
            .filter(|p| rules::page_contains_string(p, &needle))
            .cloned()
            .collect();
        let (by, reverse) = (self.sort, self.reverse);
        self.pages.sort_by(|a, b| {
            let o = rules::compare(a, b, by);
            if reverse {
                o.reverse()
            } else {
                o
            }
        });
        self.selected = None;
    }

    fn body(&self, f: &mut PanelFrame, h: i32, now: f64) {
        for (id, caption, x, w) in COLUMNS {
            f.button(id, rect(x, TOP + 2, w, 22), caption, true).font = "15-6".into();
        }
        let top = TOP + 26;
        let bottom = h - 72;
        let clip = [0, top, 280, bottom];
        let max = (i32_from(self.pages.len()) * LIST_ROW - (bottom - top)).max(0);
        let scroll = self.scroll.clamp(0, max);
        for (i, p) in self.pages.iter().enumerate() {
            let y = top + i32_from(i) * LIST_ROW - scroll;
            if y + LIST_ROW <= top || y >= bottom {
                continue;
            }
            image(
                f,
                if self.selected == Some(i) {
                    0x06001397
                } else {
                    0x06001396
                },
                rect(0, y, 280, LIST_ROW),
                Some(clip),
                false,
                false,
            );
            let timer = rules::timer_text(p.timer_running, p.timer_stamp, now);
            for ((_, _, x, w), cell) in COLUMNS.into_iter().zip([
                p.page_number.to_string(),
                p.title.clone(),
                timer,
                p.label.clone(),
            ]) {
                text(
                    f,
                    rect(x + 3, y + 2, w - 6, LIST_ROW - 4),
                    cell,
                    "15-6",
                    CREAM,
                    0,
                    false,
                    Some([x.max(clip[0]), clip[1], (x + w).min(clip[2]), clip[3]]),
                );
            }
        }
        f.control(
            "page-rows",
            rect(0, top, 280, bottom - top),
            ControlKind::HitList {
                row_count: self.pages.len(),
                row_height: LIST_ROW,
                selected: self.selected,
                offset: scroll,
            },
            true,
        );
        f.control(
            "page-scroll",
            rect(280, top, 20, bottom - top),
            ControlKind::ScrollBar {
                min: 0,
                max,
                value: scroll,
                page: bottom - top,
                step: LIST_ROW,
                vertical: true,
                arrow_size: 16,
                thumb_size: 16,
            },
            true,
        );
        f.button(
            "delete-page",
            rect(200, h - 68, 90, 28),
            "Delete",
            self.selected.is_some(),
        );
        f.button("search", rect(4, h - 36, 70, 28), "Search:", true);
        f.edit(
            "search-text",
            rect(78, h - 33, 136, 22),
            &self.search,
            100,
            false,
            true,
        );
        f.button(
            "reset-search",
            rect(220, h - 36, 70, 28),
            rules::RESET,
            true,
        );
    }
}

impl Journal {
    /// The tab shown on a world with the journal.
    fn shown_tab(&self, game: &dyn GameView) -> QuestTab {
        use dereth_client_contract::era::{quest_page, QuestPage};
        let current = self.tab.map(|t| match t {
            QuestTab::Contracts => QuestPage::Contracts,
            _ => QuestPage::Journal,
        });
        match quest_page(game.era_features(), current) {
            Some(QuestPage::Contracts) => QuestTab::Contracts,
            Some(QuestPage::Journal) if self.tab == Some(QuestTab::Pages) => QuestTab::Pages,
            _ => QuestTab::Journal,
        }
    }

    /// The Page List tab's events.
    fn list_event(&mut self, e: ControlEvent, c: &Context<'_>) -> Vec<PanelAction> {
        self.load(c.game);
        let action = match e {
            ControlEvent::Activate(id) => match id.as_str() {
                "sort-number" => JournalAction::Sort(0),
                "sort-title" => JournalAction::Sort(1),
                "sort-timer" => JournalAction::Sort(3),
                "sort-label" => JournalAction::Sort(2),
                "search" => JournalAction::Search(self.list.search.clone()),
                "reset-search" => JournalAction::ResetSearch,
                "delete-page" => {
                    let Some(p) = self.list.selected.and_then(|i| self.list.pages.get(i)) else {
                        return vec![];
                    };
                    JournalAction::Delete(p.page_number)
                }
                _ => return vec![],
            },
            ControlEvent::Edit { id, text } if id == "search-text" => {
                self.list.search = text;
                return vec![];
            }
            ControlEvent::Scroll { id, value } if id == "page-rows" || id == "page-scroll" => {
                self.list.scroll = value.max(0);
                return vec![];
            }
            ControlEvent::Select { id, index } if id == "page-rows" => {
                let again =
                    self.list.selected == Some(index) && c.game.now() - self.list.pressed <= 1.0;
                self.list.selected = (index < self.list.pages.len()).then_some(index);
                self.list.pressed = c.game.now();
                if !again {
                    return vec![];
                }
                let Some(p) = self.list.pages.get(index) else {
                    return vec![];
                };
                self.tab = Some(QuestTab::Journal);
                self.list.pressed = 0.0;
                return vec![
                    PanelAction::Game(UiRequest::Journal(JournalAction::Goto(p.page_number))),
                    PanelAction::Game(UiRequest::Journal(JournalAction::Visibility(true))),
                ];
            }
            _ => return vec![],
        };
        vec![PanelAction::Game(UiRequest::Journal(action))]
    }
}

use dereth_presentation::journal::{self as rules, JournalPage};

use dereth_client_contract::journal::{JournalAction, JournalField};

impl Journal {
    fn load(&mut self, game: &dyn GameView) -> Vec<PanelAction> {
        let state = game.journal();
        if state != self.shared {
            self.pages = state.pages.clone();
            self.current = state.current_page;
            self.boxes =
                [state.draft.days, state.draft.hours, state.draft.minutes].map(|v| v.to_string());
            self.list.sort = match state.sort {
                1 => rules::JournalSortCriteria::Title,
                2 => rules::JournalSortCriteria::Label,
                3 => rules::JournalSortCriteria::Timer,
                _ => rules::JournalSortCriteria::PageNumber,
            };
            self.list.reverse = state.reverse;
            self.list.search = state.search.clone();
            self.list.rebuild(&self.pages, state.filtered);
            self.shared = state;
        }
        vec![]
    }

    fn page(&self) -> Option<&JournalPage> {
        self.shared.loaded.then_some(&self.shared.draft)
    }
    fn last(&self) -> u32 {
        u32::try_from(self.pages.len()).unwrap_or(u32::MAX)
    }
}

/// A page as the page list names it: its number, and its label when it has one.
fn page_entry(p: &JournalPage) -> String {
    let label = p.label.trim();
    if label.is_empty() {
        rules::page_number_text(p.page_number)
    } else {
        format!("{}: {label}", p.page_number)
    }
}

impl Panel for Journal {
    fn id(&self) -> &'static str {
        "journal"
    }
    fn frame(&self, c: &Context<'_>) -> PanelFrame {
        if dereth_client_contract::era::quest_page(c.game.era_features(), None).is_none() {
            let (mut f, h) = page("Journal");
            self.body(&mut f, h, c);
            return f;
        }
        let (mut f, h) = bare_page();
        let tabs = quest_tabs(c.game);
        let shown = self.shown_tab(c.game);
        let captions: Vec<_> = tabs.iter().map(|t| (t.0, t.1)).collect();
        page_tabs(
            &mut f,
            &captions,
            tabs.iter().position(|t| t.2 == shown).unwrap_or(0),
        );
        match shown {
            QuestTab::Contracts => self.contracts.body(&mut f, h, c),
            QuestTab::Journal => self.body(&mut f, h, c),
            QuestTab::Pages => self.list.body(&mut f, h, c.game.now()),
        }
        f
    }
    fn event(&mut self, e: ControlEvent, c: &Context<'_>) -> Vec<PanelAction> {
        let available =
            dereth_client_contract::era::quest_page(c.game.era_features(), None).is_some();
        self.tab = available.then(|| self.shown_tab(c.game));
        if available {
            let tabs = quest_tabs(c.game);
            match &e {
                ControlEvent::Activate(id) if tabs.iter().any(|t| t.0 == id) => {
                    self.tab = tabs.iter().find(|t| t.0 == id).map(|t| t.2);
                    if self.tab == Some(QuestTab::Pages) {
                        self.load(c.game);
                        return vec![
                            PanelAction::Game(UiRequest::Journal(JournalAction::Visibility(false))),
                            PanelAction::Game(UiRequest::Journal(JournalAction::Search(
                                self.list.search.clone(),
                            ))),
                        ];
                    }
                    return vec![PanelAction::Game(UiRequest::Journal(
                        JournalAction::Visibility(self.tab == Some(QuestTab::Journal)),
                    ))];
                }
                ControlEvent::Activate(id) if id == "close" => {
                    return vec![
                        PanelAction::Game(UiRequest::Journal(JournalAction::Visibility(false))),
                        PanelAction::Close,
                    ]
                }
                _ => {}
            }
            match self.shown_tab(c.game) {
                QuestTab::Contracts => {
                    // The journal is read while its window is up on any tab.
                    let mut out = if c.game.era_features().journal {
                        self.load(c.game)
                    } else {
                        vec![]
                    };
                    out.extend(self.contracts.body_event(e, c));
                    return out;
                }
                QuestTab::Pages => return self.list_event(e, c),
                QuestTab::Journal => {}
            }
        }
        self.body_event(e, c)
    }
}

impl Journal {
    /// The page under its top row of `f`, `h` tall.
    fn body(&self, f: &mut PanelFrame, h: i32, c: &Context<'_>) {
        if !era_has(c.game, |e| e.journal) {
            text(
                f,
                rect(10, TOP + 10, 280, 40),
                "This world has no journal.",
                "16-7",
                CREAM,
                0,
                true,
                None,
            );
            return;
        }
        let Some(p) = self.page() else {
            return;
        };
        let caption = |f: &mut PanelFrame, y: i32, s: &str| {
            text(f, rect(10, y, 50, 20), s, "15-6", CREAM, 0, false, None);
        };
        caption(f, TOP + 6, "Label");
        f.edit(
            "label",
            rect(60, TOP + 6, 230, 20),
            p.label.clone(),
            100,
            false,
            true,
        );
        caption(f, TOP + 30, "Title");
        f.edit(
            "title",
            rect(60, TOP + 30, 230, 20),
            p.title.clone(),
            100,
            false,
            true,
        );
        caption(f, TOP + 56, "Notes");
        let notes_bottom = h - 146;
        f.edit(
            "notes",
            rect(10, TOP + 76, 280, notes_bottom - TOP - 76),
            p.notes.clone(),
            2000,
            true,
            true,
        );
        // Where the character stood, and the button that stamps it.
        text(
            f,
            rect(10, h - 106, 186, 22),
            format!(
                "Location: {}",
                rules::location_text(p.location_set, p.ns, p.ew)
            ),
            "15-6",
            CREAM,
            0,
            false,
            None,
        );
        f.button("stamp", rect(200, h - 110, 90, 28), "Stamp", true);
        // The timer: its countdown while it runs, else the three boxes.
        if p.timer_running {
            text(
                f,
                rect(10, h - 76, 186, 22),
                format!(
                    "Timer: {}",
                    rules::timer_text(p.timer_running, p.timer_stamp, c.game.now())
                ),
                "15-6",
                CREAM,
                0,
                false,
                None,
            );
        } else {
            for (i, (id, unit)) in [("days", "d"), ("hours", "h"), ("minutes", "m")]
                .into_iter()
                .enumerate()
            {
                let x = 10 + i32_from(i) * 62;
                f.edit(
                    id,
                    rect(x, h - 78, 40, 22),
                    self.boxes[i].clone(),
                    6,
                    false,
                    true,
                );
                text(
                    f,
                    rect(x + 42, h - 76, 16, 22),
                    unit,
                    "15-6",
                    CREAM,
                    0,
                    false,
                    None,
                );
            }
        }
        f.button(
            "timer",
            rect(200, h - 80, 90, 28),
            if p.timer_running {
                rules::RESET
            } else {
                rules::START
            },
            true,
        );
        // The page turns.
        let last = self.last();
        for (id, label, x, enabled) in [
            ("first", "|<", 4, self.current > 1),
            ("prev", "<", 40, self.current > 1),
            ("next", ">", 150, self.current < last),
            ("last", ">|", 186, self.current < last),
            ("new", "New", 224, true),
        ] {
            f.button(
                id,
                rect(x, h - 42, if id == "new" { 72 } else { 34 }, 34),
                label,
                enabled,
            );
        }
        // The page list: every page by number and label, the shown one chosen; choosing one
        // turns to it.
        let pages = f.control(
            "pages",
            rect(76, h - 38, 72, 25),
            ControlKind::Choice {
                options: self.pages.iter().map(page_entry).collect(),
                selected: (self.current as usize).saturating_sub(1),
            },
            true,
        );
        pages.list_skin = Some(crate::panels::ListSkin::BOOK);
        pages.font = "15-6".into();
        // The shown page, taken out.
        f.button("delete", rect(200, h - 142, 90, 28), "Delete", true);
    }
    fn body_event(&mut self, e: ControlEvent, c: &Context<'_>) -> Vec<PanelAction> {
        self.load(c.game);
        if !era_has(c.game, |e| e.journal) {
            return vec![];
        }
        let action = match e {
            ControlEvent::Edit { id, text } => {
                let field = match id.as_str() {
                    "label" => JournalField::Label(text),
                    "title" => JournalField::Title(text),
                    "notes" => JournalField::Notes(text),
                    "days" => JournalField::Days(rules::parse_count(&text)),
                    "hours" => JournalField::Hours(rules::parse_count(&text)),
                    "minutes" => JournalField::Minutes(rules::parse_count(&text)),
                    _ => return vec![],
                };
                JournalAction::SetField {
                    generation: self.shared.generation,
                    page: self.current,
                    field,
                }
            }
            ControlEvent::Select { id, index } if id == "pages" => {
                JournalAction::Goto(u32::try_from(index).unwrap_or(u32::MAX).saturating_add(1))
            }
            ControlEvent::Activate(id) => match id.as_str() {
                "first" => JournalAction::Goto(1),
                "prev" => JournalAction::Turn(-1),
                "next" => JournalAction::Turn(1),
                "last" => JournalAction::Goto(self.last()),
                "new" => JournalAction::NewPage,
                "delete" => JournalAction::Delete(self.current),
                "stamp" => JournalAction::StampLocation,
                "timer" => JournalAction::ToggleTimer,
                "close" => {
                    return vec![
                        PanelAction::Game(UiRequest::Journal(JournalAction::Visibility(false))),
                        PanelAction::Close,
                    ]
                }
                _ => return vec![],
            },
            _ => return vec![],
        };
        vec![PanelAction::Game(UiRequest::Journal(action))]
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (classic panel adapter; the requests' behaviour is the runtime's).
    use super::*;
    use dereth_client_contract::view::{CharacterTitles, ContractEntry};

    #[derive(Debug, Default)]
    struct Game {
        era: Option<dereth_client_contract::EraView>,
        journal_dir: Option<std::path::PathBuf>,
        coords: Option<(f32, f32)>,
        now: f64,
        journal: std::cell::RefCell<dereth_client_model::journal::JournalStore>,
    }
    impl GameView for Game {
        fn journal(&self) -> dereth_client_contract::journal::JournalView {
            self.journal.borrow().view()
        }
        fn era(&self) -> Option<&dereth_client_contract::EraView> {
            self.era.as_ref()
        }
        fn journal_identity(&self) -> Option<dereth_client_contract::journal::JournalIdentity> {
            Some(dereth_client_contract::journal::JournalIdentity {
                directory: self.journal_dir.clone()?,
                world: "Dereth".into(),
                character: "Scribe".into(),
            })
        }
        fn player_coords(&self) -> Option<(f32, f32)> {
            self.coords
        }
        fn now(&self) -> f64 {
            self.now
        }
        fn character_titles(&self) -> CharacterTitles {
            CharacterTitles {
                display: 2,
                titles: vec![
                    (2, "Wanderer".into()),
                    (5, "Apprentice".into()),
                    (900, String::new()),
                    (0, "Nobody".into()),
                ],
            }
        }
        fn contracts(&self) -> Vec<ContractEntry> {
            vec![ContractEntry {
                contract_id: 9,
                name: "Rats".into(),
                status: "1/5".into(),
                ..Default::default()
            }]
        }
    }
    struct TestContext<'a> {
        game: &'a Game,
        context: Context<'a>,
    }
    impl<'a> std::ops::Deref for TestContext<'a> {
        type Target = Context<'a>;
        fn deref(&self) -> &Self::Target {
            &self.context
        }
    }
    fn service(game: &Game) {
        use dereth_client_contract::journal::JournalIo;
        let effects = game.journal.borrow_mut().take_io();
        for effect in effects {
            match effect {
                JournalIo::Load {
                    identity,
                    generation,
                    revision,
                } => {
                    let pages = std::fs::read_to_string(identity.client_path())
                        .map_or(Ok(vec![]), |s| rules::parse_pages(&s));
                    game.journal
                        .borrow_mut()
                        .complete_load(identity, generation, revision, pages);
                }
                JournalIo::Save {
                    identity, pages, ..
                } => {
                    std::fs::write(identity.client_path(), rules::save_pages_text(&pages)).unwrap();
                }
            }
        }
    }
    fn drive(j: &mut Journal, event: ControlEvent, c: &TestContext<'_>) -> Vec<PanelAction> {
        let out = j.event(event, c);
        for action in &out {
            if let PanelAction::Game(UiRequest::Journal(action)) = action {
                c.game
                    .journal
                    .borrow_mut()
                    .apply(action.clone(), c.game.now, c.game.coords);
            }
        }
        service(c.game);
        j.load(c.game);
        out
    }
    fn with(game: &Game, run: impl FnOnce(&TestContext<'_>)) {
        game.journal
            .borrow_mut()
            .set_identity(game.journal_identity());
        service(game);
        let (state, pregame, keyboard, settings) = Default::default();
        run(&TestContext {
            game,
            context: Context {
                now: dereth_primitives::LocalTime(0.0),
                game,
                pregame: &pregame,
                keyboard: &keyboard,
                settings: &settings,
                map_teleport_allowed: false,
                classic: &state,
            },
        });
    }

    #[test]
    fn a_title_picked_from_the_list_is_set_as_the_shown_title() {
        let mut titles = Titles::default();
        with(&Game::default(), |c| {
            // A title with no name, and title 0, are not listed.
            assert_eq!(
                sorted_titles(c.game).1,
                [(5, "Apprentice".to_string()), (2, "Wanderer".to_string())]
            );
            // The rows are in name order: Apprentice first.
            titles.event(
                ControlEvent::Select {
                    id: "rows".into(),
                    index: 0,
                },
                c,
            );
            assert_eq!(
                titles.event(ControlEvent::Activate("set".into()), c),
                [PanelAction::Game(UiRequest::SetDisplayCharacterTitle {
                    title_id: 5
                })]
            );
        });
    }

    fn journal_dir(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("classic-journal-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    fn act(j: &mut Journal, id: &str, c: &TestContext<'_>) {
        drive(j, ControlEvent::Activate(id.into()), c);
    }

    fn type_in(j: &mut Journal, id: &str, text: &str, c: &TestContext<'_>) {
        drive(
            j,
            ControlEvent::Edit {
                id: id.into(),
                text: text.into(),
            },
            c,
        );
    }

    #[test]
    fn a_journal_page_written_in_the_classic_interface_is_in_the_file_both_interfaces_read() {
        let dir = journal_dir("page");
        let game = Game {
            journal_dir: Some(dir.clone()),
            coords: Some((12.5, -33.25)),
            now: 100.0,
            ..Game::default()
        };
        let mut j = Journal::default();
        with(&game, |c| {
            drive(&mut j, ControlEvent::Tick, c);
            act(&mut j, "tab-journal", c);
            let before = std::fs::read(dir.join("Journal-Dereth-Scribe.txt")).unwrap();
            type_in(&mut j, "label", "Quest", c);
            type_in(&mut j, "title", "Rats in the cellar", c);
            type_in(&mut j, "notes", "Five rats.\nSee the innkeeper.", c);
            act(&mut j, "stamp", c);
            type_in(&mut j, "days", "1", c);
            type_in(&mut j, "minutes", "0x1e", c);
            act(&mut j, "timer", c);
            assert_eq!(
                std::fs::read(dir.join("Journal-Dereth-Scribe.txt")).unwrap(),
                before,
                "typing and timer changes do not write files"
            );
            act(&mut j, "close", c);
        });
        let file = dir.join("Journal-Dereth-Scribe.txt");
        let text = std::fs::read_to_string(&file).expect("closing saved the page");
        let pages = rules::parse_pages(&text).expect("in the retail format");
        let p = &pages[0];
        assert_eq!(
            (p.label.as_str(), p.title.as_str(), p.notes.as_str()),
            (
                "Quest",
                "Rats in the cellar",
                "Five rats.\nSee the innkeeper."
            )
        );
        assert!(p.location_set);
        assert_eq!((p.ns, p.ew), (12.5, -33.25));
        assert!(p.timer_running);
        assert_eq!(p.timer_stamp, 100.0 + 86_400.0 + 30.0 * 60.0);

        // A page opened afresh reads the file back and counts the timer down from it.
        let mut fresh = Journal::default();
        let later = Game {
            now: 100.0 + 86_400.0,
            ..game
        };
        with(&later, |c| {
            fresh.event(ControlEvent::Tick, c);
            act(&mut fresh, "tab-journal", c);
            let f = fresh.frame(c);
            let shown = format!("{f:?}");
            assert!(shown.contains("Rats in the cellar"));
            assert!(shown.contains("Timer: 30m 0s"), "{shown}");
            assert!(shown.contains("12.5N, 33.2W") || shown.contains("12.5N, 33.3W"));
            // Reset stops the timer and brings the boxes back.
            act(&mut fresh, "timer", c);
            assert!(fresh.frame(c).controls.iter().any(|k| k.id == "days"));
        });
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn new_pages_go_at_the_end_and_deleting_one_numbers_the_rest_from_one() {
        let dir = journal_dir("pages");
        let game = Game {
            journal_dir: Some(dir.clone()),
            ..Game::default()
        };
        let mut j = Journal::default();
        with(&game, |c| {
            drive(&mut j, ControlEvent::Tick, c);
            act(&mut j, "tab-journal", c);
            type_in(&mut j, "title", "one", c);
            act(&mut j, "new", c);
            type_in(&mut j, "title", "two", c);
            act(&mut j, "new", c);
            type_in(&mut j, "title", "three", c);
            assert_eq!(j.current, 3);
            act(&mut j, "prev", c);
            act(&mut j, "delete", c);
            assert_eq!(j.current, 1, "a deletion goes back to the first page");
            act(&mut j, "close", c);
        });
        let pages = rules::parse_pages(
            &std::fs::read_to_string(dir.join("Journal-Dereth-Scribe.txt")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            pages
                .iter()
                .map(|p| (p.page_number, p.title.as_str()))
                .collect::<Vec<_>>(),
            [(1, "one"), (2, "three")]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_world_without_the_journal_says_so() {
        let mut era = dereth_client_contract::EraView::default();
        era.era = dereth_primitives::era::EraId::Infiltration;
        era.era_announced = true;
        let game = Game {
            era: Some(era),
            journal_dir: Some(journal_dir("none")),
            ..Game::default()
        };
        let mut j = Journal::default();
        with(&game, |c| {
            drive(&mut j, ControlEvent::Tick, c);
            let f = j.frame(c);
            assert!(!f.controls.iter().any(|k| k.id == "label"));
            assert!(format!("{f:?}").contains("This world has no journal."));
        });
    }

    #[test]
    fn a_picked_contract_is_abandoned_and_a_world_without_contracts_says_so() {
        let mut contracts = Contracts::default();
        with(&Game::default(), |c| {
            contracts.event(
                ControlEvent::Select {
                    id: "rows".into(),
                    index: 0,
                },
                c,
            );
            assert_eq!(
                contracts.event(ControlEvent::Activate("abandon".into()), c),
                [PanelAction::Game(UiRequest::AbandonContract {
                    contract_id: 9
                })]
            );
        });
        let mut era = dereth_client_contract::EraView::default();
        era.era = dereth_primitives::era::EraId::Infiltration;
        era.era_announced = true;
        let plain = Game {
            era: Some(era),
            ..Game::default()
        };
        with(&plain, |c| {
            let f = contracts.frame(c);
            assert!(!f.controls.iter().any(|k| k.id == "abandon"));
        });
    }

    #[test]
    fn the_journal_page_has_a_contracts_tab_on_a_world_with_both() {
        let dir = journal_dir("tabs");
        let game = Game {
            journal_dir: Some(dir.clone()),
            ..Game::default()
        };
        let mut j = Journal::default();
        let has = |f: &PanelFrame, id: &str| f.controls.iter().any(|k| k.id == id);
        with(&game, |c| {
            drive(&mut j, ControlEvent::Tick, c);
            // The initialized Contracts selection remains valid.
            let f = j.frame(c);
            assert!(has(&f, "tab-contracts") && has(&f, "tab-journal") && has(&f, "tab-pages"));
            assert!(has(&f, "abandon") && !has(&f, "stamp"));
            act(&mut j, "tab-journal", c);
            assert!(has(&j.frame(c), "stamp"));
            act(&mut j, "tab-contracts", c);
            let f = j.frame(c);
            assert!(has(&f, "abandon") && !has(&f, "stamp"));
            // The contracts tab works the contracts.
            drive(
                &mut j,
                ControlEvent::Select {
                    id: "rows".into(),
                    index: 0,
                },
                c,
            );
            assert_eq!(
                drive(&mut j, ControlEvent::Activate("abandon".into()), c),
                [PanelAction::Game(UiRequest::AbandonContract {
                    contract_id: 9
                })]
            );
            act(&mut j, "tab-journal", c);
            assert!(has(&j.frame(c), "stamp"));
        });
        // A world with neither keeps the plain page.
        let mut era = dereth_client_contract::EraView::default();
        era.era = dereth_primitives::era::EraId::Infiltration;
        era.era_announced = true;
        let plain = Game {
            era: Some(era),
            ..Game::default()
        };
        with(&plain, |c| assert!(!has(&j.frame(c), "tab-contracts")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_page_list_names_every_page_and_turns_to_the_one_chosen() {
        let dir = journal_dir("list");
        let game = Game {
            journal_dir: Some(dir.clone()),
            ..Game::default()
        };
        let mut j = Journal::default();
        let list = |j: &Journal, c: &Context<'_>| {
            j.frame(c)
                .controls
                .into_iter()
                .find_map(|k| match k.kind {
                    ControlKind::Choice { options, selected } if k.id == "pages" => {
                        Some((options, selected))
                    }
                    _ => None,
                })
                .expect("the page list")
        };
        with(&game, |c| {
            drive(&mut j, ControlEvent::Tick, c);
            act(&mut j, "tab-journal", c);
            type_in(&mut j, "label", "Rats", c);
            act(&mut j, "new", c);
            act(&mut j, "new", c);
            assert_eq!(
                list(&j, c),
                (vec!["1: Rats".into(), "~ 2 ~".into(), "~ 3 ~".into()], 2)
            );
            drive(
                &mut j,
                ControlEvent::Select {
                    id: "pages".into(),
                    index: 0,
                },
                c,
            );
            assert_eq!(list(&j, c).1, 0);
            assert!(format!("{:?}", j.frame(c)).contains("Rats"));
        });
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_page_list_tab_sorts_searches_deletes_and_turns_to_a_page_pressed_twice() {
        let dir = journal_dir("pagelist");
        let mut game = Game {
            journal_dir: Some(dir.clone()),
            now: 10.0,
            ..Game::default()
        };
        let mut j = Journal::default();
        let titles = |j: &Journal| {
            j.list
                .pages
                .iter()
                .map(|p| p.title.clone())
                .collect::<Vec<_>>()
        };
        with(&game, |c| {
            drive(&mut j, ControlEvent::Tick, c);
            act(&mut j, "tab-journal", c);
            type_in(&mut j, "title", "Bravo", c);
            act(&mut j, "new", c);
            type_in(&mut j, "title", "alpha", c);
            act(&mut j, "new", c);
            type_in(&mut j, "title", "Charlie", c);
            type_in(&mut j, "notes", "the cellar", c);
            act(&mut j, "tab-pages", c);
            let f = j.frame(c);
            for id in [
                "sort-number",
                "sort-title",
                "sort-timer",
                "sort-label",
                "page-rows",
            ] {
                assert!(f.controls.iter().any(|k| k.id == id), "{id}");
            }
            assert_eq!(
                titles(&j),
                ["Bravo", "alpha", "Charlie"],
                "by page number at first"
            );
            act(&mut j, "sort-title", c);
            assert_eq!(
                titles(&j),
                ["alpha", "Bravo", "Charlie"],
                "titles ignore case"
            );
            act(&mut j, "sort-title", c);
            assert_eq!(
                titles(&j),
                ["Charlie", "Bravo", "alpha"],
                "pressed again, reversed"
            );
            type_in(&mut j, "search-text", "CELLAR", c);
            act(&mut j, "search", c);
            assert_eq!(titles(&j), ["Charlie"], "the notes are searched too");
            act(&mut j, "reset-search", c);
            assert_eq!(titles(&j).len(), 3);
            assert!(j.list.search.is_empty());
        });
        // Bravo is the middle row; pressed once it is chosen, pressed again it is opened.
        let select = |j: &mut Journal, c: &TestContext<'_>| {
            drive(
                j,
                ControlEvent::Select {
                    id: "page-rows".into(),
                    index: 1,
                },
                c,
            )
        };
        with(&game, |c| {
            select(&mut j, c);
            assert!(j.frame(c).controls.iter().any(|k| k.id == "delete-page"));
            assert!(!j.frame(c).controls.iter().any(|k| k.id == "stamp"));
        });
        game.now = 10.5;
        with(&game, |c| {
            select(&mut j, c);
            assert!(
                j.frame(c).controls.iter().any(|k| k.id == "stamp"),
                "the journal tab"
            );
            assert_eq!(j.page().map(|p| p.title.as_str()), Some("Bravo"));
            // Deleted from the list: the pages are numbered again.
            act(&mut j, "tab-pages", c);
            select(&mut j, c);
            act(&mut j, "delete-page", c);
            act(&mut j, "close", c);
        });
        let pages = rules::parse_pages(
            &std::fs::read_to_string(dir.join("Journal-Dereth-Scribe.txt")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            pages
                .iter()
                .map(|p| (p.page_number, p.title.as_str()))
                .collect::<Vec<_>>(),
            [(1, "alpha"), (2, "Charlie")]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
    /// Behaviour: presentation.era.shared-facts-follow-the-world-profile
    #[test]
    fn quest_tabs_preserve_system_selection_and_remove_unavailable_controls() {
        let mut j = Journal::default();
        let mut g = Game::default();
        let mut era = dereth_client_contract::EraView::default();
        for (journal, contracts, want) in [
            (true, true, QuestTab::Contracts),
            (false, true, QuestTab::Contracts),
            (true, false, QuestTab::Journal),
            (false, false, QuestTab::Journal),
        ] {
            era.announced_features.set("journal", journal);
            era.announced_features.set("contracts", contracts);
            g.era = Some(era.clone());
            with(&g, |c| {
                drive(&mut j, ControlEvent::Tick, c);
                let f = j.frame(c);
                let has = |id| f.controls.iter().any(|k| k.id == id);
                assert_eq!(has("tab-journal"), journal);
                assert_eq!(has("tab-pages"), journal);
                assert_eq!(has("tab-contracts"), contracts);
                assert_eq!(j.shown_tab(c.game), want);
                for id in ["tab-journal", "tab-pages", "tab-contracts"] {
                    if !has(id) {
                        let old = j.tab;
                        drive(&mut j, ControlEvent::Activate(id.into()), c);
                        assert_eq!(j.tab, old);
                    }
                }
            });
        }
        g.era = None;
        with(&g, |c| {
            act(&mut j, "tab-contracts", c);
            assert_eq!(j.shown_tab(c.game), QuestTab::Contracts);
            assert!(j.frame(c).controls.iter().any(|k| k.id == "abandon"));
            act(&mut j, "tab-pages", c);
            assert_eq!(j.shown_tab(c.game), QuestTab::Pages);
        });
        era.announced_features.set("journal", true);
        era.announced_features.set("contracts", false);
        g.era = Some(era);
        with(&g, |c| assert_eq!(j.shown_tab(c.game), QuestTab::Pages));
    }
}
