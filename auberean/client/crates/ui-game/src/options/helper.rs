//! The options window's rows: a yes-or-no option (a check box), a number (a slider with a step
//! button at each end), a choice (a drop-down menu), and a section (a header that opens and closes
//! the list of the options under it).
//!
//! Each row is made from its template in the toolbar's layout, labelled from a string table, with
//! its help as the row's tooltip, and keeps a preference ([`super::preference`]): what was applied.
//! Its control shows what the player is setting. A row has changed while its control differs from
//! its preference; applied, the control's value becomes the preference's; recalled, the
//! preference's goes back into the control; defaulted, the preference's default goes there.
//!
//! What a row's control says (a box ticked, the slider moved or stepped, a choice made, a section
//! opened or closed) is handed on to the page the row is on, as the row heard it.

use auberean_ui::framework::FrameId;
use auberean_ui::token::{attr, msg, state};
use auberean_ui::{Handle, Mail, Token, Ui, Value};

use super::preference::{self, Description};

/// The layout the rows' templates are in (the toolbar's).
pub const LAYOUT_ENUM: u32 = 0x4100_0003;
/// The string table option labels are in by default.
pub const OPTION_STRINGS: u32 = 0x4100_0018;

/// The rows' templates and their parts.
pub mod name {
    use auberean_ui::Token;
    pub const CHECKBOX: Token = Token::of("OptionCheckbox");
    pub const SLIDER: Token = Token::of("OptionSlider");
    pub const DROP_DOWN: Token = Token::of("OptionDropDown");
    pub const TREE: Token = Token::of("OptionTree");
    pub const TEXT: Token = Token::of("OptionText");
    pub const CHECKBOX_BUTTON: Token = Token::of("CheckboxButton");
    pub const SLIDER_BAR: Token = Token::of("SliderBar");
    pub const SLIDER_LEFT: Token = Token::of("SliderLeft");
    pub const SLIDER_RIGHT: Token = Token::of("SliderRight");
    pub const DROP_DOWN_MENU: Token = Token::of("DropDownMenu");
    pub const TREE_HEADER: Token = Token::of("TreeHeader");
    pub const TREE_LIST: Token = Token::of("TreeList");
    /// A choice's number on its menu row.
    pub const KEY: Token = Token::of("Key");
    /// What a section says when it opens or closes.
    pub const EXPANDER_TOGGLE: Token = Token::of("Expander_Toggle");
}

/// A row's control and the preference it keeps.
#[derive(Debug)]
pub enum Control {
    Flag {
        checkbox: Option<Handle>,
        pref: preference::Flag,
    },
    Number {
        slider: Option<Handle>,
        left: Option<Handle>,
        right: Option<Handle>,
        pref: preference::Number,
    },
    Choice {
        menu: Option<Handle>,
        pref: preference::Choice,
    },
    /// A section: its list, its header, whether it is open, its rows (their places in the page's
    /// rows, in order).
    Section {
        list: Option<Handle>,
        header: Option<Handle>,
        open: bool,
        rows: Vec<usize>,
    },
}

/// One row of an options page: a child framework of the page.
#[derive(Debug)]
pub struct Row {
    frame: FrameId,
    /// The row's element, made from its template.
    pub element: Option<Handle>,
    /// Its template's name, which says what kind of row it is.
    pub kind: Token,
    /// Its label's table (an enumeration value), label and help (0: none).
    pub table: u32,
    pub label: u32,
    pub help: u32,
    pub control: Control,
}

/// What a row heard and hands on to its page: the message as heard, and a preference set to
/// the control's value (for the client's own preferences to follow at once).
#[derive(Debug)]
pub struct Heard {
    pub element: Token,
    pub from: Handle,
    pub what: Token,
    pub param: u32,
    pub change: Option<Change>,
}

/// A preference the player set on a row, before it is applied.
#[derive(Clone, Debug, PartialEq)]
pub enum Change {
    Flag(preference::Flag),
    Number(preference::Number),
    Choice(preference::Choice),
}

/// The float the client stores, read from an element's attribute.
fn float_attr(ui: &Ui, h: Option<Handle>, a: Token) -> f32 {
    h.and_then(|h| ui.node(h))
        .and_then(|n| n.float_attr(a))
        .unwrap_or(0.0)
}

/// A check box's highlight.
fn highlighted(ui: &Ui, h: Option<Handle>) -> bool {
    h.and_then(|h| ui.node(h))
        .is_some_and(|n| n.bool_attr(attr::HIGHLIGHTED))
}

