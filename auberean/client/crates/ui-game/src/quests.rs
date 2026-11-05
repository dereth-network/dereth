//! The quest window (a toolbar window): the journal of the quests under way and those finished.
//!
//! [`QuestWindow`] is the window: its button and key show and hide it (when hidden while a quest
//! changed, its button shows that something changed), its close button hides it, its title bar
//! moves it, and it comes to the front of the toolbar's windows when shown. Its pages are tabs:
//! under way, finished, the story.
//!
//! [`Journal`] keeps the player's quests as the scripts give them: the whole list at the session's
//! start (the player's own list, asked for through the interface), then each change. A quest under
//! way is on the under way page, one completed, failed or cancelled on the finished page, each list
//! newest given first. A row shows the quest's name and its time left (or none, or that it may be
//! taken no more); the quest that changed last is open, showing its description, its journal entry
//! and its challenge (or how it ended) and, for one under way that may be given up, a cancel
//! button, which asks first. A quest that changed has its row's name and time marked until the row
//! is pressed or the window hidden. Pressing a quest's row opens or closes it; hiding the window
//! closes the open one.

use std::any::Any;

use auberean_text::{StringInfo, Value, Variable};
use auberean_ui::framework::FrameId;
use auberean_ui::manager::global;
use auberean_ui::token::msg;
use auberean_ui::{Handle, Mail, Token, Ui};

use crate::examination::{window_to_front, BRING_TO_FRONT, WINDOW_FIELDS};
use crate::flow;
use crate::gameplay::{self, dialog, notice, Child, Dialog, Notice, Record, Request};
use crate::shortcuts::TOOLBAR_LAYOUT_ENUM;
use crate::time_display::TimeLeft;

/// A quest's status.
pub mod status {
    pub const UNDER_WAY: u32 = 0x1000_0000;
    pub const FAILED: u32 = 0x2000_0000;
    pub const COMPLETED: u32 = 0x3000_0000;
    pub const CANCELLED: u32 = 0x8000_0000;
}

/// What a quest notice says: one quest changed, the whole list, a quest added, one removed.
pub mod change {
    pub const ONE: u32 = 0x4000_0001;
    pub const ALL: u32 = 0x4000_0002;
    pub const ADDED: u32 = 0x4000_0003;
    pub const REMOVED: u32 = 0x4000_0004;
}

mod name {
    use auberean_ui::Token;
    pub const FIELD: Token = Token::of("QuestField");
    pub const BUTTON: Token = Token::of("QuestButton");
    pub const CLOSE_BUTTON: Token = Token::of("QuestCloseButton");
    pub const HELP_BUTTON: Token = Token::of("QuestHelpButton");
    pub const PANEL_TOP: Token = Token::of("QuestPanelTop");
    pub const UNDERWAY_PAGE: Token = Token::of("UnderwayPage");
    pub const FINISHED_PAGE: Token = Token::of("FinishedPage");
    pub const UNDERWAY_LIST: Token = Token::of("UnderwayQuestListBox");
    pub const FINISHED_LIST: Token = Token::of("FinishedQuestListBox");
    pub const UNDERWAY_TAB: Token = Token::of("UnderwayTab");
    pub const FINISHED_TAB: Token = Token::of("FinishedTab");
    pub const TEMPLATE: Token = Token::of("QuestTemplate");
    pub const EXTRA_TEXT: Token = Token::of("QuestExtraText");
    pub const CANCEL_ELEMENT: Token = Token::of("QuestCancelElement");
    pub const CANCEL_BUTTON: Token = Token::of("QuestCancelButton");
    pub const QUEST_NAME: Token = Token::of("QuestName");
    pub const QUEST_TIME: Token = Token::of("QuestTime");
    pub const ID: Token = Token::of("QuestIDToken");
    pub const DOWNLOAD_DONE: Token = Token::of("download_done");
    pub const NORMAL: Token = Token::of("normal");
    pub const HIGHLIGHT: Token = Token::of("highlight");
    pub const CHANGED: Token = Token::of("changed");
    pub const UNCHANGED: Token = Token::of("unchanged");
    pub const DESC: Token = Token::of("DESC");
    pub const JOURNAL: Token = Token::of("JOURNAL");
    pub const LEVEL: Token = Token::of("LEVEL");
}

