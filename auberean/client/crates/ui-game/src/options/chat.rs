//! The options window's chat page: the chat windows' options, a section for each kind of text the
//! chat panel lists (all but the administrators'), in the panel's order.
//!
//! - **A kind's section**: its font size (smaller, small, medium, large; the chat entry's has no
//!   large), its font color (ten), whether each of the four chat windows shows it, and whether a
//!   line of it shows a hidden window (the chat entry's section has only the size and color).
//! - **The devoted windows' section**: the channel each window is devoted to. The channels are
//!   named from the kinds' list by place, which stops lining up with the channels after the
//!   faction channel: the general channel is offered under the player-killer channel's name, the
//!   trade channel under the general channel's, the region channel under the trade channel's, and
//!   the last choice (no channel) under the chat entry's.
//!
//! Each section opens and closes by its header; the defaults are a new player's chat options. The
//! page shows the player's chat options as the client last read them (again whenever it reads
//! them again). **Apply**, **Recall** and **Default** are each asked first: Apply sets every
//! option (each window's filter last), tells the chat windows their options changed and sends
//! them to the server to keep; Recall puts back what was applied; Default puts the defaults in and
//! applies them. Apply and Recall are ghosted while nothing changed.

use auberean_ui::framework::FrameId;
use auberean_ui::token::{attr, msg};
use auberean_ui::{Handle, Listener, Mail, Token, Ui, Value};

use super::helper::{self, Row, Rows, OPTION_STRINGS};
use super::page;
use crate::chat::{text_type, Ask, ChatSettings, DEVOTED_TYPES, TEXT_TYPES};
use crate::gameplay::{self, dialog, Dialog, Notice};

mod name {
    use auberean_ui::Token;
    pub const PAGE: Token = Token::of("ChatPage");
    pub const TAB: Token = Token::of("ChatTab");
    pub const LIST: Token = Token::of("ChatOptionsList");
}

/// The labels of a section's options, by their strings' names.
pub mod label {
    use auberean_ui::Token;
    const fn id(s: &str) -> u32 {
        Token::of(s).raw()
    }
    pub const FONT_SIZE: u32 = id("ID_Option_ChatFontSize");
    pub const FONT_COLOR: u32 = id("ID_Option_ChatFontColor");
    pub const POPUP: u32 = id("ID_Option_ChatPopup");
    /// Chat windows 1 to 4 (a window's filter's option, or its devoted channel's).
    pub const WINDOWS: [u32; 4] = [
        id("ID_Option_Chat1"),
        id("ID_Option_Chat2"),
        id("ID_Option_Chat3"),
        id("ID_Option_Chat4"),
    ];
}

/// The font sizes offered, in order: each choice's label and the font's number.
pub const FONT_SIZES: [(Token, u32); 4] = [
    (Token::of("ID_Option_ChatSmallerFont"), 3),
    (Token::of("ID_Option_ChatSmallFont"), 0),
    (Token::of("ID_Option_ChatMediumFont"), 1),
    (Token::of("ID_Option_ChatLargeFont"), 2),
];

/// The font colors offered, in order (a choice's number is the color's).
pub const FONT_COLORS: [Token; 10] = [
    Token::of("ID_Option_ChatColor1"),
    Token::of("ID_Option_ChatColor2"),
    Token::of("ID_Option_ChatColor3"),
    Token::of("ID_Option_ChatColor4"),
    Token::of("ID_Option_ChatColor5"),
    Token::of("ID_Option_ChatColor6"),
    Token::of("ID_Option_ChatColor7"),
    Token::of("ID_Option_ChatColor8"),
    Token::of("ID_Option_ChatColor9"),
    Token::of("ID_Option_ChatColor10"),
];

/// The kinds a window's filter keeps through Apply (the page's flags set or clear the rest).
const KEPT_KINDS: u32 = text_type::ADMIN | text_type::LOG;

