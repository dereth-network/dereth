//! The options window's character page: the player's own options, kept with the player's options
//! on the server, in four sections.
//!
//! - **Combat**: target the next foe on its own, damage numbers, others' damage numbers,
//!   player-killer damage, how far away others' damage shows.
//! - **The radar's filter**: monsters, people, players, portals, lifestones, items, the map
//!   coordinates.
//! - **Social**: take allegiance, fellowship and trade offers.
//! - **Other**: the shortcut bar's numbers, its lock, its instant scrolling, the question before
//!   melting an item into money, the latency graph, fellows on the map, the pet's panel, the
//!   classic examination window, the shortcut bar's rows (1 to 5).
//!
//! Each section opens and closes by its header. The page shows the player's options as the
//! scripts read them (again whenever they are read again). **Apply** sets the changed ones, in
//! the page's order, tells the other panels the options changed and sends the options to the
//! server to keep; **Recall** (asked first) puts back what was applied; **Default** (asked
//! first) puts the defaults in and applies them. Apply and Recall are ghosted while nothing
//! changed. The shortcut bar resized by hand sets the rows (and sends the options) at once.

use auberean_ui::framework::FrameId;
use auberean_ui::token::{attr, msg};
use auberean_ui::{Handle, Listener, Mail, Token, Ui, Value};

use super::helper::{self, Row, Rows, OPTION_STRINGS};
use super::{page, Request, Values};
use crate::gameplay::{self, dialog, Dialog};

mod name {
    use auberean_ui::Token;
    pub const PAGE: Token = Token::of("CharacterPage");
    pub const TAB: Token = Token::of("CharacterTab");
    pub const LIST: Token = Token::of("CharacterList");
}

/// The label of each option, by its string's name.
pub mod label {
    use auberean_ui::Token;
    const fn id(s: &str) -> u32 {
        Token::of(s).raw()
    }
    pub const AUTO_TARGET: u32 = id("ID_Option_AutoTargetText");
    pub const DAMAGE_NUMBERS: u32 = id("ID_Option_DamageNumbersText");
    pub const OTHERS_DAMAGE: u32 = id("ID_Option_ThirdPartyDamageText");
    pub const PK_DAMAGE: u32 = id("ID_Option_ShowPKDamage");
    pub const OTHERS_DAMAGE_RANGE: u32 = id("ID_Option_ThirdPartyDamageRange");
    pub const RADAR_MONSTERS: u32 = id("ID_Option_RadarShowMonsters");
    pub const RADAR_NPCS: u32 = id("ID_Option_RadarShowNPCs");
    pub const RADAR_PCS: u32 = id("ID_Option_RadarShowPCs");
    pub const RADAR_PORTALS: u32 = id("ID_Option_RadarShowPortals");
    pub const RADAR_LIFESTONES: u32 = id("ID_Option_RadarShowLifestones");
    pub const RADAR_ITEMS: u32 = id("ID_Option_RadarShowItems");
    pub const RADAR_COORDINATES: u32 = id("ID_Option_RadarShowCoordinates");
    pub const ALLEGIANCE: u32 = id("ID_Option_AcceptAllegianceText");
    pub const FELLOWSHIP: u32 = id("ID_Option_AcceptFellowshipText");
    pub const TRADE: u32 = id("ID_Option_AcceptTradeText");
    pub const SHORTCUT_NUMBERS: u32 = id("ID_Option_ShortcutNumbersText");
    pub const SHORTCUT_LOCKED: u32 = id("ID_Option_ShortcutLocked");
    pub const INSTANT_SCROLL: u32 = id("ID_Option_ShortcutInstaScroll");
    pub const GOLD_MELTING: u32 = id("ID_Option_GoldMeltingConfirmation");
    pub const LATENCY_GRAPH: u32 = id("ID_Option_LatencyIndicatorGraph");
    pub const FELLOWS_ON_MAP: u32 = id("ID_Option_ShowFellowsOnMap");
    pub const PET_PANEL: u32 = id("ID_Option_ShowPetUI");
    pub const CLASSIC_EXAMINATION: u32 = id("ID_Option_ClassicExamination");
    pub const SHORTCUT_HEIGHT: u32 = id("ID_Option_ShortcutHeight");
}

