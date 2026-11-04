//! The status bar (top left): the indicators, the vitals, the effects.
//!
//! [`StatusBar`] makes the bar and its three panels, each a child framework:
//!
//! - **The vitals** ([`Vitals`]): health, vigor, focus and experience bars, each with its numbers
//!   ("current / greatest"); the player-killer damage and vigor loss behind health and vigor. The
//!   player's scripts say each vital and whether the player is a hero (only a hero's focus shows
//!   numbers; anyone else's shows the bar's ornament, full). A click on a bar hides or shows its
//!   numbers, and the bar's tooltip says which a click would do.
//! - **The effects** ([`Effects`]): one icon per effect on the player, each with its name as its
//!   tooltip and a bar under it running out with the effect's time (fuller, then lower, colors as it
//!   nears its end); a pulsed effect flashes when a pulse was missed. A click on one selects it and
//!   opens the examination panel on it.
//! - **The indicators** ([`Indicators`]): the connection's health (by how long since the server
//!   was last heard), the kind of player conflict the place allows, and the vitae (the tooltip gives
//!   the vitae and the experience to work it off; a click examines it).
//!
//! Shown or hidden, the bar says how much of the screen's top it covers.

use std::any::Any;

use auberean_ui::framework::FrameId;
use auberean_ui::token::{msg, state};
use auberean_ui::types::button_id;
use auberean_ui::{Handle, Mail, Token, Ui, Value};

use crate::flow;
use crate::gameplay::{self, Child, Examination, Notice, Request, View};

/// The status bar's layout's enumeration value.
pub const STATUS_LAYOUT_ENUM: u32 = 0x4100_000F;

mod name {
    use auberean_ui::Token;
    pub const STATUSBAR_FIELD: Token = Token::of("StatusbarField");
    pub const HEALTH_FIELD: Token = Token::of("HealthField");
    pub const HEALTH_TEXT: Token = Token::of("HealthText");
    pub const HEALTH_METER: Token = Token::of("HealthMeter");
    pub const PK_TEXT: Token = Token::of("PKText");
    pub const PK_METER: Token = Token::of("PKMeter");
    pub const VIGOR_FIELD: Token = Token::of("VigorField");
    pub const VIGOR_TEXT: Token = Token::of("VigorText");
    pub const VIGOR_METER: Token = Token::of("VigorMeter");
    pub const PK_VIGORLOSS_TEXT: Token = Token::of("PKVigorlossText");
    pub const PK_VIGORLOSS_METER: Token = Token::of("PKVigorlossMeter");
    pub const FOCUS_FIELD: Token = Token::of("FocusField");
    pub const FOCUS_TEXT: Token = Token::of("FocusText");
    pub const FOCUS_METER: Token = Token::of("FocusMeter");
    pub const EXPERIENCE_FIELD: Token = Token::of("ExperienceField");
    pub const EXPERIENCE_TEXT: Token = Token::of("ExperienceText");
    pub const EXPERIENCE_METER: Token = Token::of("ExperienceMeter");
    pub const HERO: Token = Token::of("hero");
    pub const NOT_HERO: Token = Token::of("not_hero");
    pub const RIGHT_FILL: Token = Token::of("right_fill");
    pub const LEFT_FILL: Token = Token::of("left_fill");
    pub const GOAL_POSITION: Token = Token::of("goal_position");
    pub const POSITION: Token = Token::of("position");
    pub const SMOOTH_MOVEMENT_DURATION: Token = Token::of("smooth_movement_duration");
    pub const PRECLUDE_HEIGHT: Token = Token::of("precludeheight");
    pub const CURRENT: Token = Token::of("current");
    pub const MAX: Token = Token::of("max");
    pub const VALUE: Token = Token::of("value");
    /// The effects' list.
    pub const EFFECT_LIST_BOX: Token = Token(0x24AB_A0BB);
    pub const EFFECT_TEMPLATE: Token = Token::of("EffectTemplate");
    pub const EFFECT_TIME_METER: Token = Token::of("EffectTimeMeter");
    pub const ICON_FIELD: Token = Token::of("IconField");
    pub const EFFECT_ID: Token = Token::of("EffectID");
    pub const EFFECT_TIMEOUT: Token = Token::of("EffectTimeout");
    pub const EXPIRATION: Token = Token::of("Expiration");
    pub const SELECTED: Token = Token::of("Selected");
    pub const ALERT: Token = Token::of("Alert");
    pub const STATE_SELECTED: Token = Token::of("selected");
    pub const STATE_SELECTED_ALERT: Token = Token::of("selected_alert");
    pub const STATE_UNSELECTED: Token = Token::of("unselected");
    pub const STATE_UNSELECTED_ALERT: Token = Token::of("unselected_alert");
    pub const ACTIVE: Token = Token::of("active");
    pub const PULSED: Token = Token::of("pulsed");
    pub const FULL: Token = Token::of("full");
    pub const MID: Token = Token::of("mid");
    pub const LOW: Token = Token::of("low");
    pub const NET_STATUS_FIELD: Token = Token::of("NetStatusField");
    pub const PK_STATUS_FIELD: Token = Token::of("PKStatusField");
    pub const VITAE_STATUS_FIELD: Token = Token::of("VitaeStatusField");
    pub const OVERLAY: Token = Token::of("Overlay");
    pub const NOT_FULL: Token = Token::of("notfull");
    pub const GOOD: Token = Token::of("good");
    pub const BAD: Token = Token::of("bad");
    pub const UGLY: Token = Token::of("ugly");
    pub const LATENCY: Token = Token::of("latency");
    pub const LOSS: Token = Token::of("loss");
    pub const BYTES_IN: Token = Token::of("BytesIn");
    pub const BYTES_OUT: Token = Token::of("BytesOut");
    pub const BORDER_PIXELS: Token = Token::of("border_pixels");
    pub const R_BACK: Token = Token::of("r_back");
    pub const G_BACK: Token = Token::of("g_back");
    pub const B_BACK: Token = Token::of("b_back");
    pub const A_BACK: Token = Token::of("a_back");
    pub const R_SENT: Token = Token::of("r_sent");
    pub const G_SENT: Token = Token::of("g_sent");
    pub const B_SENT: Token = Token::of("b_sent");
    pub const A_SENT: Token = Token::of("a_sent");
    pub const R_RECV: Token = Token::of("r_recv");
    pub const G_RECV: Token = Token::of("g_recv");
    pub const B_RECV: Token = Token::of("b_recv");
    pub const A_RECV: Token = Token::of("a_recv");
    pub const CUR: Token = Token::of("cur");
    pub const INC: Token = Token::of("inc");
    pub const XP: Token = Token::of("xp");
}

/// The player's scripts' notices the status bar hears, by the native that sends each.
pub mod notice {
    /// A vital: which, its current value, its greatest (each 64 bits).
    pub const UPDATE_VITAL: u32 = 0x0085 << 16 | 0x0001;
    /// The effects on the player changed.
    pub const UPDATE_EFFECT: u32 = 0x0085 << 16 | 0x0002;
    /// The kind of player conflict the place allows (1 to 5).
    pub const UPDATE_FACTION_STATUS: u32 = 0x0085 << 16 | 0x000D;
    /// The vitae: the share of health kept, the share gained, the experience to work it off.
    pub const UPDATE_VITAE: u32 = 0x0085 << 16 | 0x000E;
    /// Whether the player is a hero, and the focus' current value and greatest.
    pub const UPDATE_HERO_STATUS: u32 = 0x0085 << 16 | 0x0012;
    /// A pulsed effect missed its pulse (the effect).
    pub const PULSE_MISSED: u32 = 0x0085 << 16 | 0x0013;
}

/// The vitals, by the number the scripts give each.
pub mod vital {
    pub const HEALTH: u32 = 0x4000_0001;
    pub const VIGOR: u32 = 0x4000_0002;
    pub const EXPERIENCE: u32 = 0x4000_0003;
    pub const FOCUS: u32 = 0x4000_0004;
    pub const PK_DAMAGE: u32 = 0x4000_0005;
    pub const PK_VIGORLOSS: u32 = 0x4000_0006;
}