/// The chat window (1 to 4) an option's label names.
fn window_of(label: u32) -> Option<u32> {
    label::WINDOWS
        .iter()
        .position(|l| *l == label)
        .and_then(|w| u32::try_from(w + 1).ok())
}

/// A chat window's place (windows 1 to 4).
fn index(window: u32) -> usize {
    usize::try_from(window.wrapping_sub(1)).unwrap_or(usize::MAX)
}

/// The chat page.
#[derive(Debug)]
pub struct ChatOptionsPage {
    frame: FrameId,
    /// The window's framework (the page acts on its buttons only while the window shows).
    window: FrameId,
    page: Option<Handle>,
    list: Option<Handle>,
    apply_button: Option<Handle>,
    recall_button: Option<Handle>,
    rows: Rows,
    /// The sections, in the page's order: the kind of text and the section's row.
    sections: Vec<(u32, usize)>,
    /// The questions up: apply, recall, default.
    context_apply: u32,
    context_recall: u32,
    context_default: u32,
    /// The player's chat options last shown.
    shown: Option<ChatSettings>,
}

impl ChatOptionsPage {
    /// The page under the window's framework: its element, list and buttons found (without its
    /// Apply and Recall buttons it is left empty); its tab heard; hidden. Its sections are made
    /// when the client first gives a new player's chat options (see [`Self::update`]).
    // D2-02885
    pub fn new(ui: &mut Ui, window: FrameId) -> Self {
        let frame = ui.new_child_frame(window);
        let page = ui.element(name::PAGE);
        let find = |ui: &Ui, n| page.and_then(|p| ui.child_recursive(p, n));
        let list = find(ui, name::LIST);
        let apply_button = find(ui, super::name::APPLY_BUTTON);
        let recall_button = apply_button.and(find(ui, super::name::RECALL_BUTTON));
        if recall_button.is_some() {
            if let Some(l) = ui.frame_listener(frame) {
                ui.listen_for_message(l, name::TAB, page::OPENED);
                ui.listen_for_message(l, name::TAB, page::CLOSED);
            }
            ui.frame_show(frame, false);
        }
        Self {
            frame,
            window,
            page,
            list,
            apply_button,
            recall_button,
            rows: Rows::default(),
            sections: Vec::new(),
            context_apply: 0,
            context_recall: 0,
            context_default: 0,
            shown: None,
        }
    }

    /// The page's framework.
    #[must_use]
    pub const fn frame(&self) -> FrameId {
        self.frame
    }

    /// The page's rows.
    #[must_use]
    pub const fn rows(&self) -> &Rows {
        &self.rows
    }

    /// The kinds of text the page has a section for, in the page's order.
    #[must_use]
    pub fn kinds(&self) -> Vec<u32> {
        self.sections.iter().map(|s| s.0).collect()
    }

    /// The section of a kind of text, as its row.
    #[must_use]
    pub fn section(&self, kind: u32) -> Option<&Row> {
        self.section_at(kind).and_then(|s| self.rows.all.get(s))
    }

    fn section_at(&self, kind: u32) -> Option<usize> {
        self.sections.iter().find(|(k, _)| *k == kind).map(|s| s.1)
    }

    /// A kind's section's option, by its label.
    #[must_use]
    pub fn option(&self, kind: u32, label: u32) -> Option<&Row> {
        let s = self.section_at(kind)?;
        (0..self.rows.count(s))
            .filter_map(|i| self.rows.get(s, i))
            .filter_map(|at| self.rows.all.get(at))
            .find(|r| r.label == label)
    }

    fn add_option(&mut self, row: Row, section: usize) {
        let at = self.rows.all.len();
        self.rows.all.push(row);
        self.rows.add_to_section(section, at);
    }

    /// A choice made with its label and no help.
    fn choice(&self, ui: &mut Ui, label: u32) -> Row {
        let mut r = Row::choice(ui, self.frame);
        r.describe_by_label(ui, label);
        r
    }

