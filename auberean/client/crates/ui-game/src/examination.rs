//! The examination window (a toolbar window, bottom left): what the player examines, line by line.
//!
//! [`Examination`] shows the profile the player's scripts give it for what is examined (the
//! selection, a skill, an effect...): each line of the profile that shows on a tab asked for, laid
//! out in one row of the window's list, as the client lays it out: words and numbers flow from
//! left to right and wrap to a new line (keeping the line's indent) when they do not fit, pictures
//! take their own width, indents move on by 16 pixels each, a break starts a new line at the left.
//! Before a profile comes, the window shows "nothing examined" for a request about nothing, else
//! "waiting". A line whose string reference waits on string tables the data lacks shows empty;
//! when they come (or are refused), the window empties to "waiting" again until the next profile.
//!
//! The window starts as the classic window (every tab's lines at once, the tab buttons hidden);
//! the player's options can make it the tabbed one (the first tab, and the tab buttons: general,
//! details, effects, usage). Pressing a tab button (Ctrl adds or takes away a tab) or the keys for
//! the next and previous tab asks the player's scripts to examine again with those tabs. Shown, the
//! window comes to the front of the toolbar's windows and asks to examine with its tabs; hidden, it
//! asks to stop examining. Its own button and key show and hide it, its close button hides it, its
//! title bar moves it (the windows' places kept when let go).

use std::any::Any;

use auberean_text::{StringInfo, Value, Variable};
use auberean_ui::framework::FrameId;
use auberean_ui::manager::global;
use auberean_ui::token::{msg, state};
use auberean_ui::types::text::text_msg::DOWNLOAD_DONE;
use auberean_ui::{Handle, Mail, Token, Ui};

use crate::flow;
use crate::gameplay::{self, notice, Child, Notice, Record, Request};
use crate::shortcuts::TOOLBAR_LAYOUT_ENUM;
use crate::time_display::TimeLeft;

/// The first tab's tabs: what a new request asks for, and what every profile shows.
pub const FIRST_TAB: u32 = 0x203;
/// Every tab: the classic window's.
pub const ALL_TABS: u32 = 0x3FF;
/// The player's option of the classic examination window.
pub const CLASSIC_EXAMINATION: u32 = 0x2_0000;

/// The interface keys for the next and the previous tab.
pub mod action {
    pub const NEXT_TAB: u32 = 0x4100_006A;
    pub const PREVIOUS_TAB: u32 = 0x4100_006B;
}

mod name {
    use auberean_ui::Token;
    pub const FIELD: Token = Token::of("ExaminationField");
    pub const LIST_BOX: Token = Token::of("ExaminationListBox");
    pub const LIST_BOX_TABBED: Token = Token::of("ExaminationListBoxTabbed");
    pub const INTERNAL: Token = Token::of("ExaminationInternal");
    pub const BUTTON: Token = Token::of("ExaminationButton");
    pub const CLOSE_BUTTON: Token = Token::of("ExaminationCloseButton");
    pub const HELP_BUTTON: Token = Token::of("ExaminationHelpButton");
    pub const RESIZE: Token = Token::of("ExamineResize");
    pub const ROW_TEMPLATE: Token = Token::of("ExaminationRowTemplate");
    pub const TEXT_TEMPLATE: Token = Token::of("ExaminationTextTemplate");
    pub const IMAGE_TEMPLATE: Token = Token::of("ExaminationImageTemplate");
    pub const EMPTY_ROW: Token = Token::of("ExaminationEmptyRow");
    pub const PENDING_ROW: Token = Token::of("ExaminationPendingRow");
    pub const GENERAL_TAB: Token = Token::of("GeneralTab");
    pub const DETAILS_TAB: Token = Token::of("DetailsTab");
    pub const EFFECTS_TAB: Token = Token::of("EffectsTab");
    pub const USAGE_TAB: Token = Token::of("UsageTab");
    pub const CLASSIC: Token = Token::of("classic");
    pub const TABBED: Token = Token::of("tabbed");
    pub const ACTIVE: Token = Token::of("active");
    pub const INACTIVE: Token = Token::of("inactive");
    pub const DISABLED: Token = Token::of("disabled");
    pub const VALUE: Token = Token::of("value");
}