/// The player's yes-or-no options: their label, their bit in the player's options (a radar
/// filter's in the radar's mask), and whether the option is the bit's opposite.
const FLAGS: &[(u32, Bit)] = &[
    (label::AUTO_TARGET, Bit::Options(0x20)),
    (label::ALLEGIANCE, Bit::Options(0x40)),
    (label::FELLOWSHIP, Bit::Options(0x80)),
    (label::TRADE, Bit::Options(0x100)),
    (label::DAMAGE_NUMBERS, Bit::Options(0x4)),
    (label::OTHERS_DAMAGE, Bit::Options(0x8)),
    (label::PK_DAMAGE, Bit::Options(0x1000)),
    (label::SHORTCUT_NUMBERS, Bit::Options(0x1)),
    (label::SHORTCUT_LOCKED, Bit::Options(0x800)),
    (label::INSTANT_SCROLL, Bit::Options(0x2000)),
    (label::GOLD_MELTING, Bit::Opposite(0x400)),
    (label::LATENCY_GRAPH, Bit::Options(super::LATENCY_GRAPH)),
    (label::FELLOWS_ON_MAP, Bit::Opposite(0x8000)),
    (label::PET_PANEL, Bit::Opposite(0x1_0000)),
    (label::CLASSIC_EXAMINATION, Bit::Options(0x2_0000)),
    (label::RADAR_MONSTERS, Bit::Radar(0x1)),
    (label::RADAR_NPCS, Bit::Radar(0x2)),
    (label::RADAR_PCS, Bit::Radar(0x4)),
    (label::RADAR_PORTALS, Bit::Radar(0x8)),
    (label::RADAR_LIFESTONES, Bit::Radar(0x10)),
    (label::RADAR_ITEMS, Bit::Radar(0x20)),
    (label::RADAR_COORDINATES, Bit::Options(0x10)),
];

/// Where a yes-or-no option is kept.
#[derive(Clone, Copy, Debug)]
enum Bit {
    Options(u32),
    /// The bit is set when the option is off.
    Opposite(u32),
    Radar(u32),
}

/// The character page.
#[derive(Debug)]
pub struct CharacterPage {
    frame: FrameId,
    /// The window's framework (the page acts on its buttons only while the window shows).
    window: FrameId,
    page: Option<Handle>,
    rows: Rows,
    /// The sections, in the page's order.
    main: Vec<usize>,
    /// The questions up: apply (never asked), recall, default.
    context_apply: u32,
    context_recall: u32,
    context_default: u32,
    /// The player's options last shown, and how often the client had read them then.
    shown: Option<Values>,
}