/// A string by its table's enumeration value.
#[must_use]
pub fn text(ui: &Ui, table_enum: u32, id: u32) -> String {
    let table = ui
        .assets()
        .data_id_by_enum(table_enum, crate::flow::TABLE_MAPPING);
    ui.assets().text(table, id).unwrap_or_default()
}

impl Row {
    /// A row on `page`: its framework, its labels' table the options' own.
    // D2-03122
    fn base(ui: &mut Ui, page: FrameId, control: Control) -> Self {
        Self {
            frame: ui.new_child_frame(page),
            element: None,
            kind: Token(0),
            table: OPTION_STRINGS,
            label: 0,
            help: 0,
            control,
        }
    }

    /// The row's element made from the template `kind` (once).
    // D2-03124
    fn create(&mut self, ui: &mut Ui, kind: Token) -> Option<Handle> {
        if self.element.is_none() {
            self.element = ui.frame_create_element(LAYOUT_ENUM, kind);
            self.element?;
            self.kind = kind;
        }
        self.element
    }

    /// A part of the row's element.
    // D2-03125
    fn retrieve(&self, ui: &Ui, part: Token) -> Option<Handle> {
        ui.child_recursive(self.element?, part)
    }

    /// Hear one of the row's parts say something.
    fn hear(&self, ui: &mut Ui, part: Token, what: Token) {
        if let Some(l) = ui.frame_listener(self.frame) {
            ui.listen_for_message(l, part, what);
        }
    }

    /// A yes-or-no row: its check box, unticked and applied.
    // D2-03130
    pub fn flag(ui: &mut Ui, page: FrameId) -> Self {
        let mut r = Self::base(
            ui,
            page,
            Control::Flag {
                checkbox: None,
                pref: preference::Flag::new(),
            },
        );
        r.create(ui, name::CHECKBOX);
        let checkbox = r.retrieve(ui, name::CHECKBOX_BUTTON);
        if let Control::Flag { pref, .. } = &mut r.control {
            pref.set_default(false);
            pref.apply();
        }
        if let Some(c) = checkbox {
            ui.set_attr(c, attr::HIGHLIGHTED, Value::Bool(false));
        }
        r.hear(ui, name::CHECKBOX_BUTTON, msg::HIGHLIGHT_STATE_CHANGE);
        if let Control::Flag { checkbox: c, .. } = &mut r.control {
            *c = checkbox;
        }
        r
    }

    /// A number's row: its slider and step buttons, over 0 to 1, a half by default and applied,
    /// the slider where the value is.
    // D2-03144
    pub fn number(ui: &mut Ui, page: FrameId) -> Self {
        let mut r = Self::base(
            ui,
            page,
            Control::Number {
                slider: None,
                left: None,
                right: None,
                pref: preference::Number::new(),
            },
        );
        r.create(ui, name::SLIDER);
        let slider = r.retrieve(ui, name::SLIDER_BAR);
        let mut at = 0.0;
        if let Control::Number { pref, .. } = &mut r.control {
            pref.set_range(0.0, 1.0);
            pref.set_default(0.5);
            pref.apply();
            at = pref.get();
        }
        if let Some(s) = slider {
            ui.set_attr(s, attr::X_POSITION, Value::Float(at));
        }
        r.hear(ui, name::SLIDER_BAR, msg::NEW_POSITION);
        r.hear(ui, name::SLIDER_LEFT, msg::MOUSE_PRESS);
        r.hear(ui, name::SLIDER_RIGHT, msg::MOUSE_PRESS);
        let left = r.retrieve(ui, name::SLIDER_LEFT);
        let right = r.retrieve(ui, name::SLIDER_RIGHT);
        if let Control::Number {
            slider: s,
            left: l,
            right: g,
            ..
        } = &mut r.control
        {
            *s = slider;
            *l = left;
            *g = right;
        }
        r
    }

    /// A choice's row: its menu, with no choices yet.
    // D2-03159
    pub fn choice(ui: &mut Ui, page: FrameId) -> Self {
        let mut r = Self::base(
            ui,
            page,
            Control::Choice {
                menu: None,
                pref: preference::Choice::new(),
            },
        );
        r.create(ui, name::DROP_DOWN);
        let menu = r.retrieve(ui, name::DROP_DOWN_MENU);
        if let Control::Choice { pref, .. } = &mut r.control {
            pref.set_default(0);
            pref.apply();
        }
        r.hear(ui, name::DROP_DOWN_MENU, msg::NEW_SELECTION);
        if let Control::Choice { menu: m, .. } = &mut r.control {
            *m = menu;
        }
        r
    }