/// The message a toolbar window sends when it comes to the front of the others.
pub const BRING_TO_FRONT: Token = Token::of("bring_to_front");

/// The toolbar's windows' fields: each window hears the others come to the front.
pub const WINDOW_FIELDS: [&str; 7] = [
    "ExaminationField",
    "InventoryField",
    "NavigationField",
    "OptionField",
    "QuestField",
    "SkillField",
    "SocialField",
];

/// A toolbar window's field heard a window come to the front: its own comes to depth 10, the
/// others go back one (to between 11 and 18).
pub fn window_to_front(ui: &mut Ui, own: Option<Handle>, from: Option<Handle>) {
    let Some(f) = own else { return };
    let z = if from == Some(f) {
        10
    } else {
        (ui.node(f).map_or(0, |n| n.z) + 1).clamp(11, 18)
    };
    ui.set_z_level(f, z);
}

/// The kinds of a profile's lines.
pub mod kind {
    pub const INT: u32 = 0x4000_0001;
    pub const FLOAT: u32 = 0x4000_0002;
    pub const BOOL: u32 = 0x4000_0003;
    pub const STRING: u32 = 0x4000_0004;
    pub const IMAGE: u32 = 0x4000_0005;
    pub const BREAK: u32 = 0x4000_0006;
    pub const INDENT: u32 = 0x4000_0007;
    pub const COUNTDOWN: u32 = 0x4000_0008;
    pub const ICON: u32 = 0x4000_0009;
    pub const LONG: u32 = 0x4000_000A;
    pub const EFFECT: u32 = 0x4000_000C;
}

/// What is examined, as the window needs it: the request's kind (0: nothing), the object, the
/// tabs it asks for, the enumerated datum (an effect's number, a skill, a skill panel, a recipe
/// category) and the data id (a recipe, a craft skill) of the kinds that carry one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExamRequest {
    pub kind: u32,
    pub object: u64,
    pub tabs: u32,
    pub data_enum: u32,
    pub data_id: u32,
}

impl Default for ExamRequest {
    fn default() -> Self {
        Self {
            kind: 0,
            object: 0,
            tabs: FIRST_TAB,
            data_enum: 0,
            data_id: 0,
        }
    }
}

/// One line of a profile: its kind, the tabs it shows on, the element it is drawn with (0 for the
/// window's own), its value (a word, or 64 bits), the game time a countdown runs out at, a string
/// reference, an icon's layers (each a picture and its color).
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub kind: u32,
    pub tabs: u32,
    pub appearance: u32,
    pub value: u64,
    pub time: f64,
    pub text: Option<StringInfo>,
    pub icon: Vec<(u32, u32)>,
    /// An effect line's own lines, as the player's scripts made them for it.
    pub effect: Vec<Line>,
}

/// A profile: the request it answers, its lines, the tabs its lines were made for.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Profile {
    pub request: ExamRequest,
    pub lines: Vec<Line>,
    pub tabs: u32,
}

/// A tab button: its element's name, the element, the tabs it shows.
#[derive(Clone, Copy, Debug)]
struct Tab {
    name: Token,
    button: Option<Handle>,
    tabs: u32,
}

/// The examination window's panel.
#[derive(Debug)]
pub struct Examination {
    frame: FrameId,
    field: Option<Handle>,
    list: Option<Handle>,
    /// The request examined now.
    pub request: ExamRequest,
    /// The tab shown first.
    current: Token,
    /// Where the next line goes in the row: across, down; the row's foot so far; the indents.
    x: i32,
    y: i32,
    bottom: i32,
    indent: i32,
    /// The tabs asked for.
    pub tabs: u32,
    /// The row being laid out.
    row: Option<Handle>,
    /// The times left shown: each one's element and its display.
    countdowns: Vec<(Handle, TimeLeft)>,
    /// A profile is being laid out.
    processing: bool,
    /// The classic window (every tab at once).
    pub classic: bool,
    /// The tab buttons, in order.
    tab_table: Vec<Tab>,
}