/// A quest as the journal shows it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Quest {
    pub id: u32,
    pub name: StringInfo,
    pub description: StringInfo,
    pub journal: StringInfo,
    pub status: u32,
    /// How hard it is (-999: any level).
    pub challenge: i32,
    /// When it was given (game seconds).
    pub given: f64,
    /// It may be taken no more.
    pub maxed_out: bool,
    /// The seconds left before it runs out (under way) or may be taken again.
    pub seconds_remaining: i32,
    /// The player may give it up.
    // D2-12250
    pub cancellable: bool,
}

impl Quest {
    /// The open quest's text: under way, its description, its journal entry and its challenge
    /// (any level, or the level); finished, how it ended.
    // D2-12248
    #[must_use]
    pub fn extra_text(&self, table: u32) -> Option<StringInfo> {
        let text = |n: &str| StringInfo::entry(table, Token::of(n).raw());
        let ended = match self.status {
            status::UNDER_WAY => {
                let mut s = text("ID_Quest_Description");
                let mut var = |id: Token, value| {
                    s.variables.push(Variable {
                        id: id.raw(),
                        value,
                    })
                };
                var(name::DESC, Value::Text(Box::new(self.description.clone())));
                var(name::JOURNAL, Value::Text(Box::new(self.journal.clone())));
                var(
                    name::LEVEL,
                    if self.challenge == -999 {
                        Value::Text(Box::new(text("ID_Quest_ChallengeLevelAny")))
                    } else {
                        Value::Int(self.challenge)
                    },
                );
                return Some(s);
            }
            status::CANCELLED => "ID_Quest_Cancelled",
            status::COMPLETED => "ID_Quest_Completed",
            status::FAILED => "ID_Quest_Failed",
            _ => return None,
        };
        Some(text(ended))
    }
}

/// A quest in the journal with its rows: its own, its open text, its cancel button; the time
/// left shown in its row; whether it changed since last seen, whether it is open.
#[derive(Debug, Default)]
struct Descriptor {
    quest: Quest,
    element: Option<Handle>,
    extra: Option<Handle>,
    cancel: Option<Handle>,
    time: Option<TimeLeft>,
    updated: bool,
    expanded: bool,
}

impl Descriptor {
    // D2-02704
    fn new(quest: Quest) -> Self {
        Self {
            quest,
            ..Self::default()
        }
    }
}

/// The journal's two pages.
#[derive(Debug)]
pub struct Journal {
    frame: FrameId,
    underway_list: Option<Handle>,
    finished_list: Option<Handle>,
    underway: Vec<Descriptor>,
    finished: Vec<Descriptor>,
    /// The open quest's id (0: none).
    selected: u32,
    /// The lists are to be laid out again at the next tick.
    dirty: bool,
    /// The cancel question up, and the quest it asks about.
    question: u32,
    cancelling: u32,
}

/// Put a quest in a list newest given first: before the first given strictly earlier; one with
/// the same id already there is kept (`unique`).
fn insert_sorted(list: &mut Vec<Descriptor>, d: Descriptor, unique: bool) {
    if unique && list.iter().any(|x| x.quest.id == d.quest.id) {
        return;
    }
    let at = list
        .iter()
        .position(|x| x.quest.given < d.quest.given)
        .unwrap_or(list.len());
    list.insert(at, d);
}

impl Journal {
    /// The journal under the quest window's framework: its two lists found, its rows' and the
    /// clock's messages heard.
    // D2-02725
    pub fn new(ui: &mut Ui, parent: FrameId) -> Self {
        let frame = ui.new_child_frame(parent);
        let find = |ui: &Ui, page, list| ui.element(page).and_then(|p| ui.child_recursive(p, list));
        let underway_list = find(ui, name::UNDERWAY_PAGE, name::UNDERWAY_LIST);
        let finished_list = find(ui, name::FINISHED_PAGE, name::FINISHED_LIST);
        if underway_list.is_some() && finished_list.is_some() {
            if let Some(l) = ui.frame_listener(frame) {
                ui.listen_for_message(l, name::TEMPLATE, msg::BUTTON_RELEASE_ON);
                ui.listen_for_message(l, name::CANCEL_BUTTON, msg::BUTTON_RELEASE_ON);
                ui.listen_for_message(l, name::EXTRA_TEXT, name::DOWNLOAD_DONE);
                ui.listen_for_global_message(l, global::TIME);
            }
        }
        Self {
            frame,
            underway_list,
            finished_list,
            underway: Vec::new(),
            finished: Vec::new(),
            selected: 0,
            dirty: false,
            question: 0,
            cancelling: 0,
        }
    }