    /// The font sizes' choice: smaller, small, medium and large (the chat entry's without large).
    // D2-02866
    fn font_size_choice(&self, ui: &mut Ui, kind: u32) -> Row {
        let mut r = self.choice(ui, label::FONT_SIZE);
        let count = if kind == text_type::CHAT_ENTRY { 3 } else { 4 };
        for (v, (l, _)) in (0..).zip(&FONT_SIZES[..count]) {
            r.assign_choice(ui, v, OPTION_STRINGS, l.raw());
        }
        r
    }

    /// The font colors' choice, ten of them.
    // D2-02867
    fn font_color_choice(&self, ui: &mut Ui) -> Row {
        let mut r = self.choice(ui, label::FONT_COLOR);
        for (v, l) in (0..).zip(&FONT_COLORS) {
            r.assign_choice(ui, v, OPTION_STRINGS, l.raw());
        }
        r
    }

    /// A kind's section's options with a new player's values as their defaults: the devoted
    /// windows' channels (a choice for each window, of the channels offered, each named by the
    /// kinds' list at its place); or the font size, the font color and, but for the chat entry,
    /// the four windows' filters and the popup flag.
    // D2-02868
    fn populate(&mut self, ui: &mut Ui, section: usize, kind: u32, d: &ChatSettings) {
        if kind == text_type::DEVOTED {
            for (n, l) in (1..).zip(label::WINDOWS) {
                let mut r = self.choice(ui, l);
                let devoted = d.devoted_channel(n);
                for (v, (offered, named)) in (0..).zip(DEVOTED_TYPES.iter().zip(&TEXT_TYPES)) {
                    if offered.hidden {
                        continue;
                    }
                    r.assign_choice(ui, v, OPTION_STRINGS, named.name.raw());
                    if offered.kind == devoted {
                        r.set_choice_default(ui, v);
                    }
                }
                self.add_option(r, section);
            }
            return;
        }
        let (size, color, popup) = d.font(kind);
        let mut r = self.font_size_choice(ui, kind);
        if let Some(v) = (0..).zip(&FONT_SIZES).find(|(_, s)| s.1 == size) {
            r.set_choice_default(ui, v.0);
        }
        self.add_option(r, section);
        let mut r = self.font_color_choice(ui);
        r.set_choice_default(ui, i32::try_from(color).unwrap_or(0));
        self.add_option(r, section);
        if kind == text_type::CHAT_ENTRY {
            return;
        }
        for (n, l) in (1..).zip(label::WINDOWS) {
            let mut r = Row::flag(ui, self.frame);
            r.describe_by_label(ui, l);
            r.set_flag_default(ui, kind & d.filter(n) != 0);
            self.add_option(r, section);
        }
        let mut r = Row::flag(ui, self.frame);
        r.describe_by_label(ui, label::POPUP);
        r.set_flag_default(ui, popup);
        self.add_option(r, section);
    }

    /// A section for each kind of text the panel lists, closed, its options in it, in the page's
    /// list.
    // D2-02878
    fn setup(&mut self, ui: &mut Ui, defaults: &ChatSettings) {
        for t in TEXT_TYPES.iter().filter(|t| !t.hidden) {
            let mut s = Row::section(ui, self.frame);
            s.describe_by_label(ui, t.name.raw());
            let at = self.rows.all.len();
            self.rows.all.push(s);
            self.rows.dismantle(ui, at);
            self.populate(ui, at, t.kind, defaults);
            self.sections.push((t.kind, at));
            if let (Some(list), Some(e)) = (self.list, self.rows.all[at].element) {
                ui.list_add(list, e);
            }
        }
    }