impl Examination {
    /// The window under the toolbar's framework: its field and list found, its tab buttons, its
    /// buttons and the windows' fields heard, the interface keys heard; the first tab's buttons
    /// set; hidden.
    // D2-02629
    pub fn new(ui: &mut Ui, parent: FrameId, out: &mut Vec<Request>) -> Self {
        let frame = ui.new_child_frame(parent);
        let field = ui.element(name::FIELD);
        let mut me = Self {
            frame,
            field,
            list: None,
            request: ExamRequest::default(),
            current: name::GENERAL_TAB,
            x: 0,
            y: 0,
            bottom: 0,
            indent: 0,
            tabs: ALL_TABS,
            row: None,
            countdowns: Vec::new(),
            processing: false,
            classic: true,
            tab_table: Vec::new(),
        };
        if let Some(f) = field {
            me.list = ui.child_recursive(f, name::LIST_BOX);
            me.setup_tabs(ui);
        }
        if let Some(l) = ui.frame_listener(frame) {
            ui.listen_for_message(l, name::CLOSE_BUTTON, msg::BUTTON_RELEASE_ON);
            ui.listen_for_message(l, name::HELP_BUTTON, msg::BUTTON_RELEASE_ON);
            ui.listen_for_message(l, name::RESIZE, msg::MOUSE_PRESS);
            ui.listen_for_message(l, name::RESIZE, msg::MOUSE_RELEASE);
            ui.listen_for_message(l, name::BUTTON, msg::BUTTON_RELEASE_ON);
            for f in WINDOW_FIELDS {
                ui.listen_for_message(l, Token::of(f), BRING_TO_FRONT);
            }
            ui.listen_for_message(l, name::FIELD, msg::SET_SHOW_STATE);
            ui.listen_for_global_message(l, global::KEY_DOWN);
        }
        me.set_tab_states(ui, FIRST_TAB, FIRST_TAB);
        me.show(ui, false, out);
        me
    }

    /// The tab buttons found and heard: general (the first tab), details, effects, usage.
    // D2-02619
    fn setup_tabs(&mut self, ui: &mut Ui) {
        let Some(f) = self.field else { return };
        for (n, tabs) in [
            (name::GENERAL_TAB, FIRST_TAB),
            (name::DETAILS_TAB, 0x18C),
            (name::EFFECTS_TAB, 0x10),
            (name::USAGE_TAB, 0x60),
        ] {
            let button = ui.child_recursive(f, n);
            self.add_tab(Tab {
                name: n,
                button,
                tabs,
            });
            if let Some(l) = ui.frame_listener(self.frame) {
                ui.listen_for_message(l, n, msg::MOUSE_PRESS);
            }
        }
    }

    /// A tab put last in the table; one already there moves last, keeping what it held.
    // D2-02618, D2-02609, D2-02610, D2-02606
    fn add_tab(&mut self, t: Tab) {
        let kept = match self.tab_table.iter().position(|x| x.name == t.name) {
            Some(i) => self.tab_table.remove(i),
            None => t,
        };
        self.tab_table.push(kept);
    }

    /// The tab of a button's name, by the table.
    // D2-02608, D2-02605
    fn tab_named(&self, n: Token) -> Option<Tab> {
        self.tab_table.iter().find(|t| t.name == n).copied()
    }

    /// The table emptied.
    // D2-02611, D2-02601
    fn flush_tabs(&mut self) {
        self.tab_table.clear();
    }