    /// The quests under way, then those finished, in the journal's order.
    pub fn quests(&self) -> impl Iterator<Item = &Quest> {
        self.underway.iter().chain(&self.finished).map(|d| &d.quest)
    }

    /// The quest of an id, under way or finished.
    // D2-02712
    fn find(&mut self, id: u32) -> Option<&mut Descriptor> {
        self.underway
            .iter_mut()
            .chain(self.finished.iter_mut())
            .find(|d| d.quest.id == id)
    }

    fn table(ui: &Ui) -> u32 {
        ui.assets()
            .data_id_by_enum(flow::MESSAGES, flow::TABLE_MAPPING)
    }

    /// A quest's time: that it may be taken no more; the time left, kept current by the clock (in
    /// months and days above a month, days and hours above a day, hours and minutes above an
    /// hour, else minutes and seconds); or none.
    // D2-02703
    fn quest_time(ui: &mut Ui, d: &mut Descriptor, time: Option<Handle>) {
        let Some(t) = time else { return };
        let q = &d.quest;
        let table = Self::table(ui);
        let text = if q.maxed_out {
            ui.assets().write(&StringInfo::entry(
                table,
                Token::of("ID_Quest_QuestMaxed").raw(),
            ))
        } else if q.seconds_remaining > 0 {
            let end = f64::from(q.seconds_remaining) + ui.now();
            let mut shown = TimeLeft::new(ui);
            shown.init(ui, Some(t), end, 1, 1, 60);
            if let Some(old) = d.time.replace(shown) {
                old.close(ui);
            }
            return;
        } else {
            ui.assets().write(&StringInfo::entry(
                table,
                Token::of("ID_Quest_TimeNone").raw(),
            ))
        };
        ui.set_text(t, &text.unwrap_or_default());
    }

    /// A quest's rows made at `index` of a list: its own (its id, its time), then, open, its text
    /// and (under way and cancellable) its cancel button; its name, marked when it changed.
    // D2-02706
    fn add_element(ui: &mut Ui, d: &mut Descriptor, list: Handle, index: usize) {
        let Some(e) = ui.frame_create_element(TOOLBAR_LAYOUT_ENUM, name::TEMPLATE) else {
            return;
        };
        d.element = Some(e);
        ui.set_attr(e, name::ID, auberean_ui::Value::Id(d.quest.id));
        ui.list_multi_insert(list, e, index);
        let time = ui.child_recursive(e, name::QUEST_TIME);
        Self::quest_time(ui, d, time);
        if d.expanded {
            ui.set_state(e, name::HIGHLIGHT);
            Self::add_open_rows(ui, d, list, index + 1, index + 2);
        } else {
            ui.set_state(e, name::NORMAL);
        }
        Self::name_row(ui, d, e, time);
    }

    /// The open quest's text row and cancel row, where missing.
    fn add_open_rows(
        ui: &mut Ui,
        d: &mut Descriptor,
        list: Handle,
        text_at: usize,
        cancel_at: usize,
    ) {
        let table = Self::table(ui);
        if let Some(info) = d.quest.extra_text(table) {
            if d.extra.is_none() {
                d.extra = ui.frame_create_element(TOOLBAR_LAYOUT_ENUM, name::EXTRA_TEXT);
                if let Some(x) = d.extra {
                    ui.list_multi_insert(list, x, text_at);
                }
            }
            if let Some(x) = d.extra {
                ui.set_string_info(x, &info);
            }
        }
        if d.cancel.is_none()
            && d.quest.status & status::UNDER_WAY != 0
            && d.quest.status & status::FAILED == 0
            && d.quest.cancellable
        {
            d.cancel = ui.frame_create_element(TOOLBAR_LAYOUT_ENUM, name::CANCEL_ELEMENT);
            if let Some(c) = d.cancel {
                if let Some(b) = ui.child_recursive(c, name::CANCEL_BUTTON) {
                    ui.set_attr(b, name::ID, auberean_ui::Value::Id(d.quest.id));
                }
                ui.list_multi_insert(list, c, cancel_at);
            }
        }
    }