    /// Every option shown as the player's chat options say, and applied (nothing changed): a
    /// window's devoted channel, a kind's font size (applied as it was when the size is not
    /// offered), font color and popup flag, whether a window shows the kind.
    // D2-02883
    pub fn refresh(&mut self, ui: &mut Ui, s: &ChatSettings) {
        for &(kind, section) in &self.sections {
            let (size, color, popup) = s.font(kind);
            for i in 0..self.rows.count(section) {
                let Some(r) = self
                    .rows
                    .get(section, i)
                    .and_then(|at| self.rows.all.get_mut(at))
                else {
                    continue;
                };
                let window = window_of(r.label);
                if kind == text_type::DEVOTED {
                    let devoted = window.map_or(text_type::SAY, |n| s.devoted_channel(n));
                    for (v, d) in (0..).zip(&DEVOTED_TYPES) {
                        if d.kind == devoted {
                            r.set_choice(ui, v);
                            r.apply(ui);
                        }
                    }
                } else if r.label == label::FONT_SIZE {
                    if let Some((v, _)) = (0..).zip(&FONT_SIZES).find(|(_, f)| f.1 == size) {
                        r.set_choice(ui, v);
                    }
                    r.apply(ui);
                } else if r.label == label::FONT_COLOR {
                    r.set_choice(ui, i32::try_from(color).unwrap_or(0));
                    r.apply(ui);
                } else if r.label == label::POPUP {
                    r.set_flag(ui, popup);
                    r.apply(ui);
                } else if let Some(n) = window {
                    r.set_flag(ui, kind & s.filter(n) != 0);
                    r.apply(ui);
                }
            }
        }
        self.shown = Some(s.clone());
    }

    /// Whether any option changed.
    // D2-02873
    #[must_use]
    pub fn changed(&self, ui: &Ui) -> bool {
        self.sections.iter().any(|(_, s)| self.rows.changed(ui, *s))
    }

    /// Apply and Recall ghosted unless an option changed.
    // D2-02874
    pub fn update_buttons(&self, ui: &mut Ui) {
        if self.page.is_none() {
            return;
        }
        let ghosted = !self.changed(ui);
        for b in [self.apply_button, self.recall_button]
            .into_iter()
            .flatten()
        {
            ui.set_attr(b, attr::GHOSTED, Value::Bool(ghosted));
        }
    }

    /// Every option set, section by section (each section then applied): a window's devoted
    /// channel, a kind's font size, font color and popup flag; then each window's filter, its
    /// kinds as the page's flags say (the administrators' and the log's as they were). The chat
    /// windows told their options changed; the options read again and sent to the server to keep.
    /// Nothing without the player's chat options.
    // D2-02880
    pub fn apply(&mut self, ui: &mut Ui, out: &mut Vec<gameplay::Request>) {
        let Some(s) = &self.shown else { return };
        let mut filters = [1, 2, 3, 4].map(|n| s.filter(n) & KEPT_KINDS);
        let mut asks = Vec::new();
        for &(kind, section) in &self.sections {
            for i in 0..self.rows.count(section) {
                let Some(r) = self
                    .rows
                    .get(section, i)
                    .and_then(|at| self.rows.all.get(at))
                else {
                    continue;
                };
                let window = window_of(r.label);
                if kind == text_type::DEVOTED {
                    let chosen = usize::try_from(r.choice_now(ui))
                        .ok()
                        .and_then(|c| DEVOTED_TYPES.get(c));
                    if let (Some(window), Some(d)) = (window, chosen) {
                        asks.push(Ask::Devote {
                            window,
                            kind: d.kind,
                        });
                    }
                } else if r.label == label::FONT_SIZE {
                    let chosen = usize::try_from(r.choice_now(ui))
                        .ok()
                        .and_then(|c| FONT_SIZES.get(c));
                    if let Some((_, size)) = chosen {
                        asks.push(Ask::FontSize { kind, size: *size });
                    }
                } else if r.label == label::FONT_COLOR {
                    let color = u32::try_from(r.choice_now(ui)).unwrap_or(0);
                    asks.push(Ask::FontColor { kind, color });
                } else if r.label == label::POPUP {
                    let on = r.flag_now(ui);
                    asks.push(Ask::Popup { kind, on });
                } else if let Some(f) = window.and_then(|n| filters.get_mut(index(n))) {
                    if r.flag_now(ui) {
                        *f |= kind;
                    } else {
                        *f &= !kind;
                    }
                }
            }
            self.rows.apply(ui, section);
        }
        for (window, filter) in (1..).zip(filters) {
            asks.push(Ask::Filter { window, filter });
        }
        asks.push(Ask::ReloadChatSettings);
        asks.push(Ask::KeepChatOptions);
        out.extend(asks.into_iter().map(gameplay::Request::Chat));
        out.push(gameplay::Request::Notice(Notice {
            native: crate::chat::notice::RELOAD_CHAT_SETTINGS,
            ..Notice::default()
        }));
        self.update_buttons(ui);
    }