    /// Each tab button's state: active when it shows a tab asked for, else disabled when no line
    /// was made for it, else inactive; the buttons shown in the tabbed window, hidden in the
    /// classic one.
    // D2-02612
    fn set_tab_states(&self, ui: &mut Ui, asked: u32, made: u32) {
        for t in &self.tab_table {
            let s = if t.tabs & asked != 0 {
                name::ACTIVE
            } else if t.tabs & made == 0 {
                name::DISABLED
            } else {
                name::INACTIVE
            };
            let Some(b) = t.button else { continue };
            if ui.node(b).is_some_and(|n| n.state != s) {
                ui.set_state(b, s);
            }
            let visible = ui
                .node(b)
                .is_some_and(|n| n.visible && n.parent.is_none_or(|p| ui.shown(p)));
            if !self.classic && !visible {
                ui.set_visible(b, true);
            } else if self.classic && visible {
                ui.set_visible(b, false);
            }
        }
    }

    /// The tabs asked for: one tab's (with Ctrl held, that tab added or taken away); the tab
    /// shown first, the last in the table among them.
    // D2-02617
    fn select_tabs(&mut self, ui: &Ui, tabs: u32) {
        if ui.ctrl_held() {
            if tabs & self.tabs == 0 {
                self.tabs |= tabs;
            } else {
                self.tabs &= !tabs;
            }
        } else {
            self.tabs = tabs;
        }
        self.current = name::GENERAL_TAB;
        for t in &self.tab_table {
            if t.tabs & tabs != 0 {
                self.current = t.name;
            }
        }
    }

    /// The tab after the shown one that is not disabled (round to the first).
    // D2-02615
    fn next_tab(&self, ui: &Ui) -> Option<Tab> {
        let n = self.tab_table.len();
        let at = self.tab_table.iter().position(|t| t.name == self.current)?;
        (1..n)
            .map(|k| self.tab_table[(at + k) % n])
            .find(|t| t.button.is_some_and(|b| !Self::disabled(ui, b)))
    }

    /// The tab before the shown one that is not disabled (round to the last).
    // D2-02616
    fn previous_tab(&self, ui: &Ui) -> Option<Tab> {
        let n = self.tab_table.len();
        let at = self.tab_table.iter().position(|t| t.name == self.current)?;
        (1..n)
            .map(|k| self.tab_table[(at + n - k) % n])
            .find(|t| t.button.is_some_and(|b| !Self::disabled(ui, b)))
    }

    fn disabled(ui: &Ui, b: Handle) -> bool {
        ui.node(b).is_some_and(|n| n.state == name::DISABLED)
    }

    /// The classic window or the tabbed one, as the player's options say: switched, the window
    /// changes its look and list, asks for every tab (classic) or the first (tabbed), finds its
    /// tab buttons again and asks to examine with its tabs.
    // D2-02625
    pub fn set_classic(&mut self, ui: &mut Ui, classic: bool, out: &mut Vec<Request>) {
        if classic == self.classic {
            return;
        }
        self.classic = classic;
        self.cleanup_rows(ui);
        let Some(f) = self.field else { return };
        let Some(internal) = ui.child_recursive(f, name::INTERNAL) else {
            return;
        };
        ui.set_state(internal, if classic { name::CLASSIC } else { name::TABBED });
        let list = if classic {
            self.select_tabs(ui, ALL_TABS);
            name::LIST_BOX
        } else {
            self.select_tabs(ui, FIRST_TAB);
            name::LIST_BOX_TABBED
        };
        self.list = ui.child_recursive(f, list);
        self.setup_tabs(ui);
        self.set_tab_states(ui, self.tabs, self.tabs);
        out.push(Request::Examination {
            tabs: Some(self.tabs),
        });
    }

    /// The rows and the times left gone; the next line at the top left.
    // D2-02600, D2-02602
    fn cleanup_rows(&mut self, ui: &mut Ui) {
        if let Some(l) = self.list {
            ui.list_flush(l);
        }
        for (c, d) in self.countdowns.drain(..) {
            d.close(ui);
            ui.remove(c);
        }
        self.x = 0;
        self.y = 0;
        self.bottom = 0;
        self.indent = 0;
        self.row = None;
    }