    /// A quest row's name, and its name and time marked when the quest changed.
    fn name_row(ui: &mut Ui, d: &Descriptor, e: Handle, time: Option<Handle>) {
        if let Some(n) = ui.child_recursive(e, name::QUEST_NAME) {
            if d.updated {
                if let Some(t) = time {
                    ui.set_state(t, name::CHANGED);
                }
                ui.set_state(n, name::CHANGED);
            }
            ui.set_string_info(n, &d.quest.name);
        }
    }

    /// A quest's rows brought up to date: closed, its open rows gone; open, its text and cancel
    /// rows after its own.
    // D2-02707
    fn update_element(ui: &mut Ui, d: &mut Descriptor, list: Handle) {
        let Some(e) = d.element else { return };
        if d.expanded {
            ui.set_state(e, name::HIGHLIGHT);
            let at = ui.list_index_of(list, e).map_or(0, |i| i + 1);
            let cancel_at = at + usize::from(d.extra.is_some() || d.quest.extra_text(0).is_some());
            Self::add_open_rows(ui, d, list, at, cancel_at);
        } else {
            ui.set_state(e, name::NORMAL);
            for r in [d.extra.take(), d.cancel.take()].into_iter().flatten() {
                if let Some(i) = ui.list_index_of(list, r) {
                    ui.list_multi_remove(list, i);
                }
                ui.remove(r);
            }
        }
        let time = ui.child_recursive(e, name::QUEST_TIME);
        if let (Some(n), true) = (ui.child_recursive(e, name::QUEST_NAME), d.updated) {
            if let Some(t) = time {
                ui.set_state(t, name::CHANGED);
            }
            ui.set_state(n, name::CHANGED);
        }
    }

    /// A quest's rows taken out of its list and gone, and its time left with them.
    // D2-02702
    fn remove_element(ui: &mut Ui, d: &mut Descriptor, list: Handle) {
        let Some(e) = d.element.take() else { return };
        if let Some(t) = d.time.take() {
            t.close(ui);
        }
        for r in [Some(e), d.extra.take(), d.cancel.take()]
            .into_iter()
            .flatten()
        {
            if let Some(i) = ui.list_index_of(list, r) {
                ui.list_multi_remove(list, i);
            }
            ui.remove(r);
        }
    }

    /// Both lists laid out: each quest's rows made or brought up to date (the open quest open).
    // D2-02711
    fn update(&mut self, ui: &mut Ui) {
        let (Some(u), Some(f)) = (self.underway_list, self.finished_list) else {
            return;
        };
        for (list, quests) in [(u, &mut self.underway), (f, &mut self.finished)] {
            let mut open = None;
            for (i, d) in quests.iter_mut().enumerate() {
                if d.quest.id == self.selected {
                    d.expanded = true;
                    open = Some(i);
                }
                if d.element.is_some() {
                    Self::update_element(ui, d, list);
                } else {
                    Self::add_element(ui, d, list, i);
                }
            }
            ui.list_changes_done(list);
            if let Some(i) = open {
                ui.list_scroll_to_row(list, i32::try_from(i).unwrap_or(i32::MAX), true);
            }
        }
    }

