//! The options window's client page: the client's own preferences (kept on the computer, not
//! with the character), in eight sections: rendering, graphics, camera, avatars, input,
//! effects, sound, other.
//!
//! The client lists each section's preferences when the page asks for them (and again whenever
//! they change); the page makes a row for each (a choice, a number or a yes-or-no one) and takes
//! the client's changes into the row that shows the same preference. What the player sets is
//! tried at once ([`Request::Change`]). **Apply** submits the rendering preferences (which take
//! effect with a question whether to keep them: no puts them back) and applies the others, then
//! keeps them in the preferences' file; **Recall** and **Default** (each asked first) put back
//! what was applied, or the defaults (then applied). Apply and Recall are ghosted while nothing
//! changed.

use auberean_ui::framework::FrameId;
use auberean_ui::token::{attr, msg};
use auberean_ui::{Handle, Listener, Mail, Token, Ui, Value};

use super::helper::{self, Change, Row, Rows};
use super::{page, preference};
use crate::gameplay::{self, dialog, Dialog};

mod name {
    use auberean_ui::Token;
    pub const PAGE: Token = Token::of("ClientPage");
    pub const TAB: Token = Token::of("ClientTab");
    pub const LIST: Token = Token::of("ClientList");
}

/// The client's preferences' sections.
pub mod section {
    pub const GRAPHICS: u32 = 0x4000_0004;
    pub const SOUND: u32 = 0x4000_0007;
    pub const CAMERA: u32 = 0x4000_0008;
    pub const EFFECTS: u32 = 0x4000_0009;
    pub const RENDERING: u32 = 0x4000_000B;
    pub const OTHER: u32 = 0x4000_000C;
    pub const AVATARS: u32 = 0x4000_000D;
    pub const INPUT: u32 = 0x4000_000E;
}

/// The string table the client's preference labels are in.
pub const PREFERENCE_STRINGS: u32 = 0x4000_0008;

/// The sections in the page's order: each one's section and its label.
const SECTIONS: [(u32, &str); 8] = [
    (section::RENDERING, "ID_Rendering_RenderingSection"),
    (section::GRAPHICS, "ID_Graphics_GraphicsSection"),
    (section::CAMERA, "ID_Camera_CameraSection"),
    (section::AVATARS, "ID_Avatar_AvatarSection"),
    (section::INPUT, "ID_Input_InputSection"),
    (section::EFFECTS, "ID_FX_FXSection"),
    (section::SOUND, "ID_Sound_SoundSection"),
    (section::OTHER, "ID_Misc_MiscSection"),
];

/// The sections applied (after the rendering's is submitted), in the client's order.
const APPLIED: [u32; 7] = [
    section::GRAPHICS,
    section::SOUND,
    section::EFFECTS,
    section::CAMERA,
    section::AVATARS,
    section::INPUT,
    section::OTHER,
];

/// The dialog kind of a question that takes itself down after a while unless answered.
pub const TIMED_CONFIRMATION: u32 = 0x4000_0005;

/// What the client page asks of the client's preferences.
#[derive(Clone, Debug, PartialEq)]
pub enum Request {
    /// The sections the page shows: list each one's preferences now and whenever they change.
    Attach(Vec<u32>),
    /// The page goes: no longer.
    Detach(Vec<u32>),
    /// Try a preference the player set.
    Change(Change),
    /// A section's preferences taken into effect (the rendering's submitted, to be kept or put
    /// back), applied, kept as they are now (synchronized) or put back (recalled).
    Submit(u32),
    Apply(u32),
    Synchronize(u32),
    Recall(u32),
    /// The preferences kept in the preferences' file.
    Save,
}

/// The client page.
#[derive(Debug)]
pub struct ClientPage {
    frame: FrameId,
    window: FrameId,
    page: Option<Handle>,
    rows: Rows,
    /// The sections' rows, in the page's order, each with its section.
    sections: Vec<(u32, usize)>,
    /// The questions up: keep the rendering, recall, default.
    context_render: u32,
    context_recall: u32,
    context_default: u32,
}