    /// What is examined changed: shown, the window waits for its profile.
    // D2-02624
    pub fn request_changed(&mut self, ui: &mut Ui, request: ExamRequest) {
        self.request = request;
        if self.shown(ui) {
            self.update(ui, request);
        }
    }

    /// The window emptied for a request: the tab buttons by its tabs, and a row saying nothing is
    /// examined (a request about nothing) or that its profile is on its way.
    // D2-02614
    pub fn update(&mut self, ui: &mut Ui, request: ExamRequest) {
        let row = if request.kind == 0 {
            name::EMPTY_ROW
        } else {
            name::PENDING_ROW
        };
        self.cleanup_rows(ui);
        self.set_tab_states(ui, request.tabs, request.tabs);
        let Some(list) = self.list else { return };
        if let Some(r) = ui.frame_create_element(TOOLBAR_LAYOUT_ENUM, row) {
            ui.list_add(list, r);
        }
    }

    /// A profile came: the tab buttons by the tabs it asks for (the first tab's added when none
    /// of them has a line) and those it has lines for; each line on a tab asked for laid out in
    /// one row, the row as tall as its lines, put in the list.
    // D2-02613
    pub fn update_profile(&mut self, ui: &mut Ui, p: &Profile) {
        self.cleanup_rows(ui);
        let mut asked = p.request.tabs;
        if asked & p.tabs == 0 {
            asked |= FIRST_TAB;
        }
        self.set_tab_states(ui, asked, p.tabs);
        self.processing = true;
        for l in p.lines.iter().filter(|l| l.tabs & asked != 0) {
            self.process_line(ui, l);
        }
        self.processing = false;
        if let Some(r) = self.row {
            let rect = ui.node(r).map(|n| n.rect).unwrap_or_default();
            if rect.height() < self.bottom {
                ui.resize_to(r, rect.width(), self.bottom);
            }
            if let Some(list) = self.list {
                ui.list_add(list, r);
                self.row = None;
            }
        }
    }

    /// A new line in the row, as tall as at least one line of text (16), keeping the indent
    /// (`keep`) or not.
    // D2-02599
    fn line_break(&mut self, keep: bool) {
        if self.bottom - self.y < 16 {
            self.bottom = self.y + 16;
        }
        self.y = self.bottom;
        self.x = 0;
        if keep {
            self.x = self.indent * 16;
        } else {
            self.indent = 0;
        }
    }

    /// A line's text: a number, a float with two decimals, yes or no, its own string reference,
    /// a 64-bit number; `None` for the kinds that have none.
    // D2-02598
    #[must_use]
    pub fn line_text(ui: &Ui, l: &Line) -> Option<StringInfo> {
        let table = ui
            .assets()
            .data_id_by_enum(flow::MESSAGES, flow::TABLE_MAPPING);
        let number = |v: Value| {
            let mut s = StringInfo::entry(table, Token::of("ID_Examination_Number").raw());
            s.variables.push(Variable {
                id: name::VALUE.raw(),
                value: v,
            });
            s
        };
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        match l.kind {
            kind::INT => Some(number(Value::Int(l.value as u32 as i32))),
            kind::FLOAT => Some(number(Value::Float {
                value: f64::from(f32::from_bits(l.value as u32)),
                decimals: 2,
            })),
            kind::BOOL => Some(StringInfo::entry(
                table,
                Token::of(if l.value as u32 == 0 {
                    "ID_Examination_False"
                } else {
                    "ID_Examination_True"
                })
                .raw(),
            )),
            kind::STRING => Some(l.text.clone().unwrap_or_default()),
            kind::INDENT => Some(StringInfo::entry(
                table,
                Token::of("ID_Examination_Tab").raw(),
            )),
            // D2-12184
            kind::LONG => Some(number(Value::Long(l.value as i64))),
            _ => None,
        }
    }