    /// The scripts' notice about the journal's quests: the quest that changed becomes the open
    /// one; under way, it goes to (or stays in) the under way page; ended, to the finished page;
    /// the whole list read; a quest added; one removed. When something changed while the window
    /// is hidden, its button shows it.
    // D2-02727
    pub fn update_quests(
        &mut self,
        ui: &mut Ui,
        how: u32,
        quest: Option<Quest>,
        all: Option<&[Quest]>,
        window_shown: bool,
        pages: &mut Vec<Token>,
    ) {
        let quest = quest.unwrap_or_default();
        let id = quest.id;
        let previous = self.selected;
        if let Some(d) = self.find(previous) {
            d.expanded = false;
        }
        self.selected = id;
        self.dirty = false;
        let mut flash = false;
        let mut fresh = Descriptor::new(quest.clone());
        match how {
            change::ONE => {
                match quest.status {
                    status::UNDER_WAY => {
                        if let Some(d) = self.underway.iter_mut().find(|d| d.quest.id == id) {
                            d.updated = true;
                            d.quest = quest;
                        } else {
                            if let Some(i) = self.finished.iter().position(|d| d.quest.id == id) {
                                let mut d = self.finished.remove(i);
                                if let Some(l) = self.finished_list {
                                    Self::remove_element(ui, &mut d, l);
                                    ui.list_changes_done(l);
                                }
                            }
                            fresh.updated = true;
                            insert_sorted(&mut self.underway, fresh, true);
                            pages.push(name::UNDERWAY_TAB);
                        }
                        pages.push(name::UNDERWAY_TAB);
                    }
                    status::COMPLETED | status::FAILED | status::CANCELLED => {
                        if let Some(i) = self.underway.iter().position(|d| d.quest.id == id) {
                            let mut d = self.underway.remove(i);
                            if let Some(l) = self.underway_list {
                                Self::remove_element(ui, &mut d, l);
                                ui.list_changes_done(l);
                            }
                        }
                        fresh.updated = true;
                        insert_sorted(&mut self.finished, fresh, true);
                        pages.push(name::FINISHED_TAB);
                    }
                    _ => {}
                }
                flash = true;
                self.dirty = true;
            }
            change::ALL => {
                for q in all.unwrap_or_default() {
                    let d = Descriptor::new(q.clone());
                    match q.status {
                        status::UNDER_WAY => insert_sorted(&mut self.underway, d, true),
                        status::COMPLETED | status::FAILED | status::CANCELLED
                            if !self.finished.iter().any(|x| x.quest.id == q.id) =>
                        {
                            insert_sorted(&mut self.finished, d, false);
                        }
                        _ => {}
                    }
                }
                pages.push(name::UNDERWAY_TAB);
                self.dirty = true;
            }
            change::ADDED => {
                fresh.updated = true;
                insert_sorted(&mut self.underway, fresh, true);
                flash = true;
                pages.push(name::UNDERWAY_TAB);
                self.dirty = true;
            }
            change::REMOVED => {
                for (quests, list) in [
                    (&mut self.underway, self.underway_list),
                    (&mut self.finished, self.finished_list),
                ] {
                    if let Some(i) = quests.iter().position(|d| d.quest.id == id) {
                        let mut d = quests.remove(i);
                        if let Some(l) = list {
                            Self::remove_element(ui, &mut d, l);
                            ui.list_changes_done(l);
                        }
                        break;
                    }
                }
                if self.selected == id {
                    self.selected = 0;
                }
            }
            _ => {}
        }
        if self.dirty && !window_shown && flash {
            if let Some(b) = ui.element(name::BUTTON) {
                ui.set_state(b, name::CHANGED);
            }
        }
    }

    /// Asked to give a quest up: a confirmation first.
    fn ask_cancel(&mut self, ui: &Ui, quest: u32, out: &mut Vec<Request>) {
        self.cancelling = quest;
        self.question = crate::dialog::new_context();
        let text = ui
            .assets()
            .write(&StringInfo::entry(
                Self::table(ui),
                Token::of("ID_Quest_CancelConfirmationText").raw(),
            ))
            .unwrap_or_default();
        out.push(Request::Dialog(Dialog {
            id: self.question,
            kind: dialog::CONFIRMATION,
            text,
            modal: false,
            info: None,
        }));
    }

    /// A question closed: the cancel question answered yes gives the quest up (when it may be).
    // D2-02717, D2-02713
    pub fn close_confirmation(&mut self, id: u32, yes: bool, out: &mut Vec<Request>) {
        if id != self.question || !yes || self.cancelling == 0 {
            return;
        }
        self.question = 0;
        let q = self.cancelling;
        if self.find(q).is_some_and(|d| d.quest.cancellable) {
            out.push(Request::CancelQuest { quest: q });
        }
    }
}

impl Journal {
    /// The notices it registers for as it is made, by their own numbers.
    pub const NOTICES: &'static [u32] = &[0x436, 0x418, 0x1_040C];

    /// Its notices as the panels name them.
    #[must_use]
    pub fn notices_of() -> Vec<u32> {
        crate::notices::keys(Self::NOTICES)
    }
}

impl Child for Journal {
    fn notices(&self) -> Vec<u32> {
        Self::notices_of()
    }

    fn frame(&self) -> FrameId {
        self.frame
    }

    /// Hidden (with the window): the open quest closes.
    // D2-02718
    fn show(&mut self, ui: &mut Ui, on: bool, _out: &mut Vec<Request>) {
        let list_shown = self.underway_list.is_some_and(|l| ui.shown(l));
        ui.frame_show(self.frame, on);
        if list_shown && !ui.frame_shown(self.frame) && self.selected != 0 {
            let s = self.selected;
            if let Some(d) = self.find(s) {
                d.expanded = false;
                d.updated = false;
            }
            self.selected = 0;
            self.dirty = true;
        }
    }