    /// A section's row: its header and list, closed.
    // D2-03189
    pub fn section(ui: &mut Ui, page: FrameId) -> Self {
        let mut r = Self::base(
            ui,
            page,
            Control::Section {
                list: None,
                header: None,
                open: false,
                rows: Vec::new(),
            },
        );
        r.create(ui, name::TREE);
        let header = r.retrieve(ui, name::TREE_HEADER);
        let list = r.retrieve(ui, name::TREE_LIST);
        r.hear(ui, name::TREE_HEADER, msg::BUTTON_RELEASE_ON);
        if let Control::Section {
            list: l, header: h, ..
        } = &mut r.control
        {
            *h = header;
            *l = list;
        }
        r
    }

    /// The row's framework.
    #[must_use]
    pub const fn frame(&self) -> FrameId {
        self.frame
    }

    /// The row's label from the client's own table of preference strings, with no help.
    // D2-03126
    pub fn describe_by_label(&mut self, ui: &mut Ui, label: u32) {
        self.describe(ui, OPTION_STRINGS, label, 0);
    }

    /// The row's label and help: its text the label; its tooltip the help, tooltips on only with
    /// one.
    // D2-03127
    pub fn describe(&mut self, ui: &mut Ui, table: u32, label: u32, help: u32) {
        let Some(t) = self.retrieve(ui, name::TEXT) else {
            return;
        };
        self.label = label;
        self.table = table;
        self.help = help;
        let words = text(ui, table, label);
        ui.set_text(t, &words);
        let Some(e) = self.element else { return };
        if help != 0 {
            let tip = text(ui, table, help);
            ui.set_tooltip(e, &tip);
        }
        ui.set_attr(e, attr::TOOLTIPS_ON, Value::Bool(help != 0));
    }

    /// What the row is: its template's name and its label.
    // D2-03128
    #[must_use]
    pub const fn identify(&self) -> (Token, u32) {
        (self.kind, self.label)
    }

    /// The row, when it is a yes-or-no one.
    // D2-03131
    #[must_use]
    pub const fn is_flag(&self) -> bool {
        matches!(self.control, Control::Flag { .. })
    }

    /// The row, when it is a number's.
    // D2-03145
    #[must_use]
    pub const fn is_number(&self) -> bool {
        matches!(self.control, Control::Number { .. })
    }

    /// The row, when it is a choice's.
    // D2-03160
    #[must_use]
    pub const fn is_choice(&self) -> bool {
        matches!(self.control, Control::Choice { .. })
    }

    // ----------------------------------------------------------------------------------------
    // Yes or no
    // ----------------------------------------------------------------------------------------

    /// Tick the box or not; the box's state before.
    // D2-03133
    pub fn set_flag(&mut self, ui: &mut Ui, on: bool) -> bool {
        let Control::Flag { checkbox, .. } = self.control else {
            return false;
        };
        let Some(c) = checkbox else { return false };
        let was = highlighted(ui, Some(c));
        ui.set_attr(c, attr::HIGHLIGHTED, Value::Bool(on));
        was
    }

    /// The default (and the preference's value with it), the box ticked to match; the default
    /// before.
    // D2-03134
    pub fn set_flag_default(&mut self, ui: &mut Ui, on: bool) -> bool {
        let Control::Flag { checkbox, pref } = &mut self.control else {
            return false;
        };
        let was = pref.default_value();
        pref.set_default(on);
        if let Some(c) = *checkbox {
            ui.set_attr(c, attr::HIGHLIGHTED, Value::Bool(on));
        }
        was
    }

    /// Whether the box is ticked.
    // D2-03135
    #[must_use]
    pub fn flag_now(&self, ui: &Ui) -> bool {
        match self.control {
            Control::Flag { checkbox, .. } => highlighted(ui, checkbox),
            _ => false,
        }
    }

    // ----------------------------------------------------------------------------------------
    // A number
    // ----------------------------------------------------------------------------------------

    /// The number's range, its ends put in order.
    // D2-03147
    pub fn set_range(&mut self, a: f32, b: f32) {
        let Control::Number { slider, pref, .. } = &mut self.control else {
            return;
        };
        if slider.is_none() {
            return;
        }
        let (least, greatest) = if b > a { (a, b) } else { (b, a) };
        pref.set_range(least, greatest);
    }