    /// The row's width (the list's, when the row is made).
    fn row_width(ui: &Ui, row: Handle) -> i32 {
        ui.node(row).map_or(0, |n| n.rect.width())
    }

    /// One line laid out in the row (made the list's width at the first line): text where the row
    /// is (on a new line, indented, when it does not fit and the line has more than its indent),
    /// cut to the row's width, as tall as its text (a string reference waiting on the string
    /// tables it reads takes the rest of the row, empty); a picture as wide as it is; a break; indents;
    /// a countdown, the time left until its end kept current by the clock (months and days above a
    /// month, days and hours above a day, hours and minutes above an hour, else minutes and
    /// seconds).
    // D2-02607
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
    fn process_line(&mut self, ui: &mut Ui, l: &Line) {
        let Some(list) = self.list else { return };
        if self.row.is_none() {
            let Some(r) = ui.frame_create_element(TOOLBAR_LAYOUT_ENUM, name::ROW_TEMPLATE) else {
                return;
            };
            let w = ui.node(list).map_or(0, |n| n.rect.width());
            let h = ui.node(r).map_or(0, |n| n.rect.height());
            ui.resize_to(r, w, h);
            self.row = Some(r);
        }
        let Some(row) = self.row else { return };
        let row_w = Self::row_width(ui, row);
        let made = match l.kind {
            kind::INT | kind::FLOAT | kind::BOOL | kind::STRING | kind::LONG => {
                let Some(info) = Self::line_text(ui, l) else {
                    return;
                };
                let Some(e) = self.element_for(ui, l.appearance) else {
                    return;
                };
                // Added at once, it is measured; waiting on the string tables it reads, the
                // window hears when they come, and the line takes what is left of the row.
                let (mut w, _) = if ui.set_string_info(e, &info) {
                    let text = String::from_utf16_lossy(&ui.assets().element_text(&info));
                    ui.inq_size_with_margins(e, &text).unwrap_or((0, 0))
                } else {
                    if let (Some(listener), Some(id)) =
                        (ui.frame_listener(self.frame), ui.node(e).map(|n| n.id))
                    {
                        ui.listen_for_message(listener, id, DOWNLOAD_DONE);
                    }
                    (row_w, 0)
                };
                if row_w <= self.x + w && self.indent * 16 < self.x {
                    self.line_break(true);
                }
                if row_w - self.x < w {
                    w = row_w - self.x;
                }
                ui.set_parent(e, row);
                ui.move_to(e, self.x, self.y);
                self.x += w;
                let h = ui.node(e).map_or(0, |n| n.rect.height());
                ui.resize_to(e, w, h);
                ui.recalculate_glyph_list(e);
                let paper = ui.text_state(e).map_or(h, |t| t.paper_h);
                let w = ui.node(e).map_or(w, |n| n.rect.width());
                ui.resize_to(e, w, paper);
                e
            }
            kind::IMAGE | kind::ICON => {
                if row_w <= self.x {
                    self.line_break(true);
                    self.x += 16;
                    self.indent += 1;
                }
                let Some(e) = ui.frame_create_element(TOOLBAR_LAYOUT_ENUM, name::IMAGE_TEMPLATE)
                else {
                    return;
                };
                ui.set_parent(e, row);
                ui.move_to(e, self.x, self.y);
                if l.kind == kind::IMAGE {
                    ui.add_image_to_state(e, l.value as u32, 0, state::STATELESS);
                } else {
                    ui.set_icon_desc(e, &l.icon);
                }
                let Some((gw, gh)) = ui.image_size(e) else {
                    return;
                };
                let (mut w, h) = (gw as i32, gh as i32);
                if row_w - self.x < w {
                    w = row_w - self.x;
                }
                self.x += w;
                ui.resize_to(e, w, h);
                e
            }
            kind::BREAK => {
                self.line_break(false);
                return;
            }
            kind::INDENT => {
                let n = (l.value as u32 as i32).max(1);
                self.x += n * 16;
                self.indent += n;
                return;
            }
            kind::COUNTDOWN => {
                let Some(e) = self.element_for(ui, l.appearance) else {
                    return;
                };
                let (mut w, _) = ui
                    .inq_size_with_margins(e, "(XXX XXX XX)")
                    .unwrap_or((0, 0));
                if row_w - self.x < w {
                    self.line_break(true);
                    if row_w - self.x < w {
                        w = row_w - self.x;
                    }
                }
                ui.set_parent(e, row);
                ui.move_to(e, self.x, self.y);
                ui.resize_to(e, w, 16);
                let mut d = TimeLeft::new(ui);
                d.init(ui, Some(e), l.time, 1, 1, 60);
                self.countdowns.push((e, d));
                e
            }
            kind::EFFECT => {
                self.process_effect_line(ui, l);
                return;
            }
            _ => return,
        };
        let h = ui.node(made).map_or(0, |n| n.rect.height());
        if self.bottom < h + self.y {
            self.bottom = h + self.y;
        }
    }