impl ClientPage {
    /// The page under the window's framework: its element found, its tab heard; its sections
    /// made; the client asked for their preferences.
    // D2-02862
    pub fn new(ui: &mut Ui, window: FrameId) -> Self {
        let frame = ui.new_child_frame(window);
        let page = ui.element(name::PAGE);
        if let Some(l) = ui.frame_listener(frame) {
            ui.listen_for_message(l, name::TAB, page::OPENED);
            ui.listen_for_message(l, name::TAB, page::CLOSED);
        }
        let mut me = Self {
            frame,
            window,
            page,
            rows: Rows::default(),
            sections: Vec::new(),
            context_render: 0,
            context_recall: 0,
            context_default: 0,
        };
        me.construct(ui);
        me
    }

    /// The page's framework.
    #[must_use]
    pub const fn frame(&self) -> FrameId {
        self.frame
    }

    /// The page's rows and its sections (each with its section).
    #[must_use]
    pub fn rows(&self) -> (&Rows, &[(u32, usize)]) {
        (&self.rows, &self.sections)
    }

    /// The sections made, each labelled and put in the page's list.
    // D2-02861
    fn construct(&mut self, ui: &mut Ui) {
        let Some(list) = self.page.and_then(|p| ui.child_recursive(p, name::LIST)) else {
            return;
        };
        for (s, label) in SECTIONS {
            let mut row = Row::section(ui, self.frame);
            row.describe(ui, PREFERENCE_STRINGS, Token::of(label).raw(), 0);
            if let Some(e) = row.element {
                ui.list_add(list, e);
            }
            let at = self.rows.all.len();
            self.rows.all.push(row);
            self.rows.dismantle(ui, at);
            self.sections.push((s, at));
        }
    }

    /// The client is to list the page's sections' preferences.
    #[must_use]
    pub fn attach(&self) -> gameplay::Request {
        gameplay::Request::Options(super::Request::Preferences(Request::Attach(
            self.sections.iter().map(|(s, _)| *s).collect(),
        )))
    }

    /// A section's row by its section.
    // D2-02850
    #[must_use]
    pub fn lookup_expander(&self, section: u32) -> Option<usize> {
        self.sections
            .iter()
            .find(|(s, _)| *s == section)
            .map(|(_, r)| *r)
    }

    /// A preference the client changed shown on the row that shows it.
    // D2-02848
    pub fn monitor_change(&mut self, ui: &mut Ui, change: &Change) {
        let d = match change {
            Change::Flag(p) => p.description,
            Change::Number(p) => p.description,
            Change::Choice(p) => p.description,
        };
        if let Some(s) = self.lookup_expander(d.section) {
            self.rows.refresh_from(ui, s, change);
        }
    }

    /// A section's preferences listed: its rows made again, one for each (a choice, a number or a
    /// yes-or-no one), each taking its preference in; every section refitted.
    // D2-02857
    pub fn monitor_reload(&mut self, ui: &mut Ui, section: u32, prefs: &[Change]) {
        let Some(s) = self.lookup_expander(section) else {
            return;
        };
        let open = matches!(
            self.rows.all.get(s).map(|r| &r.control),
            Some(helper::Control::Section { open: true, .. })
        );
        self.rows.clear(ui, s);
        for p in prefs {
            let mut row = match p {
                Change::Choice(_) => Row::choice(ui, self.frame),
                Change::Number(_) => Row::number(ui, self.frame),
                Change::Flag(_) => Row::flag(ui, self.frame),
            };
            row.merge(ui, p);
            let at = self.rows.all.len();
            self.rows.all.push(row);
            self.rows.add_to_section(s, at);
        }
        if open {
            self.rows.construct(ui, s);
        }
        for (_, r) in self.sections.clone() {
            self.rows.refit(ui, r);
        }
        if let Some(list) = self.page.and_then(|p| ui.child_recursive(p, name::LIST)) {
            ui.list_update_and_draw(list);
        }
        self.update_buttons(ui);
    }

    /// Whether any section's preferences changed.
    // D2-02849
    #[must_use]
    pub fn changed(&self, ui: &Ui) -> bool {
        self.sections.iter().any(|(_, r)| self.rows.changed(ui, *r))
    }

