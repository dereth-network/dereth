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
    let mut titles = t.titles;
    titles.sort_by_key(|t| t.1.to_lowercase());
    (t.display, titles)
}

pub(super) fn era_has(
    game: &dyn GameView,
    which: fn(&dereth_primitives::era::EraFeatures) -> bool,
) -> bool {
    game.era().is_none_or(|e| which(&e.features()))
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
        let can_set = self.selected.is_some_and(|s| s != display);
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
                if let Some(title_id) = self.selected {
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
#[derive(Debug, Default)]
pub struct Journal {
    pages: Vec<JournalPage>,
    /// The page shown, from 1.
    current: u32,
    /// The file, once the world and the character are known.
    file: Option<std::path::PathBuf>,
    /// The three timer boxes as typed.
    boxes: [String; 3],
    /// On a world with contracts as well, the page's other tab, and whether it is the one shown.
    contracts: Contracts,
    on_contracts: bool,
}

/// Whether the journal page has a Contracts tab beside its own: on a world with both.
fn quest_tabs(game: &dyn GameView) -> bool {
    era_has(game, |e| e.journal) && era_has(game, |e| e.contracts)
}

use dereth_presentation::journal::{self as rules, JournalPage};

impl Journal {
    /// Read the journal the first time the world and the character are known. A file that does
    /// not open with a page is the client's one complaint, and leaves a single blank page.
    fn load(&mut self, game: &dyn GameView) -> Vec<PanelAction> {
        let Some(path) = game.journal_identity().map(|id| id.client_path()) else {
            return vec![];
        };
        if self.file.as_ref() == Some(&path) {
            return vec![];
        }
        let mut out = vec![];
        self.pages = match std::fs::read_to_string(&path).map(|t| rules::parse_pages(&t)) {
            Ok(Ok(pages)) => pages,
            Ok(Err(())) => {
                out.push(PanelAction::Host(HostAction::LocalFeedback {
                    text: rules::LOAD_COMPLAINT.into(),
                    severity: crate::panels::FeedbackSeverity::Warning,
                }));
                vec![]
            }
            Err(_) => vec![],
        };
        self.file = Some(path);
        if self.pages.is_empty() {
            self.pages.push(JournalPage {
                page_number: 1,
                ..JournalPage::default()
            });
        }
        self.show(1);
        out
    }

    fn save(&self) {
        if let Some(path) = &self.file {
            let _ = std::fs::write(path, rules::save_pages_text(&self.pages));
        }
    }

    fn page(&self) -> Option<&JournalPage> {
        self.pages.get(self.current.checked_sub(1)? as usize)
    }

    fn page_mut(&mut self) -> Option<&mut JournalPage> {
        let i = self.current.checked_sub(1)? as usize;
        self.pages.get_mut(i)
    }

    /// Show page `n` (from 1), its timer boxes filled from it.
    fn show(&mut self, n: u32) {
        if n == 0 || n as usize > self.pages.len() {
            return;
        }
        self.current = n;
        let p = &self.pages[n as usize - 1];
        self.boxes = [p.days, p.hours, p.minutes].map(|v| v.to_string());
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
        if !quest_tabs(c.game) {
            let (mut f, h) = page("Journal");
            self.body(&mut f, h, c);
            return f;
        }
        let (mut f, h) = bare_page();
        page_tabs(
            &mut f,
            &[("tab-contracts", "Contracts"), ("tab-journal", "Journal")],
            usize::from(!self.on_contracts),
        );
        if self.on_contracts {
            self.contracts.body(&mut f, h, c);
        } else {
            self.body(&mut f, h, c);
        }
        f
    }
    fn event(&mut self, e: ControlEvent, c: &Context<'_>) -> Vec<PanelAction> {
        if quest_tabs(c.game) {
            match &e {
                ControlEvent::Activate(id) if id == "tab-contracts" || id == "tab-journal" => {
                    self.on_contracts = id == "tab-contracts";
                    return vec![];
                }
                ControlEvent::Activate(id) if id == "close" => return vec![PanelAction::Close],
                _ if self.on_contracts => return self.contracts.body_event(e, c),
                _ => {}
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
        if matches!(&e, ControlEvent::Activate(id) if id == "close") {
            return vec![PanelAction::Close];
        }
        if !era_has(c.game, |e| e.journal) {
            return vec![];
        }
        let out = self.load(c.game);
        if self.page().is_none() {
            return out;
        }
        let mut changed = true;
        match e {
            ControlEvent::Edit { id, text } => match id.as_str() {
                "label" | "title" | "notes" => {
                    if let Some(p) = self.page_mut() {
                        match id.as_str() {
                            "label" => p.label = text,
                            "title" => p.title = text,
                            _ => p.notes = text,
                        }
                    }
                }
                "days" | "hours" | "minutes" => {
                    let i = ["days", "hours", "minutes"]
                        .iter()
                        .position(|b| *b == id)
                        .unwrap_or(0);
                    self.boxes[i] = text;
                    changed = false;
                }
                _ => changed = false,
            },
            ControlEvent::Select { id, index } if id == "pages" => {
                self.show(u32::try_from(index + 1).unwrap_or(1));
                changed = false;
            }
            ControlEvent::Activate(id) => match id.as_str() {
                "first" => self.show(1),
                "prev" if self.current > 1 => self.show(self.current - 1),
                "next" if self.current < self.last() => self.show(self.current + 1),
                "last" => self.show(self.last()),
                "new" => {
                    self.pages.push(JournalPage {
                        page_number: self.last() + 1,
                        ..JournalPage::default()
                    });
                    self.show(self.last());
                }
                "delete" => {
                    if self.pages.len() < 2 {
                        if let Some(p) = self.page_mut() {
                            *p = JournalPage {
                                page_number: p.page_number,
                                ..JournalPage::default()
                            };
                        }
                    } else {
                        self.pages.remove(self.current as usize - 1);
                        rules::renumber(&mut self.pages);
                    }
                    self.show(1);
                }
                // The coordinates are cleared before they are read, so a failed read leaves the
                // page at no location rather than the last one.
                "stamp" => {
                    let at = c.game.player_coords();
                    if let Some(p) = self.page_mut() {
                        p.ns = 0.0;
                        p.ew = 0.0;
                        p.location_set = at.is_some();
                        if let Some((ns, ew)) = at {
                            p.ns = f64::from(ns);
                            p.ew = f64::from(ew);
                        }
                    }
                }
                "timer" => {
                    let now = c.game.now();
                    let counts = self.boxes.clone().map(|b| rules::parse_count(&b));
                    if let Some(p) = self.page_mut() {
                        if p.timer_running {
                            p.timer_stamp = 0.0;
                            p.timer_running = false;
                        } else {
                            [p.days, p.hours, p.minutes] = counts;
                            p.timer_stamp = rules::timer_stamp(now, p.days, p.hours, p.minutes);
                            p.timer_running = true;
                        }
                    }
                }
                _ => changed = false,
            },
            _ => changed = false,
        }
        if changed {
            self.save();
        }
        out
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
    }
    impl GameView for Game {
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
                titles: vec![(2, "Wanderer".into()), (5, "Apprentice".into())],
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
    fn with(game: &Game, run: impl FnOnce(&Context<'_>)) {
        let (state, pregame, keyboard, settings) = Default::default();
        run(&Context {
            game,
            pregame: &pregame,
            keyboard: &keyboard,
            settings: &settings,
            map_teleport_allowed: false,
            classic: &state,
        });
    }

    #[test]
    fn a_title_picked_from_the_list_is_set_as_the_shown_title() {
        let mut titles = Titles::default();
        with(&Game::default(), |c| {
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

    fn act(j: &mut Journal, id: &str, c: &Context<'_>) {
        j.event(ControlEvent::Activate(id.into()), c);
    }

    fn type_in(j: &mut Journal, id: &str, text: &str, c: &Context<'_>) {
        j.event(
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
            j.event(ControlEvent::Tick, c);
            type_in(&mut j, "label", "Quest", c);
            type_in(&mut j, "title", "Rats in the cellar", c);
            type_in(&mut j, "notes", "Five rats.\nSee the innkeeper.", c);
            act(&mut j, "stamp", c);
            type_in(&mut j, "days", "1", c);
            type_in(&mut j, "minutes", "0x1e", c);
            act(&mut j, "timer", c);
        });
        let file = dir.join("Journal-Dereth-Scribe.txt");
        let text = std::fs::read_to_string(&file).expect("the page was written at once");
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
            j.event(ControlEvent::Tick, c);
            type_in(&mut j, "title", "one", c);
            act(&mut j, "new", c);
            type_in(&mut j, "title", "two", c);
            act(&mut j, "new", c);
            type_in(&mut j, "title", "three", c);
            assert_eq!(j.current, 3);
            act(&mut j, "prev", c);
            act(&mut j, "delete", c);
            assert_eq!(j.current, 1, "a deletion goes back to the first page");
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
            j.event(ControlEvent::Tick, c);
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
            j.event(ControlEvent::Tick, c);
            let f = j.frame(c);
            assert!(has(&f, "tab-contracts") && has(&f, "tab-journal") && has(&f, "stamp"));
            act(&mut j, "tab-contracts", c);
            let f = j.frame(c);
            assert!(has(&f, "abandon") && !has(&f, "stamp"));
            // The contracts tab works the contracts.
            j.event(
                ControlEvent::Select {
                    id: "rows".into(),
                    index: 0,
                },
                c,
            );
            assert_eq!(
                j.event(ControlEvent::Activate("abandon".into()), c),
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
            j.event(ControlEvent::Tick, c);
            type_in(&mut j, "label", "Rats", c);
            act(&mut j, "new", c);
            act(&mut j, "new", c);
            assert_eq!(
                list(&j, c),
                (vec!["1: Rats".into(), "~ 2 ~".into(), "~ 3 ~".into()], 2)
            );
            j.event(
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
}