/// What the examination panel is asked to show for an effect, and for the vitae.
const EXAMINE_EFFECT: u32 = 0x4000_0001;
const EXAMINE_VITAE: u32 = 0x4000_000A;

/// How the connection is doing, as the network indicator shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetQuality {
    Good,
    Bad,
    Ugly,
}

/// What kind of player conflict the place allows, as the conflict indicator shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConflictArea {
    Peace,
    Control,
    Factional,
    FreeForAll,
    FactionalAnywhere,
}

/// One vital: current and greatest.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Vital {
    pub current: u32,
    pub max: u32,
}

/// The connection's health as the client measures it every few seconds: the round trip and the
/// share of packets lost (seconds, a share), the bytes received and sent, the time the counts span,
/// and how long since the server was last heard (seconds); `beat` numbers the measures.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Link {
    pub beat: u64,
    pub latency: f64,
    pub loss: f64,
    pub bytes_in: u32,
    pub bytes_out: u32,
    pub span: f32,
    pub since_heard: f32,
}

/// One effect on the player, as the scripts list them: its number, name and icon, when it was cast
/// and how long it lasts (-1 for no limit), and whether it pulses.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EffectNode {
    pub eid: u32,
    pub name: String,
    pub icon: u32,
    pub time_cast: f64,
    pub duration: f64,
    pub pulsed: bool,
}

/// The effects on the player as the scripts last listed them, with the clock they are timed by.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EffectList {
    pub version: u64,
    pub now: f64,
    pub nodes: Vec<EffectNode>,
}

/// What the examination asks for, as a notice of its change carries it.
fn examined(n: &Notice) -> Option<Examination> {
    n.records.iter().flatten().find_map(|r| match r {
        gameplay::Record::Request(r) => Some(Examination {
            kind: r.kind,
            data_enum: r.data_enum,
            item: r.object,
            ..Examination::default()
        }),
        _ => None,
    })
}

/// A float argument of a notice.
fn float_at(words: &[u32], i: usize) -> f32 {
    f32::from_bits(words.get(i).copied().unwrap_or(0))
}

/// A 64-bit argument of a notice: its high word, then its low.
fn long_at(words: &[u32], i: usize) -> u64 {
    let hi = words.get(i).copied().unwrap_or(0);
    let lo = words.get(i + 1).copied().unwrap_or(0);
    u64::from(hi) << 32 | u64::from(lo)
}

/// A string reference of the client's messages, written with its variables.
fn write(ui: &Ui, string: &str, vars: Vec<(Token, auberean_text::Value)>) -> String {
    let table = ui
        .assets()
        .data_id_by_enum(flow::MESSAGES, flow::TABLE_MAPPING);
    let mut info = auberean_text::StringInfo::entry(table, Token::of(string).raw());
    for (id, value) in vars {
        info.variables.push(auberean_text::Variable {
            id: id.raw(),
            value,
        });
    }
    ui.assets().write(&info).unwrap_or_default()
}

// ---------------------------------------------------------------------------------------------
// One vital's bar
// ---------------------------------------------------------------------------------------------

/// One vital's bar and numbers: the bar's fill goes to the share of the greatest (a bar filling
/// from the right, to what is left over), the numbers "current / greatest".
#[derive(Clone, Copy, Debug)]
pub struct VitalBar {
    current: u64,
    max: u64,
    meter: Option<Handle>,
    text: Option<Handle>,
    right_fill: bool,
}

impl VitalBar {
    /// A bar over its meter and its numbers' text; whether it fills from the right.
    // D2-02976
    pub fn new(ui: &Ui, meter: Option<Handle>, text: Option<Handle>) -> Self {
        let right_fill = meter.is_some_and(|m| ui.child(m, name::RIGHT_FILL).is_some());
        Self {
            current: 0,
            max: 0,
            meter,
            text,
            right_fill,
        }
    }

    /// The vital (when it changed): the fill to the share of the greatest (the current value taken
    /// no higher than it), the numbers written as they came.
    // D2-02977
    pub fn update(&mut self, ui: &mut Ui, current: u64, max: u64) {
        if current == self.current && max == self.max {
            return;
        }
        self.current = current.min(max);
        self.max = max;
        #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
        let mut fill = if max == 0 {
            0.0
        } else {
            (self.current as f64 / max as f64) as f32
        };
        if self.right_fill {
            fill = 1.0 - fill;
        }
        if let Some(m) = self.meter {
            ui.set_attr(m, name::GOAL_POSITION, Value::Float(fill));
        }
        let text = write(
            ui,
            "ID_Vital_DisplayText",
            vec![
                (name::CURRENT, auberean_text::Value::ULong(current)),
                (name::MAX, auberean_text::Value::ULong(max)),
            ],
        );
        if let Some(t) = self.text {
            ui.set_text(t, &text);
        }
    }

    /// A new current value, the greatest kept.
    // D2-02978
    pub fn update_current(&mut self, ui: &mut Ui, current: u64) {
        let max = self.max;
        self.update(ui, current, max);
    }

    /// A new greatest, the current value kept.
    // D2-02979
    pub fn update_max(&mut self, ui: &mut Ui, max: u64) {
        let current = self.current;
        self.update(ui, current, max);
    }

    /// The numbers hidden if shown, shown if hidden; whether they show now.
    // D2-02980
    pub fn toggle_text(&self, ui: &mut Ui) -> bool {
        let Some(t) = self.text else { return false };
        let shown = ui.node(t).is_some_and(|n| n.visible);
        ui.set_visible(t, !shown);
        !shown
    }

    /// The numbers shown or hidden; whether they showed before.
    // D2-02981
    pub fn set_text_state(&self, ui: &mut Ui, on: bool) -> bool {
        let Some(t) = self.text else { return false };
        let was = ui.node(t).is_some_and(|n| n.visible);
        ui.set_visible(t, on);
        was
    }

    /// The bar's state.
    // D2-02982
    pub fn set_meter_state(&self, ui: &mut Ui, s: Token) {
        if let Some(m) = self.meter {
            ui.set_state(m, s);
        }
    }

    /// The vital as shown: current (no higher than the greatest) and greatest.
    #[must_use]
    pub const fn shown(&self) -> (u64, u64) {
        (self.current, self.max)
    }
}

// ---------------------------------------------------------------------------------------------
// The vitals
// ---------------------------------------------------------------------------------------------

/// The vitals' panel.
#[derive(Debug)]
pub struct Vitals {
    frame: FrameId,
    health_field: Option<Handle>,
    vigor_field: Option<Handle>,
    focus_field: Option<Handle>,
    health: Option<VitalBar>,
    pk: Option<VitalBar>,
    vigor: Option<VitalBar>,
    pk_vigorloss: Option<VitalBar>,
    experience: Option<VitalBar>,
    focus: Option<VitalBar>,
    hero: bool,
    /// The scripts have said a vital (until then the player's qualities give health and vigor).
    heard: bool,
    from_qualities: Option<[u32; 4]>,
}