    /// Move the slider to a value (held to the range, over a range that is not empty), and say
    /// so as the slider would; the preference's value.
    // D2-03148
    pub fn set_number(&mut self, ui: &mut Ui, v: f32) -> f32 {
        let Control::Number { slider, pref, .. } = &self.control else {
            return 0.0;
        };
        let Some(s) = *slider else { return 0.0 };
        let was = pref.get();
        let (least, greatest) = pref.range();
        if least < greatest {
            let mut v = v;
            if v < least {
                v = least;
            }
            if v > greatest {
                v = greatest;
            }
            let at = (v - least) / (greatest - least);
            ui.set_attr(s, attr::X_POSITION, Value::Float(at));
            ui.broadcast(s, msg::NEW_POSITION, 0);
        }
        was
    }

    /// The default (held to the range; the preference's value with it), the slider moved to it;
    /// the default before.
    // D2-03149
    pub fn set_number_default(&mut self, ui: &mut Ui, v: f32) -> f32 {
        let Control::Number { pref, .. } = &mut self.control else {
            return 0.0;
        };
        let was = pref.default_value();
        let (least, greatest) = pref.range();
        let mut v = v;
        if v < least {
            v = least;
        }
        if v > greatest {
            v = greatest;
        }
        pref.set_default(v);
        self.set_number(ui, v);
        was
    }

    /// The value the slider stands at.
    // D2-03150
    #[must_use]
    pub fn number_now(&self, ui: &Ui) -> f32 {
        let Control::Number { slider, pref, .. } = &self.control else {
            return 0.0;
        };
        if slider.is_none() {
            return 0.0;
        }
        let at = float_attr(ui, *slider, attr::X_POSITION);
        let (least, greatest) = pref.range();
        (greatest - least) * at + least
    }

    // ----------------------------------------------------------------------------------------
    // A choice
    // ----------------------------------------------------------------------------------------

    /// A choice on the menu, before the first with a greater number.
    // D2-03161
    pub fn add_choice(&mut self, ui: &mut Ui, v: i32, words: &str) {
        let Control::Choice { menu, .. } = self.control else {
            return;
        };
        let Some(m) = menu else { return };
        let mut at = 0;
        while at < ui.menu_len(m) {
            let k = ui
                .menu_item(m, at)
                .and_then(|i| ui.node(i))
                .and_then(|n| n.int_attr(name::KEY));
            if k.is_some_and(|k| v < k) {
                break;
            }
            at += 1;
        }
        if let Some(item) = ui.menu_insert_text(m, words, at) {
            ui.set_attr(item, name::KEY, Value::Int(v));
        }
    }

    /// A choice on the menu and in the preference's choices.
    // D2-03162
    pub fn assign_choice(&mut self, ui: &mut Ui, v: i32, table: u32, label: u32) {
        let words = text(ui, table, label);
        self.add_choice(ui, v, &words);
        if let Control::Choice { pref, .. } = &mut self.control {
            pref.assign_choice(v, (table, label));
        }
    }

    /// The menu's choice made the one with number `v`, when the menu has it; the choice before.
    // D2-03163
    pub fn set_choice(&mut self, ui: &mut Ui, v: i32) -> i32 {
        let Control::Choice { menu, .. } = self.control else {
            return 0;
        };
        let Some(m) = menu else { return 0 };
        let was = self.choice_now(ui);
        for at in 0..ui.menu_len(m) {
            let Some(item) = ui.menu_item(m, at) else {
                continue;
            };
            if ui.node(item).and_then(|n| n.int_attr(name::KEY)) == Some(v) {
                ui.menu_select_item(m, Some(item));
                break;
            }
        }
        was
    }

    /// The default (and the preference's value with it, when it is one of the choices), the menu
    /// set to it; the default before.
    // D2-03164
    pub fn set_choice_default(&mut self, ui: &mut Ui, v: i32) -> i32 {
        let Control::Choice { pref, .. } = &mut self.control else {
            return 0;
        };
        let was = pref.default_value();
        pref.set_default(v);
        self.set_choice(ui, v);
        was
    }