    /// An effect line: its own lines laid out in its place, but for the effects among them.
    // D2-02603
    fn process_effect_line(&mut self, ui: &mut Ui, l: &Line) {
        for sub in l.effect.iter().filter(|s| s.kind != kind::EFFECT) {
            self.process_line(ui, sub);
        }
    }

    /// A line's own element (its appearance), else the window's text element.
    fn element_for(&self, ui: &mut Ui, appearance: u32) -> Option<Handle> {
        (appearance != 0)
            .then(|| ui.frame_create_element(TOOLBAR_LAYOUT_ENUM, Token(appearance)))
            .flatten()
            .or_else(|| ui.frame_create_element(TOOLBAR_LAYOUT_ENUM, name::TEXT_TEMPLATE))
    }

    fn shown_now(&self, ui: &Ui) -> bool {
        ui.frame_shown(self.frame)
    }
}

impl Examination {
    /// The notices it registers for as it is made, by their own numbers.
    pub const NOTICES: &'static [u32] = &[0x45F, 0x417, 0x46D, 0x441, 0x45D, 0x1_040C];

    /// Its notices as the panels name them.
    #[must_use]
    pub fn notices_of() -> Vec<u32> {
        crate::notices::keys(Self::NOTICES)
    }
}

impl Child for Examination {
    fn notices(&self) -> Vec<u32> {
        Self::notices_of()
    }

    fn frame(&self) -> FrameId {
        self.frame
    }

    /// Shown: the window comes to the front, shows what is examined and asks to examine with its
    /// tabs; hidden: it asks to stop examining and empties.
    // D2-02621
    fn show(&mut self, ui: &mut Ui, on: bool, out: &mut Vec<Request>) {
        ui.frame_show(self.frame, on);
        let Some(f) = self.field else { return };
        if self.shown_now(ui) {
            ui.broadcast(f, BRING_TO_FRONT, 0);
            let r = self.request;
            self.update(ui, r);
            out.push(Request::Examination {
                tabs: Some(self.tabs),
            });
        } else {
            out.push(Request::Examination { tabs: None });
            self.update(ui, ExamRequest::default());
        }
        let shown = self.shown_now(ui);
        ui.set_visible(f, shown);
    }