    /// Every option back to what was applied.
    // D2-02876
    pub fn recall(&mut self, ui: &mut Ui) {
        for (_, s) in self.sections.clone() {
            self.rows.recall(ui, s);
        }
        self.update_buttons(ui);
    }

    /// Every option to its default, then applied.
    // D2-02881
    pub fn default(&mut self, ui: &mut Ui, out: &mut Vec<gameplay::Request>) {
        for (_, s) in self.sections.clone() {
            self.rows.to_default(ui, s);
        }
        self.apply(ui, out);
        self.update_buttons(ui);
    }

    fn question(text: &str, ui: &Ui, out: &mut Vec<gameplay::Request>) -> u32 {
        let id = super::new_dialog_id();
        out.push(gameplay::Request::Dialog(Dialog {
            id,
            kind: dialog::CONFIRMATION,
            text: crate::flow::message(ui, OPTION_STRINGS, text),
            modal: false,
            info: None,
        }));
        id
    }

    /// Ask before applying (unless asked already).
    // D2-02863
    pub fn confirm_apply(&mut self, ui: &Ui, out: &mut Vec<gameplay::Request>) {
        if self.context_apply == 0 {
            self.context_apply = Self::question("ID_Option_ApplyText", ui, out);
        }
    }

    /// Ask before recalling (unless asked already).
    // D2-02864
    pub fn confirm_recall(&mut self, ui: &Ui, out: &mut Vec<gameplay::Request>) {
        if self.context_recall == 0 {
            self.context_recall = Self::question("ID_Option_RecallText", ui, out);
        }
    }

    /// Ask before the defaults (unless asked already).
    // D2-02865
    pub fn confirm_default(&mut self, ui: &Ui, out: &mut Vec<gameplay::Request>) {
        if self.context_default == 0 {
            self.context_default = Self::question("ID_Option_DefaultText", ui, out);
        }
    }

    fn clear_confirmations(&mut self, out: &mut Vec<gameplay::Request>) {
        for c in [
            &mut self.context_apply,
            &mut self.context_recall,
            &mut self.context_default,
        ] {
            if *c != 0 {
                out.push(gameplay::Request::DialogDown(*c));
                *c = 0;
            }
        }
    }

    /// A question answered: yes applies, recalls or puts the defaults in; the questions
    /// forgotten, the others taken down.
    // D2-02882
    pub fn close_confirmation_dialog(
        &mut self,
        ui: &mut Ui,
        id: u32,
        yes: bool,
        out: &mut Vec<gameplay::Request>,
    ) {
        if yes {
            if id == self.context_apply {
                self.apply(ui, out);
            }
            if id == self.context_recall {
                self.recall(ui);
            }
            if id == self.context_default {
                self.default(ui, out);
            }
        }
        for c in [
            &mut self.context_apply,
            &mut self.context_recall,
            &mut self.context_default,
        ] {
            if id == *c {
                *c = 0;
            }
        }
        self.clear_confirmations(out);
    }