    /// The number of the menu's choice (0 with none).
    // D2-03165
    #[must_use]
    pub fn choice_now(&self, ui: &Ui) -> i32 {
        let Control::Choice { menu, .. } = self.control else {
            return 0;
        };
        menu.and_then(|m| ui.menu_selected_item(m))
            .and_then(|i| ui.node(i))
            .and_then(|n| n.int_attr(name::KEY))
            .unwrap_or(0)
    }

    // ----------------------------------------------------------------------------------------
    // The row's own (a section's are its rows', see [`Rows`])
    // ----------------------------------------------------------------------------------------

    /// Whether the control differs from the preference.
    // D2-03136, D2-03151, D2-03166
    #[must_use]
    pub fn changed(&self, ui: &Ui) -> bool {
        match &self.control {
            Control::Flag { pref, .. } => self.flag_now(ui) != pref.get(),
            Control::Number { pref, .. } => {
                let d = self.number_now(ui) - pref.get();
                d * d >= 0.0001
            }
            Control::Choice { pref, .. } => self.choice_now(ui) != pref.get(),
            Control::Section { .. } => false,
        }
    }

    /// The control's value made the preference's, and marked applied.
    // D2-03137, D2-03152, D2-03167
    pub fn apply(&mut self, ui: &Ui) {
        let flag = self.flag_now(ui);
        let number = self.number_now(ui);
        let choice = self.choice_now(ui);
        match &mut self.control {
            Control::Flag { pref, .. } => {
                pref.set_value(flag);
                pref.apply();
            }
            Control::Number { pref, .. } => {
                pref.set_value(number);
                pref.apply();
            }
            Control::Choice { pref, .. } => {
                pref.set_value(choice);
                pref.apply();
            }
            Control::Section { .. } => {}
        }
    }

    /// The control's value tried in the preference, what it held marked applied (the rendering's,
    /// kept or put back by the answer to a question).
    pub fn submit(&mut self, ui: &Ui) {
        let flag = self.flag_now(ui);
        let number = self.number_now(ui);
        let choice = self.choice_now(ui);
        match &mut self.control {
            Control::Flag { pref, .. } => {
                pref.apply();
                pref.set_value(flag);
            }
            Control::Number { pref, .. } => {
                pref.apply();
                pref.set_value(number);
            }
            Control::Choice { pref, .. } => {
                pref.apply();
                pref.set_value(choice);
            }
            Control::Section { .. } => {}
        }
    }

    /// What was tried in the preference kept, or put back; the control stays as it is.
    pub fn settle(&mut self, keep: bool) {
        match &mut self.control {
            Control::Flag { pref, .. } => {
                if keep {
                    pref.apply();
                } else {
                    pref.recall();
                }
            }
            Control::Number { pref, .. } => {
                if keep {
                    pref.apply();
                } else {
                    pref.recall();
                }
            }
            Control::Choice { pref, .. } => {
                if keep {
                    pref.apply();
                } else {
                    pref.recall();
                }
            }
            Control::Section { .. } => {}
        }
    }

    /// The preference's value back in the control.
    // D2-03138, D2-03153, D2-03168
    pub fn recall(&mut self, ui: &mut Ui) {
        match &self.control {
            Control::Flag { pref, .. } => {
                let v = pref.get();
                self.set_flag(ui, v);
            }
            Control::Number { pref, .. } => {
                let v = pref.get();
                self.set_number(ui, v);
            }
            Control::Choice { pref, .. } => {
                let v = pref.get();
                self.set_choice(ui, v);
            }
            Control::Section { .. } => {}
        }
    }

    /// The preference's default in the control.
    // D2-03139, D2-03154, D2-03169
    pub fn to_default(&mut self, ui: &mut Ui) {
        match &self.control {
            Control::Flag { pref, .. } => {
                let v = pref.default_value();
                self.set_flag(ui, v);
            }
            Control::Number { pref, .. } => {
                let v = pref.default_value();
                self.set_number(ui, v);
            }
            Control::Choice { pref, .. } => {
                let v = pref.default_value();
                self.set_choice(ui, v);
            }
            Control::Section { .. } => {}
        }
    }

    /// The control shows the preference's value; done.
    // D2-03142, D2-03157, D2-03172
    pub fn refresh(&mut self, ui: &mut Ui) -> bool {
        self.recall(ui);
        true
    }