    /// A quest's row pressed (opened or closed); its text laid out; its cancel button; the clock
    /// (the lists laid out when they changed).
    // D2-02715, D2-02716
    fn listen(&mut self, ui: &mut Ui, mail: &Mail, out: &mut Vec<Request>) {
        match *mail {
            Mail::Global {
                what: global::TIME, ..
            } if self.dirty => {
                self.dirty = false;
                self.update(ui);
            }
            Mail::Element {
                element,
                from,
                what,
                ..
            } => {
                let id = ui.node(from).and_then(|n| n.id_attr(name::ID)).unwrap_or(0);
                if element == name::TEMPLATE && what == msg::BUTTON_RELEASE_ON {
                    self.selected = 0;
                    if let Some(d) = self.find(id) {
                        d.updated = false;
                        d.expanded = !d.expanded;
                        self.dirty = true;
                    }
                }
                if element == name::EXTRA_TEXT && what == name::DOWNLOAD_DONE {
                    ui.recalculate_glyph_list(from);
                    let w = ui.node(from).map_or(0, |n| n.rect.width());
                    let h = ui.text_state(from).map_or(0, |t| t.paper_h);
                    ui.resize_to(from, w, h);
                    for list in [self.underway_list, self.finished_list]
                        .into_iter()
                        .flatten()
                    {
                        ui.list_update_and_draw(list);
                    }
                }
                if element == name::CANCEL_BUTTON && what == msg::BUTTON_RELEASE_ON {
                    self.ask_cancel(ui, id, out);
                }
            }
            _ => {}
        }
    }

    /// The clock: each quest's time left written again when it is time to look.
    fn update(&mut self, ui: &mut Ui, _view: &gameplay::View<'_>, _out: &mut Vec<Request>) {
        for d in self.underway.iter_mut().chain(self.finished.iter_mut()) {
            if let Some(t) = &mut d.time {
                t.tick(ui);
            }
        }
    }