impl CharacterPage {
    /// The page under the window's framework: its element found, its tab heard; its sections and
    /// options made; shown in the page's list.
    // D2-02843
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
            main: Vec::new(),
            context_apply: 0,
            context_recall: 0,
            context_default: 0,
            shown: None,
        };
        me.setup(ui);
        me.build_list(ui);
        me
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

    /// The row of an option, by its label.
    #[must_use]
    pub fn row(&self, label: u32) -> Option<&Row> {
        self.rows.all.iter().find(|r| r.label == label)
    }

    /// An option's place among the rows, by its label.
    #[must_use]
    pub fn place(&self, label: u32) -> Option<usize> {
        self.rows.all.iter().position(|r| r.label == label)
    }

    /// A row added: to a section's rows (or, with none, among the page's sections), and to every
    /// row the page keeps.
    // D2-02841
    fn add_option(&mut self, row: Row, section: Option<usize>) -> usize {
        let at = self.rows.all.len();
        self.rows.all.push(row);
        match section {
            Some(s) => self.rows.add_to_section(s, at),
            None => self.main.push(at),
        }
        at
    }

    /// A section made with its label, closed, among the page's sections.
    fn section(&mut self, ui: &mut Ui, label: &str) -> usize {
        let mut s = Row::section(ui, self.frame);
        s.describe_by_label(ui, Token::of(label).raw());
        let at = self.add_option(s, None);
        self.rows.dismantle(ui, at);
        at
    }

    /// A yes-or-no option made in a section with its default and its label and help.
    fn flag(&mut self, ui: &mut Ui, section: usize, default: bool, label: &str) {
        let mut r = Row::flag(ui, self.frame);
        r.set_flag_default(ui, default);
        let help = format!("{label}_Help");
        r.describe(
            ui,
            OPTION_STRINGS,
            Token::of(label).raw(),
            Token::of(&help).raw(),
        );
        self.add_option(r, Some(section));
    }

    /// The sections and their options, with their defaults, in the page's order.
    // D2-02842
    fn setup(&mut self, ui: &mut Ui) {
        let combat = self.section(ui, "ID_Options_Combat");
        self.flag(ui, combat, true, "ID_Option_AutoTargetText");
        self.flag(ui, combat, true, "ID_Option_DamageNumbersText");
        self.flag(ui, combat, false, "ID_Option_ThirdPartyDamageText");
        self.flag(ui, combat, true, "ID_Option_ShowPKDamage");
        let mut range = Row::number(ui, self.frame);
        range.set_range(0.0, 1.0);
        range.set_number_default(ui, 1.0);
        range.describe(
            ui,
            OPTION_STRINGS,
            label::OTHERS_DAMAGE_RANGE,
            Token::of("ID_Option_ThirdPartyDamageRange_Help").raw(),
        );
        self.add_option(range, Some(combat));
        let radar = self.section(ui, "ID_Options_RadarFilter");
        self.flag(ui, radar, true, "ID_Option_RadarShowMonsters");
        self.flag(ui, radar, true, "ID_Option_RadarShowNPCs");
        self.flag(ui, radar, true, "ID_Option_RadarShowPCs");
        self.flag(ui, radar, true, "ID_Option_RadarShowPortals");
        self.flag(ui, radar, true, "ID_Option_RadarShowLifestones");
        self.flag(ui, radar, false, "ID_Option_RadarShowItems");
        self.flag(ui, radar, true, "ID_Option_RadarShowCoordinates");
        let social = self.section(ui, "ID_Options_Social");
        self.flag(ui, social, true, "ID_Option_AcceptAllegianceText");
        self.flag(ui, social, true, "ID_Option_AcceptFellowshipText");
        self.flag(ui, social, true, "ID_Option_AcceptTradeText");
        let misc = self.section(ui, "ID_Options_MiscGameplay");
        self.flag(ui, misc, true, "ID_Option_ShortcutNumbersText");
        self.flag(ui, misc, false, "ID_Option_ShortcutLocked");
        self.flag(ui, misc, false, "ID_Option_ShortcutInstaScroll");
        self.flag(ui, misc, true, "ID_Option_GoldMeltingConfirmation");
        self.flag(ui, misc, false, "ID_Option_LatencyIndicatorGraph");
        self.flag(ui, misc, true, "ID_Option_ShowFellowsOnMap");
        self.flag(ui, misc, true, "ID_Option_ShowPetUI");
        self.flag(ui, misc, false, "ID_Option_ClassicExamination");
        let mut height = Row::choice(ui, self.frame);
        for n in 1..=5 {
            let l = Token::of(&format!("ID_Option_ShortcutHeight{n}")).raw();
            height.assign_choice(ui, n, OPTION_STRINGS, l);
        }
        height.set_choice_default(ui, 1);
        height.describe(
            ui,
            OPTION_STRINGS,
            label::SHORTCUT_HEIGHT,
            Token::of("ID_Option_ShortcutHeight_Help").raw(),
        );
        self.add_option(height, Some(misc));
    }

    /// The sections in the page's list.
    // D2-02833
    fn build_list(&self, ui: &mut Ui) {
        let Some(list) = self.page.and_then(|p| ui.child_recursive(p, name::LIST)) else {
            return;
        };
        for s in &self.main {
            if let Some(e) = self.rows.all.get(*s).and_then(|r| r.element) {
                ui.list_add(list, e);
            }
        }
    }

    /// A yes-or-no option's box ticked as the player's options say.
    // D2-02824
    fn refresh_flag(ui: &mut Ui, row: &mut Row, v: &Values) {
        let Some(&(_, bit)) = FLAGS.iter().find(|(l, _)| *l == row.label) else {
            return;
        };
        let on = match bit {
            Bit::Options(b) => v.bits & b != 0,
            Bit::Opposite(b) => v.bits & b == 0,
            Bit::Radar(b) => v.radar_mask & b != 0,
        };
        row.set_flag(ui, on);
    }

    /// Every option shown as the player's options say, and applied (nothing changed).
    // D2-02829
    pub fn refresh(&mut self, ui: &mut Ui, v: &Values) {
        for at in 0..self.rows.all.len() {
            let row = &mut self.rows.all[at];
            if row.is_flag() {
                Self::refresh_flag(ui, row, v);
            }
            if row.is_number() && row.label == label::OTHERS_DAMAGE_RANGE {
                row.set_number(ui, v.damage_range);
            }
            if row.is_choice() && row.label == label::SHORTCUT_HEIGHT {
                row.set_choice(ui, i32::try_from(v.shortcut_height).unwrap_or(0));
            }
            if matches!(row.control, helper::Control::Section { .. }) {
                self.rows.apply(ui, at);
            } else {
                self.rows.all[at].apply(ui);
            }
        }
        self.shown = Some(*v);
    }

    /// Whether any option changed.
    // D2-02830
    #[must_use]
    pub fn changed(&self, ui: &Ui) -> bool {
        self.main.iter().any(|s| self.rows.changed(ui, *s))
    }

    /// The bar's rows set (the bar resized by hand): kept and sent at once; the rows' choice
    /// shows them, applied.
    // D2-02831
    pub fn set_shortcut_height(
        &mut self,
        ui: &mut Ui,
        rows: i32,
        out: &mut Vec<gameplay::Request>,
    ) {
        out.push(gameplay::Request::Options(Request::Set {
            changes: vec![(label::SHORTCUT_HEIGHT, u32::try_from(rows).unwrap_or(0))],
            tell: false,
        }));
        for r in &mut self.rows.all {
            if r.label == label::SHORTCUT_HEIGHT && r.is_choice() {
                r.set_choice(ui, rows);
                r.apply(ui);
            }
        }
    }

    /// Apply and Recall ghosted unless an option changed.
    // D2-02834
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

    /// The changed options set, in the page's order, each then applied; the other panels told;
    /// the options sent to keep.
    // D2-02836
    pub fn apply(&mut self, ui: &mut Ui, out: &mut Vec<gameplay::Request>) {
        let mut changes = Vec::new();
        for r in &mut self.rows.all {
            if !r.changed(ui) {
                continue;
            }
            if r.is_flag() {
                changes.push((r.label, u32::from(r.flag_now(ui))));
                r.apply(ui);
            }
            if r.is_number() {
                if r.label == label::OTHERS_DAMAGE_RANGE {
                    changes.push((r.label, r.number_now(ui).to_bits()));
                }
                r.apply(ui);
            }
            if r.is_choice() {
                if r.label == label::SHORTCUT_HEIGHT {
                    changes.push((r.label, u32::try_from(r.choice_now(ui)).unwrap_or(0)));
                }
                r.apply(ui);
            }
        }
        out.push(gameplay::Request::Options(Request::Set {
            changes,
            tell: true,
        }));
        self.update_buttons(ui);
    }

    /// Every section's options back to what was applied.
    // D2-02837
    pub fn recall(&mut self, ui: &mut Ui) {
        for s in self.main.clone() {
            self.rows.recall(ui, s);
        }
        self.update_buttons(ui);
    }

    /// Every option to its default, then applied.
    // D2-02838
    pub fn default(&mut self, ui: &mut Ui, out: &mut Vec<gameplay::Request>) {
        for s in self.main.clone() {
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

    /// Ask before recalling (unless asked already).
    // D2-02821
    pub fn confirm_recall(&mut self, ui: &Ui, out: &mut Vec<gameplay::Request>) {
        if self.context_recall == 0 {
            self.context_recall = Self::question("ID_Option_RecallText", ui, out);
        }
    }

    /// Ask before the defaults (unless asked already).
    // D2-02822
    pub fn confirm_default(&mut self, ui: &Ui, out: &mut Vec<gameplay::Request>) {
        if self.context_default == 0 {
            self.context_default = Self::question("ID_Option_DefaultText", ui, out);
        }
    }

    /// The questions up taken down.
    // D2-02823
    pub fn clear_confirmations(&mut self, out: &mut Vec<gameplay::Request>) {
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

    /// A question answered: yes recalls or puts the defaults in; the questions forgotten.
    // D2-02839
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
    /// or closed (its buttons no longer heard); while the window shows, its buttons (apply,
    /// recall, default: the last two asked first) and anything its options said (a section
    /// opened or closed lays the list out again; the buttons' ghosting).
    // D2-02840
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

    /// The player's options as the client last read them: shown again when read again (the
    /// scripts read them again, the radar's coordinates were shown or hidden, the bar's rows set).
    // D2-02832, D2-00158
    pub fn update(&mut self, ui: &mut Ui, v: Option<&Values>) {
        let Some(v) = v else { return };
        if self.shown.is_none_or(|s| s.read != v.read) {
            self.refresh(ui, v);
            self.update_buttons(ui);
        }
    }

    /// The page goes: its questions, its list emptied, its rows.
    // D2-02828, D2-02835
    pub fn close(mut self, ui: &mut Ui, out: &mut Vec<gameplay::Request>) {
        self.clear_confirmations(out);
        if let Some(list) = self.page.and_then(|p| ui.child_recursive(p, name::LIST)) {
            ui.list_flush(list);
        }
        for r in self.rows.all.drain(..) {
            r.close(ui);
        }
        ui.drop_frame(self.frame);
    }
}