    /// Apply and Recall ghosted unless a preference changed.
    // D2-02852
    pub fn update_buttons(&self, ui: &mut Ui) {
        let Some(p) = self.page else { return };
        let ghosted = !self.changed(ui);
        for b in [super::name::APPLY_BUTTON, super::name::RECALL_BUTTON] {
            let Some(b) = ui.child_recursive(p, b) else {
                return;
            };
            ui.set_attr(b, attr::GHOSTED, Value::Bool(ghosted));
        }
    }

    /// Every section applied: the rendering submitted, the others applied; with the rendering
    /// changed, the question whether to keep it, else the preferences kept in their file.
    // D2-02854
    pub fn apply(&mut self, ui: &mut Ui, out: &mut Vec<gameplay::Request>) {
        let mut rendering = false;
        for (s, r) in self.sections.clone() {
            if s == section::RENDERING {
                rendering = self.rows.changed(ui, r);
                // A row left as it was set when its value was put back is tried again.
                for c in self.rows.changes(ui, r) {
                    out.push(gameplay::Request::Options(super::Request::Preferences(
                        Request::Change(c),
                    )));
                }
                self.rows.submit(ui, r);
            } else {
                self.rows.apply(ui, r);
            }
        }
        let mut ask = |r| out.push(gameplay::Request::Options(super::Request::Preferences(r)));
        ask(Request::Submit(section::RENDERING));
        for s in APPLIED {
            ask(Request::Apply(s));
        }
        if rendering {
            self.verify_render_options(ui, out);
        } else {
            out.push(gameplay::Request::Options(super::Request::Preferences(
                Request::Save,
            )));
        }
        self.update_buttons(ui);
    }

    /// Every section back to what was applied.
    // D2-02855
    pub fn recall(&mut self, ui: &mut Ui) {
        for (_, r) in self.sections.clone() {
            self.rows.recall(ui, r);
        }
        self.update_buttons(ui);
    }

    /// Every section to its defaults, then applied.
    // D2-02856
    pub fn default(&mut self, ui: &mut Ui, out: &mut Vec<gameplay::Request>) {
        for (s, r) in self.sections.clone() {
            self.rows.to_default(ui, r);
            // The rendering's are told as it is applied.
            if s != section::RENDERING {
                for c in self.rows.changes(ui, r) {
                    out.push(gameplay::Request::Options(super::Request::Preferences(
                        Request::Change(c),
                    )));
                }
            }
        }
        self.apply(ui, out);
        self.update_buttons(ui);
    }

    fn question(ui: &Ui, kind: u32, text: &str, out: &mut Vec<gameplay::Request>) -> u32 {
        let id = super::new_dialog_id();
        out.push(gameplay::Request::Dialog(Dialog {
            id,
            kind,
            text: crate::flow::message(ui, helper::OPTION_STRINGS, text),
            modal: false,
            info: None,
        }));
        id
    }

    /// Ask before recalling (unless asked already).
    // D2-02844
    pub fn confirm_recall(&mut self, ui: &Ui, out: &mut Vec<gameplay::Request>) {
        if self.context_recall == 0 {
            self.context_recall =
                Self::question(ui, dialog::CONFIRMATION, "ID_Option_RecallText", out);
        }
    }

    /// Ask before the defaults (unless asked already).
    // D2-02845
    pub fn confirm_default(&mut self, ui: &Ui, out: &mut Vec<gameplay::Request>) {
        if self.context_default == 0 {
            self.context_default =
                Self::question(ui, dialog::CONFIRMATION, "ID_Option_DefaultText", out);
        }
    }

    /// Ask whether to keep the rendering as it now is (unless asked already).
    // D2-02847
    pub fn verify_render_options(&mut self, ui: &Ui, out: &mut Vec<gameplay::Request>) {
        if self.context_render == 0 {
            let id = super::new_dialog_id();
            let table = ui
                .assets()
                .data_id_by_enum(helper::OPTION_STRINGS, crate::flow::TABLE_MAPPING);
            out.push(gameplay::Request::Dialog(Dialog {
                id,
                kind: TIMED_CONFIRMATION,
                text: String::new(),
                modal: false,
                info: Some(auberean_text::StringInfo::entry(
                    table,
                    Token::of("ID_Option_RenderText").raw(),
                )),
            }));
            self.context_render = id;
        }
    }