    /// What the page heard: its tab opened (its buttons heard, their ghosting, the page laid out)
    /// or closed (its buttons no longer heard); while the window shows, its buttons (each asked
    /// first) and anything its options said (a section opened or closed lays the list out
    /// again), then the buttons' ghosting.
    // D2-02877
    pub fn listen(
        &mut self,
        ui: &mut Ui,
        element: Token,
        what: Token,
        out: &mut Vec<gameplay::Request>,
    ) {
        let buttons = [
            super::name::APPLY_BUTTON,
            super::name::RECALL_BUTTON,
            super::name::DEFAULT_BUTTON,
        ];
        if element == name::TAB && what == page::OPENED {
            if let Some(l) = ui.frame_listener(self.frame) {
                for b in buttons {
                    ui.listen_for_message(l, b, msg::BUTTON_RELEASE_ON);
                }
            }
            self.update_buttons(ui);
            if let Some(p) = self.page {
                ui.update(p);
            }
        } else if element == name::TAB && what == page::CLOSED {
            if let Some(l) = ui.frame_listener(self.frame) {
                for b in buttons {
                    ui.unregister_for_message(Listener::Other(l), b, msg::BUTTON_RELEASE_ON);
                }
            }
        } else if ui.frame_shown(self.window) {
            if what == msg::BUTTON_RELEASE_ON {
                if element == super::name::APPLY_BUTTON {
                    self.confirm_apply(ui, out);
                }
                if element == super::name::RECALL_BUTTON {
                    self.confirm_recall(ui, out);
                }
                if element == super::name::DEFAULT_BUTTON {
                    self.confirm_default(ui, out);
                }
            }
            if what == helper::name::EXPANDER_TOGGLE {
                if let Some(list) = self.list {
                    ui.list_update_and_draw(list);
                }
            }
            self.update_buttons(ui);
        }
    }

    /// The page's own mail and its rows': a row's message handed on (a section's header opens or
    /// closes it first).
    pub fn hear(&mut self, ui: &mut Ui, out: &mut Vec<gameplay::Request>) {
        for m in crate::camera::take(ui, self.frame) {
            if let Mail::Element { element, what, .. } = m {
                self.listen(ui, element, what, out);
            }
        }
        for at in 0..self.rows.all.len() {
            let frame = self.rows.all[at].frame();
            for m in crate::camera::take(ui, frame) {
                let Some(h) = self.rows.all[at].listen(ui, &m) else {
                    continue;
                };
                if h.what == helper::name::EXPANDER_TOGGLE {
                    self.rows.toggle(ui, at);
                }
                self.listen(ui, h.element, h.what, out);
            }
        }
    }

    /// The player's chat options as the client last read them, and a new player's: the sections
    /// made when the latter are first given (unless the page was left empty); the options shown
    /// again whenever the client has read them again (the chat options or the player's options
    /// changed).
    // D2-02884
    pub fn update(
        &mut self,
        ui: &mut Ui,
        now: Option<&ChatSettings>,
        defaults: Option<&ChatSettings>,
    ) {
        if self.sections.is_empty() && self.recall_button.is_some() {
            if let Some(d) = defaults {
                self.setup(ui, d);
            }
        }
        if self.sections.is_empty() {
            return;
        }
        let Some(s) = now else { return };
        if self.shown.as_ref().is_none_or(|v| v.revision != s.revision) {
            self.refresh(ui, s);
        }
    }

    /// The page goes: its list emptied, its rows, its questions taken down.
    // D2-02872, D2-02875
    pub fn close(mut self, ui: &mut Ui, out: &mut Vec<gameplay::Request>) {
        if let Some(list) = self.list {
            ui.list_flush(list);
        }
        for r in self.rows.all.drain(..) {
            r.close(ui);
        }
        self.clear_confirmations(out);
        ui.drop_frame(self.frame);
    }
}