impl Vitals {
    /// The vitals under the status bar's framework, found by name: health (with the
    /// player-killer damage behind it, its numbers hidden), vigor (with the vigor loss behind it),
    /// focus, experience; the health and vigor tooltips; clicks on the bars heard.
    // D2-02402
    pub fn new(ui: &mut Ui, parent: FrameId) -> Self {
        let frame = ui.new_child_frame(parent);
        let health_field = ui.element(name::HEALTH_FIELD);
        let vigor_field = ui.element(name::VIGOR_FIELD);
        let focus_field = ui.element(name::FOCUS_FIELD);
        let display = |ui: &Ui, field: Option<Handle>, meter, text| {
            field.map(|f| {
                VitalBar::new(
                    ui,
                    ui.child_recursive(f, meter),
                    ui.child_recursive(f, text),
                )
            })
        };
        let health = display(ui, health_field, name::HEALTH_METER, name::HEALTH_TEXT);
        let pk = display(ui, health_field, name::PK_METER, name::PK_TEXT);
        let vigor = display(ui, vigor_field, name::VIGOR_METER, name::VIGOR_TEXT);
        let pk_vigorloss = display(
            ui,
            vigor_field,
            name::PK_VIGORLOSS_METER,
            name::PK_VIGORLOSS_TEXT,
        );
        let focus = display(ui, focus_field, name::FOCUS_METER, name::FOCUS_TEXT);
        let experience_field = ui.element(name::EXPERIENCE_FIELD);
        let experience = display(
            ui,
            experience_field,
            name::EXPERIENCE_METER,
            name::EXPERIENCE_TEXT,
        );
        if let Some(f) = health_field {
            Self::set_tooltip(ui, true, f, "ID_Vital_TT_Health");
        }
        if let Some(p) = pk {
            p.set_text_state(ui, false);
        }
        if let Some(f) = vigor_field {
            Self::set_tooltip(ui, true, f, "ID_Vital_TT_Vigor");
        }
        if let Some(p) = pk_vigorloss {
            p.set_text_state(ui, false);
        }
        if let Some(l) = ui.frame_listener(frame) {
            for f in [name::HEALTH_FIELD, name::VIGOR_FIELD, name::FOCUS_FIELD] {
                ui.listen_for_message(l, f, msg::MOUSE_PRESS_TOP);
            }
        }
        Self {
            frame,
            health_field,
            vigor_field,
            focus_field,
            health,
            pk,
            vigor,
            pk_vigorloss,
            experience,
            focus,
            hero: false,
            heard: false,
            from_qualities: None,
        }
    }

    /// A bar's tooltip: its string, saying a click would hide its numbers (shown) or show them.
    // D2-02401
    fn set_tooltip(ui: &mut Ui, shown: bool, field: Handle, string: &str) {
        let which = if shown {
            "ID_Vital_TT_Hide"
        } else {
            "ID_Vital_TT_Show"
        };
        let table = ui
            .assets()
            .data_id_by_enum(flow::MESSAGES, flow::TABLE_MAPPING);
        let text = write(
            ui,
            string,
            vec![(
                name::VALUE,
                auberean_text::Value::Text(Box::new(auberean_text::StringInfo::entry(
                    table,
                    Token::of(which).raw(),
                ))),
            )],
        );
        ui.set_tooltip(field, &text);
    }

    /// A vital the scripts said: its bar (focus only for a hero).
    // D2-02400
    pub fn update_vital(&mut self, ui: &mut Ui, which: u32, current: u64, max: u64) {
        self.heard = true;
        let d = match which {
            vital::HEALTH => self.health.as_mut(),
            vital::VIGOR => self.vigor.as_mut(),
            vital::EXPERIENCE => self.experience.as_mut(),
            vital::FOCUS if self.hero => self.focus.as_mut(),
            vital::PK_DAMAGE => self.pk.as_mut(),
            vital::PK_VIGORLOSS => self.pk_vigorloss.as_mut(),
            _ => None,
        };
        if let Some(d) = d {
            d.update(ui, current, max);
        }
    }

    /// Whether the player is a hero: a hero's focus bar with its numbers and tooltip; anyone
    /// else's shows the ornament, full, without numbers or tooltip.
    // D2-02405
    pub fn update_hero_status(&mut self, ui: &mut Ui, hero: bool, current: u64, max: u64) {
        self.hero = hero;
        let Some(f) = self.focus.as_mut() else {
            return;
        };
        f.set_meter_state(ui, if hero { name::HERO } else { name::NOT_HERO });
        if hero {
            f.set_text_state(ui, true);
            f.update(ui, current, max);
            if let Some(field) = self.focus_field {
                Self::set_tooltip(ui, true, field, "ID_Vital_TT_Focus");
            }
            return;
        }
        f.set_text_state(ui, false);
        f.update(ui, 1, 1);
        if let Some(field) = self.focus_field {
            ui.clear_element_tooltip(field);
        }
    }

    /// A left click on a bar: its numbers hidden or shown, its tooltip saying what a click would do
    /// next (focus only for a hero; anyone else's focus numbers stay hidden).
    // D2-02404
    fn clicked(&mut self, ui: &mut Ui, element: Token) {
        if element == name::HEALTH_FIELD {
            let (Some(d), Some(f)) = (self.health, self.health_field) else {
                return;
            };
            let shown = d.toggle_text(ui);
            Self::set_tooltip(ui, shown, f, "ID_Vital_TT_Health");
        } else if element == name::VIGOR_FIELD {
            let (Some(d), Some(f)) = (self.vigor, self.vigor_field) else {
                return;
            };
            let shown = d.toggle_text(ui);
            Self::set_tooltip(ui, shown, f, "ID_Vital_TT_Vigor");
        } else if element == name::FOCUS_FIELD {
            let Some(d) = self.focus else { return };
            if self.hero {
                let shown = d.toggle_text(ui);
                if let Some(f) = self.focus_field {
                    Self::set_tooltip(ui, shown, f, "ID_Vital_TT_Focus");
                }
                return;
            }
            d.set_text_state(ui, false);
            if let Some(f) = self.focus_field {
                ui.clear_element_tooltip(f);
            }
        }
    }

    /// Health and vigor as shown: current, greatest, current, greatest.
    #[must_use]
    pub fn shown(&self) -> Option<[u32; 4]> {
        let n = |d: Option<VitalBar>| {
            d.map(|d| {
                let (c, m) = d.shown();
                (
                    u32::try_from(c).unwrap_or(u32::MAX),
                    u32::try_from(m).unwrap_or(u32::MAX),
                )
            })
        };
        let (h, hm) = n(self.health)?;
        let (v, vm) = n(self.vigor)?;
        Some([h, hm, v, vm])
    }
}

impl Vitals {
    /// The notices it registers for as it is made, by their own numbers.
    pub const NOTICES: &'static [u32] = &[0x403, 0x1_040C, 0x477];

    /// Its notices as the panels name them.
    #[must_use]
    pub fn notices_of() -> Vec<u32> {
        crate::notices::keys(Self::NOTICES)
    }
}

impl Child for Vitals {
    fn notices(&self) -> Vec<u32> {
        Self::notices_of()
    }

    fn frame(&self) -> FrameId {
        self.frame
    }

    fn listen(&mut self, ui: &mut Ui, mail: &Mail, _out: &mut Vec<Request>) {
        if let Mail::Element {
            element,
            what,
            param,
            ..
        } = *mail
        {
            if what == msg::MOUSE_PRESS_TOP && param == button_id::LEFT {
                self.clicked(ui, element);
            }
        }
    }

    fn notice(&mut self, ui: &mut Ui, n: &Notice, _out: &mut Vec<Request>) {
        match n.native {
            notice::UPDATE_VITAL => {
                let which = n.words.first().copied().unwrap_or(0);
                self.update_vital(ui, which, long_at(&n.words, 1), long_at(&n.words, 3));
            }
            notice::UPDATE_HERO_STATUS => {
                let hero = n.word() != 0;
                self.update_hero_status(ui, hero, long_at(&n.words, 1), long_at(&n.words, 3));
            }
            _ => {}
        }
    }

    /// Until the scripts say a vital (a client without them), health and vigor from the player's
    /// qualities, the focus an ornament.
    fn update(&mut self, ui: &mut Ui, view: &View<'_>, _out: &mut Vec<Request>) {
        if self.heard {
            return;
        }
        let Some(v) = view.vitals else { return };
        if self.from_qualities == Some(v) {
            return;
        }
        let first = self.from_qualities.is_none();
        self.from_qualities = Some(v);
        let [h, hm, g, gm] = v.map(u64::from);
        if let Some(d) = self.health.as_mut() {
            d.update(ui, h, hm);
        }
        if let Some(d) = self.vigor.as_mut() {
            d.update(ui, g, gm);
        }
        if first {
            self.update_hero_status(ui, false, 1, 1);
        }
    }