    /// The journal goes: its quests and their rows, its listening.
    // D2-02726
    fn close(mut self: Box<Self>, ui: &mut Ui) {
        for (quests, list) in [
            (&mut self.underway, self.underway_list),
            (&mut self.finished, self.finished_list),
        ] {
            for mut d in quests.drain(..) {
                if let Some(l) = list {
                    Self::remove_element(ui, &mut d, l);
                }
            }
        }
        ui.drop_frame(self.frame);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// The quest window's panel: the window, and the journal under it.
#[derive(Debug)]
pub struct QuestWindow {
    frame: FrameId,
    field: Option<Handle>,
    pub journal: Option<Journal>,
    pub story: Option<crate::story_quests::StoryQuests>,
}

impl QuestWindow {
    /// The window under the toolbar's framework: its field found, the journal made, its buttons
    /// and the windows' fields heard; hidden.
    // D2-02700
    pub fn new(ui: &mut Ui, parent: FrameId) -> Self {
        let frame = ui.new_child_frame(parent);
        let field = ui.element(name::FIELD);
        let journal = field.map(|_| Journal::new(ui, frame));
        let story = field.map(|_| crate::story_quests::StoryQuests::new(ui, frame));
        if journal.is_some() {
            if let Some(l) = ui.frame_listener(frame) {
                ui.listen_for_message(l, name::CLOSE_BUTTON, msg::BUTTON_RELEASE_ON);
                ui.listen_for_message(l, name::HELP_BUTTON, msg::BUTTON_RELEASE_ON);
                ui.listen_for_message(l, name::PANEL_TOP, msg::MOUSE_PRESS);
                ui.listen_for_message(l, name::PANEL_TOP, msg::MOUSE_RELEASE);
                ui.listen_for_message(l, name::BUTTON, msg::MOUSE_RELEASE);
                for f in WINDOW_FIELDS {
                    ui.listen_for_message(l, Token::of(f), BRING_TO_FRONT);
                }
            }
        }
        let mut me = Self {
            frame,
            field,
            journal,
            story,
        };
        me.show(ui, false, &mut Vec::new());
        me
    }

    /// Open one of the window's pages (its tab).
    // D2-02699
    pub fn set_active_panel(ui: &mut Ui, tab: Token) {
        if let Some(t) = ui.element(tab) {
            ui.panel_page_start_use_active(t);
        }
    }
}

impl QuestWindow {
    /// The notices its panels register for as they are made, in the order they are made.
    #[must_use]
    pub fn notices_of() -> Vec<u32> {
        crate::notices::union([
            Journal::notices_of(),
            crate::story_quests::StoryQuests::notices_of(),
        ])
    }
}

impl Child for QuestWindow {
    fn notices(&self) -> Vec<u32> {
        Self::notices_of()
    }

    fn frame(&self) -> FrameId {
        self.frame
    }

    /// Shown or hidden, with its journal: shown, it comes to the front and its button no longer
    /// shows a change (it takes its unchanged look).
    // D2-02697
    fn show(&mut self, ui: &mut Ui, on: bool, out: &mut Vec<Request>) {
        ui.frame_show(self.frame, on);
        if let Some(j) = &mut self.journal {
            gameplay::hear(j, ui, out);
        }
        if let Some(s) = &mut self.story {
            gameplay::hear(s, ui, out);
        }
        let Some(f) = self.field else { return };
        ui.broadcast(f, BRING_TO_FRONT, 0);
        let shown = ui.frame_shown(self.frame);
        ui.set_visible(f, shown);
        if shown {
            if let Some(b) = ui.element(name::BUTTON) {
                ui.set_state(b, name::UNCHANGED);
            }
        }
    }

    /// A window come to the front; the title bar pressed (to the front) and let go (the places
    /// kept); its button, close and help buttons.
    // D2-02698
    fn listen(&mut self, ui: &mut Ui, mail: &Mail, out: &mut Vec<Request>) {
        let Mail::Element {
            element,
            what,
            from,
            ..
        } = *mail
        else {
            return;
        };
        if what == BRING_TO_FRONT {
            window_to_front(ui, self.field, Some(from));
        }
        if element == name::PANEL_TOP {
            if what == msg::MOUSE_PRESS {
                if let Some(f) = self.field {
                    ui.broadcast(f, BRING_TO_FRONT, 0);
                }
            }
            if what == msg::MOUSE_RELEASE {
                gameplay::GamePlay::save_locations_changed(out);
            }
        }
        if element == name::BUTTON && what == msg::MOUSE_RELEASE {
            let shown = ui.frame_shown(self.frame);
            self.show(ui, !shown, out);
        } else if element == name::CLOSE_BUTTON && what == msg::BUTTON_RELEASE_ON {
            self.show(ui, false, out);
        } else if element == name::HELP_BUTTON && what == msg::BUTTON_RELEASE_ON {
            out.push(Request::Help { open: true });
        }
    }

    /// The scripts' quest notices reach the journal (the pages they name opened); a question
    /// closed may give a quest up.
    fn notice(&mut self, ui: &mut Ui, n: &Notice, out: &mut Vec<Request>) {
        if let Some(s) = &mut self.story {
            gameplay::hand(s, ui, n, out);
        }
        if let Some((id, yes)) = crate::dialog::confirmation_closed(n) {
            if let Some(j) = &mut self.journal {
                j.close_confirmation(id, yes, out);
            }
            return;
        }
        if n.native != notice::UPDATE_QUESTS {
            return;
        }
        let shown = ui.frame_shown(self.frame);
        let Some(j) = &mut self.journal else { return };
        let quest = n.records.iter().flatten().find_map(|r| match r {
            Record::Quest(q) => Some(q.clone()),
            _ => None,
        });
        let all: Vec<Quest> = n
            .records
            .iter()
            .flatten()
            .filter_map(|r| match r {
                Record::Quests(l) => Some(l.clone()),
                _ => None,
            })
            .flatten()
            .collect();
        let mut pages = Vec::new();
        j.update_quests(ui, n.word(), quest, Some(&all), shown, &mut pages);
        for p in pages {
            Self::set_active_panel(ui, p);
        }
    }

    /// Its own panels' frame: the journal.
    fn update(&mut self, ui: &mut Ui, view: &gameplay::View<'_>, out: &mut Vec<Request>) {
        if let Some(j) = &mut self.journal {
            gameplay::drive(j, ui, view, &[], out);
        }
        if let Some(s) = &mut self.story {
            gameplay::drive(s, ui, view, &[], out);
        }
    }

    // D2-02701
    fn close(self: Box<Self>, ui: &mut Ui) {
        if let Some(j) = self.journal {
            Box::new(j).close(ui);
        }
        if let Some(s) = self.story {
            Box::new(s).close(ui);
        }
        ui.drop_frame(self.frame);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