    /// The questions up taken down.
    // D2-02846
    pub fn clear_confirmations(&mut self, out: &mut Vec<gameplay::Request>) {
        for c in [
            &mut self.context_render,
            &mut self.context_recall,
            &mut self.context_default,
        ] {
            if *c != 0 {
                out.push(gameplay::Request::DialogDown(*c));
                *c = 0;
            }
        }
    }

    /// A question answered: the rendering kept (yes) or put back (no) with its rows left as they
    /// are, so put back they differ from it and the page can apply them again; the preferences kept
    /// in their file; yes recalls, or puts the defaults in; the questions forgotten.
    // D2-02858
    pub fn close_confirmation_dialog(
        &mut self,
        ui: &mut Ui,
        id: u32,
        yes: bool,
        out: &mut Vec<gameplay::Request>,
    ) {
        let mut ask = |r| out.push(gameplay::Request::Options(super::Request::Preferences(r)));
        if id == self.context_render && id != 0 {
            ask(if yes {
                Request::Synchronize(section::RENDERING)
            } else {
                Request::Recall(section::RENDERING)
            });
            ask(Request::Save);
            if let Some(s) = self.lookup_expander(section::RENDERING) {
                self.rows.settle(s, yes);
            }
            self.update_buttons(ui);
        }
        if yes {
            if id == self.context_recall {
                self.recall(ui);
            }
            if id == self.context_default {
                self.default(ui, out);
            }
        }
        for c in [
            &mut self.context_render,
            &mut self.context_recall,
            &mut self.context_default,
        ] {
            if id == *c {
                *c = 0;
            }
        }
        self.clear_confirmations(out);
    }

    /// What the page heard: its tab (as the character page's); while the window shows, its
    /// buttons and anything its rows said (a preference the player set tried at once).
    // D2-02859
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
                    self.apply(ui, out);
                }
                if element == super::name::RECALL_BUTTON {
                    self.confirm_recall(ui, out);
                }
                if element == super::name::DEFAULT_BUTTON {
                    self.confirm_default(ui, out);
                }
            } else {
                if what == helper::name::EXPANDER_TOGGLE {
                    let Some(list) = self.page.and_then(|p| ui.child_recursive(p, name::LIST))
                    else {
                        return;
                    };
                    ui.list_update_and_draw(list);
                }
                self.update_buttons(ui);
            }
        }
    }

    /// The page's own mail and its rows': a preference the player set tried at once, then the
    /// row's message handed on.
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
                if let Some(c) = h.change {
                    out.push(gameplay::Request::Options(super::Request::Preferences(
                        Request::Change(c),
                    )));
                }
                self.listen(ui, h.element, h.what, out);
            }
        }
    }

    /// The page goes: its questions, its list emptied, the client no longer asked, its rows.
    // D2-02853, D2-02860, D2-02851
    pub fn close(mut self, ui: &mut Ui, out: &mut Vec<gameplay::Request>) {
        self.clear_confirmations(out);
        if let Some(list) = self.page.and_then(|p| ui.child_recursive(p, name::LIST)) {
            ui.list_flush(list);
        }
        out.push(gameplay::Request::Options(super::Request::Preferences(
            Request::Detach(self.sections.iter().map(|(s, _)| *s).collect()),
        )));
        for r in self.rows.all.drain(..) {
            r.close(ui);
        }
        ui.drop_frame(self.frame);
    }
}

/// The client's preferences as a host lists them for the page: each section's, in its order, and
/// a count that grows with every listing (the page takes a listing in once).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Listing {
    pub serial: u32,
    pub sections: Vec<(u32, Vec<Change>)>,
}

/// A yes-or-no preference of the client's (for a host listing its preferences).
#[must_use]
pub fn flag(description: preference::Description, value: bool, default: bool) -> Change {
    let mut p = preference::Flag::new();
    p.set_default(default);
    p.set_value(value);
    p.description = description;
    Change::Flag(p)
}