    /// The vitals go: the clicks no longer heard.
    // D2-02399, D2-02403
    fn close(self: Box<Self>, ui: &mut Ui) {
        ui.drop_frame(self.frame);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

// ---------------------------------------------------------------------------------------------
// The effects
// ---------------------------------------------------------------------------------------------

/// The effects' panel.
#[derive(Debug)]
pub struct Effects {
    frame: FrameId,
    list: Option<Handle>,
    /// The effect selected (its number) and its element.
    selected_eid: u32,
    selected: Option<Handle>,
    /// The scripts' list as last given, and the notices waiting for this frame's list.
    effects: Option<EffectList>,
    noticed: Vec<Notice>,
    started: bool,
}

impl Effects {
    /// The effects under the status bar's framework: their list found by name, a press on an
    /// effect heard. (The effects themselves come with the first list from the scripts.)
    // D2-02420
    pub fn new(ui: &mut Ui, parent: FrameId) -> Self {
        let frame = ui.new_child_frame(parent);
        let list = ui.element(name::EFFECT_LIST_BOX);
        if let Some(l) = ui.frame_listener(frame) {
            ui.listen_for_message(l, name::EFFECT_TEMPLATE, msg::MOUSE_PRESS);
        }
        Self {
            frame,
            list,
            selected_eid: 0,
            selected: None,
            effects: None,
            noticed: Vec::new(),
            started: false,
        }
    }

    /// The effects' icons, in order.
    #[must_use]
    pub fn icons(&self, ui: &Ui) -> Vec<Handle> {
        self.list.map(|l| ui.list_items(l)).unwrap_or_default()
    }

    /// An effect selected (its icon marked, the one before unmarked; an alerted icon keeps its
    /// alert).
    // D2-02406
    fn select_element(&mut self, ui: &mut Ui, e: Option<Handle>) {
        if self.selected == e {
            return;
        }
        if let Some(icon) = self
            .selected
            .and_then(|old| ui.child_recursive(old, name::ICON_FIELD))
        {
            ui.set_attr(icon, name::SELECTED, Value::Bool(false));
            let alert = ui.node(icon).is_some_and(|n| n.bool_attr(name::ALERT));
            let s = if alert {
                name::STATE_UNSELECTED_ALERT
            } else {
                name::STATE_UNSELECTED
            };
            ui.set_state(icon, s);
        }
        self.selected = e;
        if let Some(icon) = e.and_then(|e| ui.child_recursive(e, name::ICON_FIELD)) {
            ui.set_attr(icon, name::SELECTED, Value::Bool(true));
            let alert = ui.node(icon).is_some_and(|n| n.bool_attr(name::ALERT));
            let s = if alert {
                name::STATE_SELECTED_ALERT
            } else {
                name::STATE_SELECTED
            };
            ui.set_state(icon, s);
        }
    }

    /// The effects again: each new one made in order of when it runs out (its name as its
    /// tooltip, its icon, a timed one's bar running out with its colors' shares set, a pulsed one's
    /// end kept); effects no longer listed taken away; the selected one marked again.
    // D2-02417
    fn init(&mut self, ui: &mut Ui) {
        self.selected = None;
        let Some(l) = self.list else { return };
        let list = self.effects.clone().unwrap_or_default();
        let now = list.now;
        for node in &list.nodes {
            let timed = node.duration != -1.0 && node.time_cast != -1.0;
            #[allow(clippy::cast_possible_truncation)]
            let expiry = if timed {
                (node.time_cast + node.duration) as f32
            } else {
                f32::MAX
            };
            let mut before = None;
            let mut found = false;
            for i in 0..ui.list_items(l).len() {
                let Some(item) = ui.list_item(l, i) else {
                    continue;
                };
                let Some(eid) = ui.node(item).and_then(|n| n.id_attr(name::EFFECT_ID)) else {
                    continue;
                };
                if eid == node.eid {
                    found = true;
                    break;
                }
                if before.is_none() {
                    let timeout = ui
                        .node(item)
                        .and_then(|n| n.float_attr(name::EFFECT_TIMEOUT))
                        .unwrap_or(f32::MIN_POSITIVE);
                    if timeout > expiry {
                        before = Some(item);
                    }
                }
            }
            if found {
                continue;
            }
            self.make_effect(ui, l, node, before, now, expiry);
        }
        let items = ui.list_items(l);
        for item in items.into_iter().rev() {
            let eid = ui.node(item).and_then(|n| n.id_attr(name::EFFECT_ID));
            let listed = eid.is_some_and(|e| list.nodes.iter().any(|n| n.eid == e));
            if !listed {
                if let Some(i) = ui.list_index_of(l, item) {
                    if let Some(e) = ui.list_remove(l, i) {
                        ui.remove(e);
                    }
                }
                continue;
            }
            if eid == Some(self.selected_eid) {
                self.selected = Some(item);
                if let Some(icon) = ui.child_recursive(item, name::ICON_FIELD) {
                    ui.set_attr(icon, name::SELECTED, Value::Bool(true));
                    ui.set_state(icon, name::STATE_SELECTED);
                }
            }
        }
    }

    /// One effect's icon, put before `before` (else at the end).
    fn make_effect(
        &mut self,
        ui: &mut Ui,
        l: Handle,
        node: &EffectNode,
        before: Option<Handle>,
        now: f64,
        expiry: f32,
    ) {
        let Some(e) = ui.frame_create_element(STATUS_LAYOUT_ENUM, name::EFFECT_TEMPLATE) else {
            return;
        };
        ui.set_tooltip(e, &node.name);
        ui.list_insert_before(l, e, before);
        if let Some(icon) = ui.child_recursive(e, name::ICON_FIELD) {
            ui.add_image_to_state(icon, node.icon, 2, state::STATELESS);
        }
        let end = node.time_cast + node.duration;
        let timed = node.duration != -1.0 && node.time_cast != -1.0;
        if timed && end > now {
            if node.pulsed {
                ui.set_state(e, name::PULSED);
                #[allow(clippy::cast_possible_truncation)]
                ui.set_attr(e, name::EXPIRATION, Value::Float(end as f32));
            } else {
                ui.set_state(e, name::ACTIVE);
                if let Some(m) = ui.child_recursive(e, name::EFFECT_TIME_METER) {
                    Self::run_out(ui, m, node, now, end);
                }
            }
        }
        ui.set_attr(e, name::EFFECT_ID, Value::Id(node.eid));
        ui.set_attr(e, name::EFFECT_TIMEOUT, Value::Float(expiry));
    }

    /// A timed effect's bar: from what is left, running out over the rest of its time; its colors
    /// change at shares set five minutes (at most half the time) and a minute (at most a quarter)
    /// before the end.
    fn run_out(ui: &mut Ui, m: Handle, node: &EffectNode, now: f64, end: f64) {
        #[allow(clippy::cast_possible_truncation)]
        let elapsed = ((now - node.time_cast) / node.duration) as f32;
        let left = 1.0 - elapsed;
        ui.meter_state_at(m, name::FULL, 0.0);
        #[allow(clippy::cast_possible_truncation)]
        let per = (1.0 / node.duration) as f32;
        let inv_left = 1.0 / left;
        let mid = (((1.0 - (per * 300.0).min(0.5)) - elapsed) * inv_left).max(0.0);
        ui.meter_state_at(m, name::MID, mid);
        let low = (((1.0 - (per * 60.0).min(0.25)) - elapsed) * inv_left).max(0.0);
        ui.meter_state_at(m, name::LOW, low);
        #[allow(clippy::cast_possible_truncation)]
        ui.set_attr(
            m,
            name::SMOOTH_MOVEMENT_DURATION,
            Value::Float((end - now) as f32),
        );
        ui.set_attr(m, name::POSITION, Value::Float(left));
        ui.set_attr(m, name::GOAL_POSITION, Value::Float(0.0));
    }

    /// A pulsed effect missed its pulse: its icon alerted, the effect active, its bar low and
    /// running out from full over what is left of its time.
    // D2-02409
    fn pulse_missed(&mut self, ui: &mut Ui, eid: u32, now: f64) {
        let Some(l) = self.list else { return };
        let items = ui.list_items(l);
        let Some(&e) = items.iter().rev().find(|e| {
            ui.node(**e)
                .and_then(|n| n.id_attr(name::EFFECT_ID))
                .is_some_and(|i| i == eid)
        }) else {
            return;
        };
        let Some(icon) = ui.child_recursive(e, name::ICON_FIELD) else {
            return;
        };
        ui.set_attr(icon, name::ALERT, Value::Bool(true));
        let selected = ui.node(icon).is_some_and(|n| n.bool_attr(name::SELECTED));
        let s = if selected {
            name::STATE_SELECTED_ALERT
        } else {
            name::STATE_UNSELECTED_ALERT
        };
        ui.set_state(icon, s);
        ui.set_state(e, name::ACTIVE);
        let Some(m) = ui.child_recursive(e, name::EFFECT_TIME_METER) else {
            return;
        };
        if let Some(f) = ui.child(m, name::LEFT_FILL) {
            ui.set_state(f, name::LOW);
        }
        if let Some(x) = ui.node(e).and_then(|n| n.float_attr(name::EXPIRATION)) {
            #[allow(clippy::cast_possible_truncation)]
            let d = (f64::from(x) - now) as f32;
            ui.set_attr(m, name::SMOOTH_MOVEMENT_DURATION, Value::Float(d));
            ui.set_attr(m, name::POSITION, Value::Float(1.0));
            ui.set_attr(m, name::GOAL_POSITION, Value::Float(0.0));
        }
    }

    /// The examination asks for another thing: an effect (selected anew if it is another), or
    /// something else (none selected).
    // D2-02418
    pub fn examination_request_changed(&mut self, ui: &mut Ui, request: Examination) {
        if request.kind != EXAMINE_EFFECT {
            self.select_element(ui, None);
            self.selected_eid = 0;
            return;
        }
        if request.data_enum != self.selected_eid {
            self.selected_eid = request.data_enum;
            self.init(ui);
        }
    }

    /// A press on an effect (left or right button): selected, and the examination panel opened on
    /// it with nothing selected in the world.
    // D2-02408
    fn pressed(&mut self, ui: &mut Ui, e: Handle, button: u32, out: &mut Vec<Request>) {
        if button != button_id::LEFT && button != button_id::RIGHT {
            return;
        }
        let eid = ui
            .node(e)
            .and_then(|n| n.id_attr(name::EFFECT_ID))
            .unwrap_or(0);
        self.select_element(ui, Some(e));
        self.selected_eid = eid;
        out.push(Request::Examine(Examination {
            kind: EXAMINE_EFFECT,
            data_enum: eid,
            ..Examination::default()
        }));
        out.push(Request::OpenExaminationPanel);
    }
}

impl Effects {
    /// The notices it registers for as it is made, by their own numbers.
    pub const NOTICES: &'static [u32] = &[0x45D, 0x410, 0x47B, 0x1_040C];

    /// Its notices as the panels name them.
    #[must_use]
    pub fn notices_of() -> Vec<u32> {
        crate::notices::keys(Self::NOTICES)
    }
}

impl Child for Effects {
    fn notices(&self) -> Vec<u32> {
        Self::notices_of()
    }

    fn frame(&self) -> FrameId {
        self.frame
    }

    fn listen(&mut self, ui: &mut Ui, mail: &Mail, out: &mut Vec<Request>) {
        if let Mail::Element {
            from,
            element,
            what,
            param,
        } = *mail
        {
            if element == name::EFFECT_TEMPLATE && what == msg::MOUSE_PRESS {
                self.pressed(ui, from, param, out);
            }
        }
    }

    /// Notices wait for this frame's list from the scripts.
    fn notice(&mut self, _ui: &mut Ui, n: &Notice, _out: &mut Vec<Request>) {
        if n.native == notice::UPDATE_EFFECT
            || n.native == notice::PULSE_MISSED
            || n.native == gameplay::notice::EXAMINATION_REQUEST_CHANGED
        {
            self.noticed.push(n.clone());
        }
    }

    /// This frame's list from the scripts (the first makes the effects), then the notices.
    // D2-02419
    fn update(&mut self, ui: &mut Ui, view: &View<'_>, _out: &mut Vec<Request>) {
        if let Some(l) = view.effects {
            if self.effects.as_ref().map(|e| e.version) != Some(l.version) {
                self.effects = Some(l.clone());
            }
        }
        if !self.started {
            self.started = true;
            self.init(ui);
        }
        let now = self.effects.as_ref().map_or(0.0, |e| e.now);
        for n in std::mem::take(&mut self.noticed) {
            if n.native == notice::UPDATE_EFFECT {
                self.init(ui);
            } else if n.native == notice::PULSE_MISSED {
                self.pulse_missed(ui, n.word(), now);
            } else if let Some(request) = examined(&n) {
                self.examination_request_changed(ui, request);
            }
        }
    }

    /// The effects go: the presses no longer heard.
    // D2-02407
    fn close(self: Box<Self>, ui: &mut Ui) {
        if let Some(l) = self.list {
            ui.list_flush(l);
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

// ---------------------------------------------------------------------------------------------
// The indicators
// ---------------------------------------------------------------------------------------------

/// The network indicator.
#[derive(Debug)]
pub struct NetworkIndicator {
    frame: FrameId,
    field: Option<Handle>,
    /// The state shown; the seconds since the server was last heard at which it turns good, bad
    /// and ugly; whether the graph of the bandwidth shows.
    shown: Token,
    good: f32,
    bad: f32,
    ugly: f32,
    graph: bool,
    last: Option<Link>,
    /// The graph's columns, one a measure, the oldest at `next` (written over next).
    traffic: Vec<Traffic>,
    next: usize,
    /// The state shown changed at the last measure from a lit one: the graph's ground is left
    /// clear until the next.
    fresh: bool,
    /// The options were read again: whether the graph shows is asked of them.
    reload: bool,
}

/// One column of the bandwidth graph: the bytes sent and received a second at one measure.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Traffic {
    pub sent: u32,
    pub received: u32,
}

/// The bandwidth graph's largest size, either way.
const GRAPH_SIZE: i32 = 32;

impl NetworkIndicator {
    /// The network indicator under the indicators' framework: its field, its thresholds, plain.
    // D2-02439
    pub fn new(ui: &mut Ui, parent: FrameId) -> Self {
        let frame = ui.new_child_frame(parent);
        let field = ui.element(name::NET_STATUS_FIELD);
        let f = |a| {
            field
                .and_then(|h| ui.node(h))
                .and_then(|n| n.float_attr(a))
                .unwrap_or(0.0)
        };
        Self {
            frame,
            field,
            shown: state::STATELESS,
            good: f(name::GOOD),
            bad: f(name::BAD),
            ugly: f(name::UGLY),
            graph: false,
            last: None,
            traffic: Vec::new(),
            next: 0,
            fresh: false,
            reload: false,
        }
    }

    /// The state shown.
    #[must_use]
    pub const fn shown(&self) -> Token {
        self.shown
    }

    /// Whether the bandwidth graph shows.
    #[must_use]
    pub const fn graph(&self) -> bool {
        self.graph
    }

    /// The bandwidth graph's columns, the oldest first.
    #[must_use]
    pub fn traffic(&self) -> Vec<Traffic> {
        let n = self.traffic.len();
        (0..n).map(|i| self.traffic[(self.next + i) % n]).collect()
    }

    /// The connection measured again: the tooltip (the round trip and the loss, with the graph
    /// the bytes in and out each second); with the graph, a column of it (as many columns as the
    /// indicator is wide inside its border); the state by how long since the server was last
    /// heard; the graph drawn again.
    // D2-02437
    pub fn heartbeat(&mut self, ui: &mut Ui, link: Link) {
        let Some(field) = self.field else { return };
        #[allow(clippy::cast_possible_truncation)]
        let loss = f64::from((link.loss * 100.0) as f32);
        let mut vars = vec![
            (
                name::LATENCY,
                auberean_text::Value::Float {
                    value: link.latency * 1000.0,
                    decimals: 1,
                },
            ),
            (
                name::LOSS,
                auberean_text::Value::Float {
                    value: loss,
                    decimals: 1,
                },
            ),
        ];
        // The bytes a second: the count over the span, kept as a single.
        #[allow(clippy::cast_possible_truncation)]
        let rate = |bytes: u32| (f64::from(bytes) * (1.0 / f64::from(link.span))) as f32;
        let (bytes_in, bytes_out) = (rate(link.bytes_in), rate(link.bytes_out));
        let string = if self.graph {
            vars.push((
                name::BYTES_IN,
                auberean_text::Value::Float {
                    value: f64::from(bytes_in),
                    decimals: 1,
                },
            ));
            vars.push((
                name::BYTES_OUT,
                auberean_text::Value::Float {
                    value: f64::from(bytes_out),
                    decimals: 1,
                },
            ));
            "ID_Indicator_TT_NetStatusBW"
        } else {
            "ID_Indicator_TT_NetStatus"
        };
        let text = write(ui, string, vars);
        ui.set_tooltip(field, &text);
        let border = ui
            .node(field)
            .and_then(|n| n.int_attr(name::BORDER_PIXELS))
            .unwrap_or(0);
        let width = ui.node(field).map_or(0, |n| n.rect.width());
        if self.graph && border * 2 < width {
            let columns = usize::try_from(width - border * 2).unwrap_or(1);
            if columns != self.traffic.len() {
                self.traffic.resize(columns, Traffic::default());
            }
            // The rates kept as whole bytes, cut toward zero.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let column = Traffic {
                sent: bytes_out as u32,
                received: bytes_in as u32,
            };
            if let Some(c) = self.traffic.get_mut(self.next) {
                *c = column;
            }
            self.next = (self.next + 1) % columns;
        }
        let f = link.since_heard;
        let s = if f >= self.ugly {
            name::UGLY
        } else if f >= self.bad {
            name::BAD
        } else if f >= self.good {
            name::GOOD
        } else {
            state::STATELESS
        };
        if self.shown == s {
            self.fresh = false;
        } else {
            self.fresh = self.shown != state::STATELESS;
            ui.set_state(field, s);
            self.shown = s;
        }
        self.last = Some(link);
        self.draw_graph(ui);
    }

    /// The options came again: whether the bandwidth graph shows; the indicator drawn again.
    // D2-02430, D2-02431
    pub fn reload_options(&mut self, ui: &mut Ui, graph: bool) {
        self.graph = graph;
        self.draw_graph(ui);
    }

    /// The bandwidth graph over the indicator, inside its border (at most 32 texels either way),
    /// while the indicator shows and the player asked for it: the ground in its color (clear just
    /// after the state changed from a lit one), then a column a measure from the oldest, the bytes
    /// sent a second as a bar down from the top and the bytes received as one up from the foot,
    /// blended over the indicator's picture.
    // D2-02433, D2-02438
    fn draw_graph(&self, ui: &mut Ui) {
        let Some(field) = self.field else { return };
        let Some(n) = ui.node(field) else { return };
        let border = n.int_attr(name::BORDER_PIXELS).unwrap_or(0);
        let (w, h) = (n.rect.width(), n.rect.height());
        if !self.graph || !ui.frame_shown(self.frame) || border * 2 >= w || border * 2 >= h {
            ui.set_overlay(field, None);
            return;
        }
        let color = |parts: [Token; 4]| parts.map(|t| n.float_attr(t).unwrap_or(0.0));
        let mut ground = color([name::R_BACK, name::G_BACK, name::B_BACK, name::A_BACK]);
        let sent = color([name::R_SENT, name::G_SENT, name::B_SENT, name::A_SENT]);
        let received = color([name::R_RECV, name::G_RECV, name::B_RECV, name::A_RECV]);
        if self.fresh {
            ground[3] = 0.0;
        }
        let mut graph = Graph::new(
            (w - border * 2).min(GRAPH_SIZE),
            (h - border * 2).min(GRAPH_SIZE),
            ground,
        );
        let columns = self.traffic.len();
        let mut at = self.next;
        for x in 0..graph.picture.width {
            if at >= columns {
                break;
            }
            let c = self.traffic[at];
            graph.bar(c.sent, x, sent, false);
            graph.bar(c.received, x, received, true);
            at = (at + 1) % columns;
        }
        ui.set_overlay(field, Some((border, border, graph.picture)));
    }
}

/// A color as the interface's pictures hold it: each part times 255, cut toward zero.
// D2-03480
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn texel(c: [f32; 4]) -> [u8; 4] {
    let p = |v: f32| (f64::from(v) * 255.0) as u8;
    [p(c[2]), p(c[1]), p(c[0]), p(c[3])]
}

/// The bandwidth graph being drawn: its picture, its ground's color, and the longest a bar grows
/// (half the graph's height, less one).
#[derive(Debug)]
struct Graph {
    picture: auberean_ui::draw::Surface,
    ground: [f32; 4],
    longest: u32,
}

impl Graph {
    /// A graph `w` by `h` filled with its ground.
    // D2-02428
    fn new(w: i32, h: i32, ground: [f32; 4]) -> Self {
        let mut picture = auberean_ui::draw::Surface::new(w, h);
        let g = texel(ground);
        for y in 0..h {
            for x in 0..w {
                picture.put(x, y, g);
            }
        }
        Self {
            picture,
            ground,
            longest: u32::try_from((h + 1) / 2 - 1).unwrap_or(0),
        }
    }

    /// A color part of the way from the ground to `color`.
    // D2-02429
    #[allow(clippy::cast_possible_truncation)]
    fn blend(&self, color: [f32; 4], t: f32) -> [f32; 4] {
        let keep = 1.0 - f64::from(t);
        let mut out = [0.0; 4];
        for (i, o) in out.iter_mut().enumerate() {
            *o = (keep * f64::from(self.ground[i]) + f64::from(t) * f64::from(color[i])) as f32;
        }
        out
    }

    /// One measure's bar in column `x`: as long as the eighth of the bytes takes halvings to come
    /// down to one (at most the longest), down from the top or up from the foot, with the texel
    /// past its end part of the way to its color by how far the bytes go past that power of two.
    // D2-02432
    #[allow(clippy::cast_possible_truncation)]
    fn bar(&mut self, bytes: u32, x: i32, color: [f32; 4], from_foot: bool) {
        let eighths = bytes >> 3;
        if eighths == 0 {
            return;
        }
        let level = (31 - eighths.leading_zeros()).min(self.longest);
        let l = i32::try_from(level).unwrap_or(0);
        let foot = self.picture.height - 1;
        let (y0, y1, past) = if from_foot {
            (foot - l, foot, foot - l - 1)
        } else {
            (0, l, l + 1)
        };
        let c = texel(color);
        for y in y0..=y1 {
            self.picture.put(x, y, c);
        }
        if level != self.longest {
            let whole = 1u32 << level;
            let t = (f64::from(eighths - whole) / f64::from(whole)) as f32;
            let b = texel(self.blend(color, t));
            self.picture.put(x, past, b);
        }
    }
}

impl NetworkIndicator {
    /// The notices it registers for as it is made, by their own numbers.
    pub const NOTICES: &'static [u32] = &[0x46D, 0x417];

    /// Its notices as the panels name them.
    #[must_use]
    pub fn notices_of() -> Vec<u32> {
        crate::notices::keys(Self::NOTICES)
    }
}

impl Child for NetworkIndicator {
    fn notices(&self) -> Vec<u32> {
        Self::notices_of()
    }

    fn frame(&self) -> FrameId {
        self.frame
    }

    /// The player's options read again, or changed by the options window: whether the graph
    /// shows is asked of them.
    fn notice(&mut self, _ui: &mut Ui, n: &Notice, _out: &mut Vec<Request>) {
        if n.native == gameplay::notice::RELOAD_OPTIONS
            || n.native == crate::options::notice::OPTIONS_CHANGED
        {
            self.reload = true;
        }
    }

    fn update(&mut self, ui: &mut Ui, view: &View<'_>, _out: &mut Vec<Request>) {
        if self.reload {
            if let Some(o) = view.options {
                self.reload = false;
                self.reload_options(ui, o.bits & crate::options::LATENCY_GRAPH != 0);
            }
        }
        if let Some(l) = view.link {
            if self.last != Some(l) {
                self.heartbeat(ui, l);
            }
        }
    }

    // D2-02435, D2-02436
    fn close(self: Box<Self>, ui: &mut Ui) {
        ui.drop_frame(self.frame);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// The conflict indicator.
#[derive(Debug)]
pub struct PkStatus {
    frame: FrameId,
    field: Option<Handle>,
}

impl PkStatus {
    /// The conflict indicator under the indicators' framework: its field.
    // D2-02441
    pub fn new(ui: &mut Ui, parent: FrameId) -> Self {
        Self {
            frame: ui.new_child_frame(parent),
            field: ui.element(name::PK_STATUS_FIELD),
        }
    }

    /// The kind of player conflict the place allows, as the scripts number it (1 peace, 2
    /// controlled, 3 faction against faction, 4 free for all, 5 faction against faction anywhere;
    /// anything else, nothing shown).
    // D2-02442
    pub fn update_faction_status(&self, ui: &mut Ui, area: u32) {
        let s = match area {
            1 => Token::of("peace"),
            2 => Token::of("control"),
            3 => Token::of("factional"),
            4 => Token::of("ffa"),
            5 => Token::of("kvkanywhere"),
            _ => state::STATELESS,
        };
        if let Some(f) = self.field {
            ui.set_state(f, s);
        }
    }

    /// The same, by area.
    pub fn set_conflict_area(&self, ui: &mut Ui, a: ConflictArea) {
        let n = match a {
            ConflictArea::Peace => 1,
            ConflictArea::Control => 2,
            ConflictArea::Factional => 3,
            ConflictArea::FreeForAll => 4,
            ConflictArea::FactionalAnywhere => 5,
        };
        self.update_faction_status(ui, n);
    }
}

impl PkStatus {
    /// The notices it registers for as it is made, by their own numbers.
    pub const NOTICES: &'static [u32] = &[0x460, 0x1_040C];

    /// Its notices as the panels name them.
    #[must_use]
    pub fn notices_of() -> Vec<u32> {
        crate::notices::keys(Self::NOTICES)
    }
}

impl Child for PkStatus {
    fn notices(&self) -> Vec<u32> {
        Self::notices_of()
    }

    fn frame(&self) -> FrameId {
        self.frame
    }

    fn notice(&mut self, ui: &mut Ui, n: &Notice, _out: &mut Vec<Request>) {
        if n.native == notice::UPDATE_FACTION_STATUS {
            self.update_faction_status(ui, n.word());
        }
    }

    // D2-02443
    fn close(self: Box<Self>, ui: &mut Ui) {
        ui.drop_frame(self.frame);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// The vitae indicator.
#[derive(Debug)]
pub struct VitaeStatus {
    frame: FrameId,
    field: Option<Handle>,
    overlay: Option<Handle>,
}

impl VitaeStatus {
    /// The vitae indicator under the indicators' framework: its field, its overlay hidden; a click
    /// heard.
    // D2-02444
    pub fn new(ui: &mut Ui, parent: FrameId) -> Self {
        let frame = ui.new_child_frame(parent);
        let field = ui.element(name::VITAE_STATUS_FIELD);
        let overlay = field.and_then(|f| ui.child_recursive(f, name::OVERLAY));
        if let Some(o) = overlay {
            ui.set_visible(o, false);
        }
        if let Some(l) = ui.frame_listener(frame) {
            ui.listen_for_message(l, name::VITAE_STATUS_FIELD, msg::MOUSE_PRESS_TOP);
        }
        Self {
            frame,
            field,
            overlay,
        }
    }

    /// The vitae: below full, the indicator worn down and its tooltip giving the vitae, the share
    /// gained and the experience to work it off; at full, plain with the full vitae's tooltip.
    // D2-02446
    pub fn update_vitae(&self, ui: &mut Ui, current: f32, gain: f32, experience: f32) {
        let (s, text) = if current >= 100.0 {
            (
                state::STATELESS,
                write(ui, "ID_Indicator_TT_FullVitae", Vec::new()),
            )
        } else {
            let int = |f: f32| auberean_text::Value::Int(dereth_primitives::num::to_i32(f));
            (
                name::NOT_FULL,
                write(
                    ui,
                    "ID_Indicator_TT_VitaeStatus",
                    vec![
                        (name::CUR, int(current)),
                        (name::INC, int(gain)),
                        (name::XP, int(experience)),
                    ],
                ),
            )
        };
        if let Some(f) = self.field {
            ui.set_state(f, s);
            ui.set_tooltip(f, &text);
        }
    }

    /// Whether the vitae shows worn (true below full; false plain).
    #[must_use]
    pub fn worn(&self, ui: &Ui) -> bool {
        self.field
            .and_then(|f| ui.node(f))
            .is_some_and(|n| n.state == name::NOT_FULL)
    }

    /// The examination asks for another thing: the overlay shows while it asks for the vitae and
    /// the vitae is below full.
    // D2-02445
    pub fn examination_request_changed(&self, ui: &mut Ui, request: Examination) {
        let Some(o) = self.overlay else { return };
        let on = request.kind == EXAMINE_VITAE && self.worn(ui);
        ui.set_visible(o, on);
    }
}

impl VitaeStatus {
    /// The notices it registers for as it is made, by their own numbers.
    pub const NOTICES: &'static [u32] = &[0x45D, 0x461, 0x1_040C];

    /// Its notices as the panels name them.
    #[must_use]
    pub fn notices_of() -> Vec<u32> {
        crate::notices::keys(Self::NOTICES)
    }
}

impl Child for VitaeStatus {
    fn notices(&self) -> Vec<u32> {
        Self::notices_of()
    }

    fn frame(&self) -> FrameId {
        self.frame
    }

    /// A click on the indicator while the vitae is below full: the examination panel opened on it,
    /// nothing selected in the world.
    // D2-02448
    fn listen(&mut self, ui: &mut Ui, mail: &Mail, out: &mut Vec<Request>) {
        if let Mail::Element { element, param, .. } = *mail {
            if element == name::VITAE_STATUS_FIELD
                && self.worn(ui)
                && (param == button_id::LEFT || param == button_id::RIGHT)
            {
                out.push(Request::Examine(Examination {
                    kind: EXAMINE_VITAE,
                    ..Examination::default()
                }));
                out.push(Request::OpenExaminationPanel);
            }
        }
    }

    fn notice(&mut self, ui: &mut Ui, n: &Notice, _out: &mut Vec<Request>) {
        if n.native == notice::UPDATE_VITAE {
            let w = &n.words;
            self.update_vitae(ui, float_at(w, 0), float_at(w, 1), float_at(w, 2));
        } else if n.native == gameplay::notice::EXAMINATION_REQUEST_CHANGED {
            if let Some(request) = examined(n) {
                self.examination_request_changed(ui, request);
            }
        }
    }

    // D2-02447
    fn close(self: Box<Self>, ui: &mut Ui) {
        ui.drop_frame(self.frame);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// The indicators' panel: the network, conflict and vitae indicators, each a framework of its own.
#[derive(Debug)]
// D2-02426
pub struct Indicators {
    frame: FrameId,
    /// The indicators, in the panel's list (the last made first).
    children: Vec<Box<dyn Child>>,
}

impl Indicators {
    /// The indicators under the status bar's framework.
    // D2-02425
    pub fn new(ui: &mut Ui, parent: FrameId) -> Self {
        let frame = ui.new_child_frame(parent);
        let mut me = Self {
            frame,
            children: Vec::new(),
        };
        me.setup_children(ui);
        me
    }

    /// The network, conflict and vitae indicators, each put at the front of the list.
    // D2-02422
    fn setup_children(&mut self, ui: &mut Ui) {
        let f = self.frame;
        let net: Box<dyn Child> = Box::new(NetworkIndicator::new(ui, f));
        let pk: Box<dyn Child> = Box::new(PkStatus::new(ui, f));
        let vitae: Box<dyn Child> = Box::new(VitaeStatus::new(ui, f));
        for c in [net, pk, vitae] {
            self.children.insert(0, c);
        }
    }

    /// The first indicator of a kind.
    #[must_use]
    pub fn child<T: Child>(&self) -> Option<&T> {
        self.children
            .iter()
            .find_map(|c| c.as_any().downcast_ref::<T>())
    }

    /// The indicators go.
    // D2-02423, D2-02424
    fn cleanup_children(&mut self, ui: &mut Ui) {
        for c in self.children.drain(..) {
            c.close(ui);
        }
    }
}

impl Indicators {
    /// The notices its panels register for as they are made, in the order they are made.
    #[must_use]
    pub fn notices_of() -> Vec<u32> {
        crate::notices::union([
            NetworkIndicator::notices_of(),
            PkStatus::notices_of(),
            VitaeStatus::notices_of(),
        ])
    }
}

impl Child for Indicators {
    fn notices(&self) -> Vec<u32> {
        Self::notices_of()
    }

    fn frame(&self) -> FrameId {
        self.frame
    }

    /// Shown or hidden with its framework.
    // D2-02421
    fn show(&mut self, ui: &mut Ui, on: bool, out: &mut Vec<Request>) {
        ui.frame_show(self.frame, on);
        for c in &mut self.children {
            gameplay::hear(c.as_mut(), ui, out);
        }
    }

    fn notice(&mut self, ui: &mut Ui, n: &Notice, out: &mut Vec<Request>) {
        for c in &mut self.children {
            gameplay::hand(c.as_mut(), ui, n, out);
        }
    }

    fn update(&mut self, ui: &mut Ui, view: &View<'_>, out: &mut Vec<Request>) {
        for c in &mut self.children {
            gameplay::drive(c.as_mut(), ui, view, &[], out);
        }
    }

    fn close(mut self: Box<Self>, ui: &mut Ui) {
        self.cleanup_children(ui);
        ui.drop_frame(self.frame);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

// ---------------------------------------------------------------------------------------------
// The status bar
// ---------------------------------------------------------------------------------------------

/// The status bar's panel.
#[derive(Debug)]
// D2-02398
pub struct StatusBar {
    frame: FrameId,
    /// The bar.
    pub root: Option<Handle>,
    /// Its panels, in its list (the last made first): the indicators, the effects, the vitals.
    children: Vec<Box<dyn Child>>,
}

impl StatusBar {
    /// The status bar under the play screen: the bar from its record, its panels, shown.
    // D2-02397
    pub fn new(ui: &mut Ui, parent: FrameId, out: &mut Vec<Request>) -> Self {
        let frame = ui.new_child_frame(parent);
        let root = ui.frame_create_root(frame, STATUS_LAYOUT_ENUM, name::STATUSBAR_FIELD);
        let mut me = Self {
            frame,
            root,
            children: Vec::new(),
        };
        if root.is_some() {
            me.setup_children(ui);
            me.show(ui, true, out);
        }
        me
    }

    /// The vitals, the effects and the indicators, each put at the front of the list.
    // D2-02394
    fn setup_children(&mut self, ui: &mut Ui) {
        let f = self.frame;
        let vitals: Box<dyn Child> = Box::new(Vitals::new(ui, f));
        let effects: Box<dyn Child> = Box::new(Effects::new(ui, f));
        let indicators: Box<dyn Child> = Box::new(Indicators::new(ui, f));
        for c in [vitals, effects, indicators] {
            self.children.insert(0, c);
        }
    }

    /// The first panel of a kind.
    #[must_use]
    pub fn child<T: Child>(&self) -> Option<&T> {
        self.children
            .iter()
            .find_map(|c| c.as_any().downcast_ref::<T>())
    }

    /// The first panel of a kind, to change.
    pub fn child_mut<T: Child>(&mut self) -> Option<&mut T> {
        self.children
            .iter_mut()
            .find_map(|c| c.as_any_mut().downcast_mut::<T>())
    }

    /// The vitals as shown: health, its greatest, vigor, its greatest.
    #[must_use]
    pub fn vitals_shown(&self) -> Option<[u32; 4]> {
        let v = self.child::<Vitals>()?;
        v.from_qualities
            .or_else(|| v.heard.then(|| v.shown()).flatten())
    }

    /// The vitals given at once (health, vigor, a hero's focus): as the scripts would say them.
    pub fn set_vitals(&mut self, ui: &mut Ui, health: Vital, vigor: Vital, focus: Option<Vital>) {
        let Some(v) = self.child_mut::<Vitals>() else {
            return;
        };
        let f = focus.unwrap_or(Vital { current: 1, max: 1 });
        v.update_hero_status(ui, focus.is_some(), f.current.into(), f.max.into());
        v.update_vital(ui, vital::HEALTH, health.current.into(), health.max.into());
        v.update_vital(ui, vital::VIGOR, vigor.current.into(), vigor.max.into());
    }

    /// The conflict indicator.
    pub fn set_conflict_area(&self, ui: &mut Ui, a: ConflictArea) {
        if let Some(p) = self
            .child::<Indicators>()
            .and_then(|i| i.child::<PkStatus>())
        {
            p.set_conflict_area(ui, a);
        }
    }
}

impl StatusBar {
    /// The notices its panels register for as they are made, in the order they are made.
    #[must_use]
    pub fn notices_of() -> Vec<u32> {
        crate::notices::union([
            Vitals::notices_of(),
            Effects::notices_of(),
            NetworkIndicator::notices_of(),
            PkStatus::notices_of(),
            VitaeStatus::notices_of(),
        ])
    }
}

impl Child for StatusBar {
    fn notices(&self) -> Vec<u32> {
        Self::notices_of()
    }

    fn frame(&self) -> FrameId {
        self.frame
    }

    /// Shown or hidden with its framework: the bar with it, its panels told, how much of the
    /// screen's top it covers said.
    // D2-02393
    fn show(&mut self, ui: &mut Ui, on: bool, out: &mut Vec<Request>) {
        ui.frame_show(self.frame, on);
        for c in &mut self.children {
            gameplay::hear(c.as_mut(), ui, out);
        }
        let shown = ui.frame_shown(self.frame);
        let Some(r) = self.root else { return };
        ui.set_visible(r, shown);
        let height = ui
            .node(r)
            .and_then(|n| n.int_attr(name::PRECLUDE_HEIGHT))
            .unwrap_or(0);
        // D2-00322
        out.push(Request::PrecludeFromTop(if shown { height } else { 0 }));
    }

    fn notice(&mut self, ui: &mut Ui, n: &Notice, out: &mut Vec<Request>) {
        for c in &mut self.children {
            gameplay::hand(c.as_mut(), ui, n, out);
        }
    }

    fn update(&mut self, ui: &mut Ui, view: &View<'_>, out: &mut Vec<Request>) {
        for c in &mut self.children {
            gameplay::drive(c.as_mut(), ui, view, &[], out);
        }
    }

    fn close(mut self: Box<Self>, ui: &mut Ui) {
        for c in self.children.drain(..) {
            c.close(ui);
        }
        if let Some(r) = self.root {
            ui.frame_remove_root(self.frame, r);
            ui.remove(r);
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