    /// The control shows another preference's value (one the client changed); done.
    // D2-03143, D2-03158, D2-03173
    pub fn refresh_from(&mut self, ui: &mut Ui, change: &Change) -> bool {
        match (&mut self.control, change) {
            (Control::Flag { pref, .. }, Change::Flag(p)) => pref.copy(p),
            (Control::Number { pref, .. }, Change::Number(p)) => pref.copy(p),
            (Control::Choice { pref, .. }, Change::Choice(p)) => pref.copy(p),
            _ => false,
        };
        match change {
            Change::Flag(p) => {
                self.set_flag(ui, p.get());
            }
            Change::Number(p) => {
                self.set_number(ui, p.get());
            }
            Change::Choice(p) => {
                self.set_choice(ui, p.get());
            }
        }
        true
    }

    /// A preference of the client's taken into the row: its values and description (the label
    /// and help from it), then the control from it. Only one of the row's own kind is taken.
    // D2-03132, D2-03146, D2-03177
    pub fn merge(&mut self, ui: &mut Ui, from: &Change) {
        let d = match (&mut self.control, from) {
            (Control::Flag { pref, .. }, Change::Flag(p)) => {
                pref.copy(p);
                pref.description
            }
            (Control::Number { pref, .. }, Change::Number(p)) => {
                pref.copy(p);
                pref.description
            }
            (Control::Choice { pref, menu }, Change::Choice(p)) => {
                pref.copy(p);
                if let Some(m) = *menu {
                    ui.menu_flush(m);
                }
                let choices = pref.list_choices().to_vec();
                let d = pref.description;
                let literals: Vec<Option<String>> = choices
                    .iter()
                    .map(|(v, _)| pref.literal_choice(*v).map(str::to_owned))
                    .collect();
                for ((v, (table, label)), literal) in choices.into_iter().zip(literals) {
                    let words = literal.unwrap_or_else(|| text(ui, table, label));
                    self.add_choice(ui, v, &words);
                }
                d
            }
            _ => return,
        };
        self.describe(ui, d.table, d.label, d.help);
        self.refresh(ui);
    }

    /// The preference set to the control's value, as the player sets it.
    fn change(&self, ui: &Ui) -> Option<Change> {
        match &self.control {
            Control::Flag { pref, .. } => {
                let mut p = preference::Flag::new();
                p.copy(pref);
                p.set_value(self.flag_now(ui));
                Some(Change::Flag(p))
            }
            Control::Number { pref, .. } => {
                let mut p = preference::Number::new();
                p.copy(pref);
                p.set_value(self.number_now(ui));
                Some(Change::Number(p))
            }
            Control::Choice { pref, .. } => {
                let mut p = preference::Choice::new();
                p.copy(pref);
                p.set_value(self.choice_now(ui));
                Some(Change::Choice(p))
            }
            Control::Section { .. } => None,
        }
    }

    /// The row's preference's description.
    #[must_use]
    pub fn description(&self) -> Option<Description> {
        match &self.control {
            Control::Flag { pref, .. } => Some(pref.description),
            Control::Number { pref, .. } => Some(pref.description),
            Control::Choice { pref, .. } => Some(pref.description),
            Control::Section { .. } => None,
        }
    }

    /// What the row's parts said, acted on; what the page hears of it.
    ///
    /// - a check box: its highlight changed: the preference set to it, handed on;
    /// - a slider's step buttons: a tenth of the range down or up; the slider moved: the
    ///   preference set to it, handed on;
    /// - a menu's new choice: the preference set to it, handed on;
    /// - a section's header: the section opened or closed, handed on as such.
    // D2-03140, D2-03155, D2-03170
    pub fn listen(&mut self, ui: &mut Ui, mail: &Mail) -> Option<Heard> {
        let Mail::Element {
            element,
            from,
            what,
            param,
        } = *mail
        else {
            return None;
        };
        let heard = |change| {
            Some(Heard {
                element,
                from,
                what,
                param,
                change,
            })
        };
        match &self.control {
            Control::Flag { checkbox, .. } => {
                if Some(from) == *checkbox && what == msg::HIGHLIGHT_STATE_CHANGE {
                    return heard(self.change(ui));
                }
                None
            }
            Control::Number {
                slider,
                left,
                right,
                pref,
            } => {
                slider.as_ref()?;
                let (slider, left, right) = (*slider, *left, *right);
                if what == msg::MOUSE_PRESS && (Some(from) == left || Some(from) == right) {
                    let (least, greatest) = pref.range();
                    let step = (greatest - least) * 0.1;
                    if Some(from) == left {
                        let v = self.number_now(ui) - step;
                        self.set_number(ui, v);
                    }
                    if Some(from) == right {
                        let v = self.number_now(ui) + step;
                        self.set_number(ui, v);
                    }
                }
                if what == msg::NEW_POSITION && Some(from) == slider {
                    return heard(self.change(ui));
                }
                None
            }
            Control::Choice { menu, .. } => {
                let m = (*menu)?;
                if from == m && what == msg::NEW_SELECTION && ui.menu_selected_item(m).is_some() {
                    return heard(self.change(ui));
                }
                None
            }
            // The page opens or closes the section (it holds the section's rows).
            Control::Section { header, .. } => {
                if Some(from) != *header {
                    return None;
                }
                Some(Heard {
                    element,
                    from,
                    what: name::EXPANDER_TOGGLE,
                    param,
                    change: None,
                })
            }
        }
    }