    /// The window's field shown or hidden; a window come to the front; the title bar pressed (to
    /// the front) and let go (the places kept); its button, close and help buttons; a tab button
    /// pressed; the keys for the next and previous tab (the tabbed window).
    // D2-02623, D2-02622
    fn listen(&mut self, ui: &mut Ui, mail: &Mail, out: &mut Vec<Request>) {
        match *mail {
            Mail::Global {
                what: global::KEY_DOWN,
                param,
            } => {
                if !self.shown_now(ui) || self.classic {
                    return;
                }
                let tab = match param {
                    action::NEXT_TAB => self.next_tab(ui),
                    action::PREVIOUS_TAB => self.previous_tab(ui),
                    _ => return,
                };
                if let Some(t) = tab {
                    self.select_tabs(ui, t.tabs);
                    out.push(Request::Examination {
                        tabs: Some(self.tabs),
                    });
                }
            }
            Mail::Element {
                element,
                what,
                from,
                param,
                ..
            } => {
                // A line's text came: the window no longer hears it, and (unless a profile is
                // being laid out) empties to the request examined, waiting for its profile again.
                if what == DOWNLOAD_DONE {
                    if let Some(listener) = ui.frame_listener(self.frame) {
                        ui.unregister_for_message(listener, element, DOWNLOAD_DONE);
                    }
                    if !self.processing {
                        let r = self.request;
                        self.update(ui, r);
                    }
                    return;
                }
                if what == msg::SET_SHOW_STATE && Some(from) == self.field {
                    self.show(ui, param != 0, out);
                }
                if what == BRING_TO_FRONT {
                    window_to_front(ui, self.field, Some(from));
                }
                if element == name::RESIZE {
                    if what == msg::MOUSE_PRESS {
                        if let Some(f) = self.field {
                            ui.broadcast(f, BRING_TO_FRONT, 0);
                        }
                    }
                    if what == msg::MOUSE_RELEASE {
                        gameplay::GamePlay::save_locations_changed(out);
                    }
                }
                if element == name::BUTTON && what == msg::BUTTON_RELEASE_ON {
                    let shown = self.shown_now(ui);
                    self.show(ui, !shown, out);
                } else if element == name::CLOSE_BUTTON && what == msg::BUTTON_RELEASE_ON {
                    self.show(ui, false, out);
                } else if element == name::HELP_BUTTON && what == msg::BUTTON_RELEASE_ON {
                    out.push(Request::Help { open: true });
                } else if what == msg::MOUSE_PRESS && self.shown_now(ui) {
                    let Some(t) = self.tab_named(element) else {
                        return;
                    };
                    if t.button.is_some_and(|b| !Self::disabled(ui, b)) {
                        self.select_tabs(ui, t.tabs);
                        out.push(Request::Examination {
                            tabs: Some(self.tabs),
                        });
                    }
                }
            }
            _ => {}
        }
    }

    /// The scripts' notices: a profile, what is examined changed, open the window, the options
    /// read again (the classic window or the tabbed one).
    fn notice(&mut self, ui: &mut Ui, n: &Notice, out: &mut Vec<Request>) {
        let record = n.records.iter().flatten().next();
        match (n.native, record) {
            (notice::UPDATE_EXAMINATION_PROFILE, Some(Record::Profile(p))) => {
                self.update_profile(ui, p);
            }
            (notice::EXAMINATION_REQUEST_CHANGED, Some(Record::Request(r))) => {
                self.request_changed(ui, *r);
            }
            // D2-02596
            (notice::OPEN_EXAMINATION_PANEL | crate::inventory::notice::OPEN_EXAMINATION, _) => {
                self.show(ui, true, out);
            }
            (
                notice::RELOAD_OPTIONS | crate::options::notice::OPTIONS_CHANGED,
                Some(Record::OptionBits(bits)),
            ) => {
                self.set_classic(ui, bits & CLASSIC_EXAMINATION != 0, out);
            }
            _ => {}
        }
    }

    /// The clock: each time left written again when it is time to look.
    fn update(&mut self, ui: &mut Ui, _view: &gameplay::View<'_>, _out: &mut Vec<Request>) {
        for (_, d) in &mut self.countdowns {
            d.tick(ui);
        }
    }

    /// The window goes: its rows, its tab table, its listening.
    // D2-02620, D2-02628, D2-02626
    fn close(mut self: Box<Self>, ui: &mut Ui) {
        self.cleanup_rows(ui);
        self.flush_tabs();
        ui.drop_frame(self.frame);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