    /// The row goes: its element and its framework.
    // D2-03123, D2-03141, D2-03156, D2-03171, D2-03190, D2-03192, D2-03129
    pub fn close(self, ui: &mut Ui) {
        if let Some(e) = self.element {
            ui.remove(e);
        }
        ui.drop_frame(self.frame);
    }
}

/// A page's rows, by place: the sections hold their rows' places.
#[derive(Debug, Default)]
pub struct Rows {
    pub all: Vec<Row>,
}

impl Rows {
    /// A section's row added to it.
    // D2-03193
    pub fn add_to_section(&mut self, section: usize, row: usize) {
        if let Some(Row {
            control: Control::Section { rows, .. },
            ..
        }) = self.all.get_mut(section)
        {
            rows.push(row);
        }
    }

    /// A section's rows.
    fn section_rows(&self, section: usize) -> Vec<usize> {
        match self.all.get(section).map(|r| &r.control) {
            Some(Control::Section { rows, .. }) => rows.clone(),
            _ => Vec::new(),
        }
    }

    /// How many rows a section has.
    // D2-11178
    #[must_use]
    pub fn count(&self, section: usize) -> usize {
        self.section_rows(section).len()
    }

    /// A section's row at `at`.
    // D2-03181
    #[must_use]
    pub fn get(&self, section: usize, at: usize) -> Option<usize> {
        self.section_rows(section).get(at).copied()
    }

    /// The section (a row) and its list and header.
    fn parts(&self, section: usize) -> Option<(Option<Handle>, Option<Handle>, Option<Handle>)> {
        let r = self.all.get(section)?;
        match &r.control {
            Control::Section { list, header, .. } => Some((r.element, *list, *header)),
            _ => None,
        }
    }

    /// The section as tall as its header and its rows when open; its list as tall as its rows.
    // D2-03178
    pub fn refit(&self, ui: &mut Ui, section: usize) {
        let Some((Some(e), Some(list), Some(header))) = self.parts(section) else {
            return;
        };
        let rows: i32 = ui
            .list_items(list)
            .iter()
            .filter_map(|i| ui.node(*i))
            .map(|n| n.rect.height())
            .sum();
        let (w, head) = (
            ui.node(e).map_or(0, |n| n.rect.width()),
            ui.node(header).map_or(0, |n| n.rect.height()),
        );
        ui.resize_to(e, w, head + rows);
        let lw = ui.node(list).map_or(0, |n| n.rect.width());
        ui.resize_to(list, lw, rows);
        ui.update(list);
    }

    /// Every row of a section shows its preference's value; whether all did.
    // D2-03179
    pub fn refresh(&mut self, ui: &mut Ui, section: usize) -> bool {
        let mut all = true;
        for r in self.section_rows(section) {
            if let Some(row) = self.all.get_mut(r) {
                all &= row.refresh(ui);
            }
        }
        all
    }

    /// A preference the client changed shown on the section's row that keeps it (by its section
    /// and key); whether one did.
    // D2-03180
    pub fn refresh_from(&mut self, ui: &mut Ui, section: usize, change: &Change) -> bool {
        let id = match change {
            Change::Flag(p) => p.description.identify(),
            Change::Number(p) => p.description.identify(),
            Change::Choice(p) => p.description.identify(),
        };
        if id.1 == 0 {
            return false;
        }
        for r in self.section_rows(section) {
            let Some(row) = self.all.get_mut(r) else {
                continue;
            };
            if row.description().is_some_and(|d| d.identify().1 == id.1) {
                row.refresh_from(ui, change);
                return true;
            }
        }
        false
    }

    /// The section opened: its rows in its list, its header lit, refitted.
    // D2-03182
    pub fn construct(&mut self, ui: &mut Ui, section: usize) {
        let Some((_, Some(list), header)) = self.parts(section) else {
            return;
        };
        for r in self.section_rows(section) {
            if let Some(e) = self.all.get(r).and_then(|r| r.element) {
                ui.list_multi_add(list, e);
            }
        }
        ui.list_changes_done(list);
        if let Some(h) = header {
            ui.set_state(h, state::HIGHLIGHT);
        }
        if let Some(Row {
            control: Control::Section { open, .. },
            ..
        }) = self.all.get_mut(section)
        {
            *open = true;
        }
        self.refit(ui, section);
    }

    /// The section closed: its list emptied, its header plain, refitted.
    // D2-03183
    pub fn dismantle(&mut self, ui: &mut Ui, section: usize) {
        let Some((_, Some(list), header)) = self.parts(section) else {
            return;
        };
        while !ui.list_items(list).is_empty() {
            ui.list_multi_remove(list, 0);
        }
        ui.list_changes_done(list);
        if let Some(h) = header {
            ui.set_state(h, state::NORMAL);
        }
        if let Some(Row {
            control: Control::Section { open, .. },
            ..
        }) = self.all.get_mut(section)
        {
            *open = false;
        }
        self.refit(ui, section);
    }

    /// Whether any of the section's rows changed.
    // D2-03184
    #[must_use]
    pub fn changed(&self, ui: &Ui, section: usize) -> bool {
        self.section_rows(section)
            .iter()
            .any(|r| self.all.get(*r).is_some_and(|r| r.changed(ui)))
    }

    /// Every row of the section applied.
    // D2-03185
    pub fn apply(&mut self, ui: &Ui, section: usize) {
        for r in self.section_rows(section) {
            if let Some(row) = self.all.get_mut(r) {
                row.apply(ui);
            }
        }
    }

    /// What each of the section's rows holds that its preference does not.
    #[must_use]
    pub fn changes(&self, ui: &Ui, section: usize) -> Vec<Change> {
        self.section_rows(section)
            .into_iter()
            .filter_map(|r| self.all.get(r))
            .filter(|r| r.changed(ui))
            .filter_map(|r| r.change(ui))
            .collect()
    }

    /// Every row of the section submitted.
    pub fn submit(&mut self, ui: &Ui, section: usize) {
        for r in self.section_rows(section) {
            if let Some(row) = self.all.get_mut(r) {
                row.submit(ui);
            }
        }
    }

    /// Every row of the section's tried value kept, or put back.
    pub fn settle(&mut self, section: usize, keep: bool) {
        for r in self.section_rows(section) {
            if let Some(row) = self.all.get_mut(r) {
                row.settle(keep);
            }
        }
    }

    /// Every row of the section recalled.
    // D2-03186
    pub fn recall(&mut self, ui: &mut Ui, section: usize) {
        for r in self.section_rows(section) {
            if let Some(row) = self.all.get_mut(r) {
                row.recall(ui);
            }
        }
    }

    /// Every row of the section to its default.
    // D2-03187
    pub fn to_default(&mut self, ui: &mut Ui, section: usize) {
        for r in self.section_rows(section) {
            if let Some(row) = self.all.get_mut(r) {
                row.to_default(ui);
            }
        }
    }

    /// The section's header pressed: the section opened when closed, else closed.
    // D2-03188
    pub fn toggle(&mut self, ui: &mut Ui, section: usize) {
        let open = matches!(
            self.all.get(section).map(|r| &r.control),
            Some(Control::Section { open: true, .. })
        );
        if open {
            self.dismantle(ui, section);
        } else {
            self.construct(ui, section);
        }
    }

    /// The section's rows gone, the section closed first.
    // D2-03191
    pub fn clear(&mut self, ui: &mut Ui, section: usize) {
        if self.parts(section).is_none_or(|p| p.1.is_none()) {
            return;
        }
        self.dismantle(ui, section);
        let rows = self.section_rows(section);
        if let Some(Row {
            control: Control::Section { rows: r, .. },
            ..
        }) = self.all.get_mut(section)
        {
            r.clear();
        }
        // The rows' places stay taken (others' places do not move); their elements go.
        for at in rows {
            if let Some(row) = self.all.get_mut(at) {
                if let Some(e) = row.element.take() {
                    ui.remove(e);
                }
            }
        }
    }
}
