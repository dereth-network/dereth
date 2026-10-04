//! Classic panels read the existing game contract and emit semantic requests.
pub use crate::TextAlign;
pub use crate::TextRun;
pub mod character_options;
use crate::{widgets::Rect, Command, Screen};
pub use dereth_client_contract::{
    pregame::PregameView,
    view::{GameView, UiRequest},
};
pub use dereth_primitives::{DataId, ObjectId};

pub struct Context<'a> {
    pub game: &'a dyn GameView,
    pub pregame: &'a PregameView,
    pub keyboard: &'a KeyboardState,
    pub settings: &'a ClassicSettings,
    pub map_teleport_allowed: bool,
    pub classic: &'a ClassicState,
}
impl std::fmt::Debug for Context<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Context")
            .field("map_teleport_allowed", &self.map_teleport_allowed)
            .finish_non_exhaustive()
    }
}
#[derive(Clone, Debug)]
pub struct ClassicState {
    pub reply_targets: dereth_client_contract::chat::window::ReplyTargets,
    pub chat_focus: Option<(u8, [bool; 14])>,
    pub chat_target: Option<(ObjectId, String)>,
    pub classic_power_level: Option<f32>,
    pub stack_split: Option<(u32, u32)>,
    pub equipment_priority: std::collections::BTreeMap<ObjectId, u32>,
    pub cursor_mode: u32,
    pub active_right: String,
    /// The welcome text the character screen's message box shows, if this client has one.
    pub welcome: String,
    pub active_bottom: String,
    pub book_edit_privileged: bool,
    pub portraits: std::collections::BTreeMap<ObjectId, ClassicPortrait>,
    pub option_words: [u32; 2],
    pub timestamp_format: String,
    pub abuse_response: Option<String>,
    pub game_status: String,
    pub chat: Vec<(u32, String)>,
    /// The character sheet's augmentation and luminance section, composed from the world's string
    /// tables; empty when the world's era has neither.
    pub augmentations: String,
}
#[derive(Clone, Debug)]
pub struct ClassicPortrait {
    pub textures: [u32; 3],
    pub palettes: [u32; 3],
}
impl Default for ClassicState {
    fn default() -> Self {
        Self {
            reply_targets: Default::default(),
            stack_split: None,
            chat_focus: None,
            chat_target: None,
            classic_power_level: None,
            equipment_priority: Default::default(),
            cursor_mode: 0,
            active_right: String::new(),
            welcome: String::new(),
            active_bottom: String::new(),
            book_edit_privileged: false,
            portraits: std::collections::BTreeMap::new(),
            option_words: crate::screens::DEFAULT_WORDS,
            timestamp_format: String::new(),
            abuse_response: None,
            game_status: String::new(),
            chat: vec![],
            augmentations: String::new(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClassicSettings {
    pub sound_available: bool,
    pub detail_available: bool,
    pub hardware_acceleration: bool,
    pub resolutions: Vec<(u32, u32)>,
    pub resolution: usize,
    pub stereo: bool,
    pub effects: bool,
    pub ambient: bool,
    pub interface: bool,
    pub effects_volume: f32,
    pub ambient_volume: f32,
    pub brightness: f32,
    pub camera_stiffness: f32,
    pub performance: f32,
    pub auto_degrade: bool,
    pub landscape_detail: bool,
    pub environment_detail: bool,
    /// The four texture sizes a settings file left by the classic interface kept, as its steps
    /// (0 full size .. 3 the smallest); read only to carry such a file into the profile.
    pub texture_levels: [u8; 4],
    /// Whether the game fills the monitor (a borderless window over it) rather than a window.
    pub full_screen: bool,
}

#[derive(Clone, Debug, Default)]
pub struct KeyboardState {
    pub capture_revision: u64,
    pub dirty: bool,
    pub warning: Option<String>,
    pub scheme: u32,
    pub schemes: Vec<String>,
    pub bindings: Vec<KeyBinding>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyBinding {
    pub action: u32,
    pub map: u32,
    pub label: String,
    pub keys: Vec<String>,
}
/// How a line of local feedback is shown: information, or a warning.
pub use dereth_client_contract::feedback::FeedbackSeverity;

#[derive(Clone, Debug, PartialEq)]
pub enum HostAction {
    /// Open the key page on the character screen once the world has been left, as the character
    /// screen's own keyboard button does.
    KeyboardOnLeaving,
    /// The spell research page's Test: the formula's components (as component class ids, in
    /// order) tried on the target.
    TestSpellFormula {
        components: Vec<u32>,
    },
    OverwriteKeyMap {
        name: String,
    },
    Quit,
    LegacyHelp(u32),
    PrintLegacyHelp {
        context: u32,
        topic: u32,
    },
    ClassicShortcutDrop {
        object: ObjectId,
        slot: u32,
        from: Option<u32>,
    },
    FocusControl(String),
    /// Begin a tell to the named character in the chat entry: `@tell <name>, ` typed there and
    /// the entry focused, ready for the message.
    StartTell(String),
    ConfirmBinding(bool),
    DialogAnswer {
        id: String,
        accepted: bool,
    },
    CombatMode(u32),
    LocalFeedback {
        text: String,
        severity: crate::panels::FeedbackSeverity,
    },
    SplitForPanel {
        object: ObjectId,
        amount: u32,
    },
    SocialTarget(u8),
    CharacterOptions {
        words: [u32; 2],
        timestamp_format: String,
        save: bool,
    },
    ClearBindingSlot {
        action: u32,
        map: u32,
        slot: usize,
    },
    CancelBindingCapture,
    AllegianceSend {
        action: dereth_client_contract::view::AllegianceAction,
        target: ObjectId,
    },
    MapTeleport {
        lx: u32,
        ly: u32,
    },
    VendorSellAll,
    CloseVendorForced,
    CloseGroundForced,
    SaveKeyMapAs {
        name: String,
    },
    DeleteKeyScheme {
        name: String,
    },
    ApplyClassicSettings(ClassicSettings),
    DefaultClassicSettings(ClassicSettings),
    PreviewClassicSettings(ClassicSettings),
    ResetClassicSettings,
    KeyboardScheme(u32),
    CaptureBinding {
        action: u32,
        map: u32,
        slot: usize,
    },
    ClearBinding {
        action: u32,
        map: u32,
    },
    RestoreBindings,
    QueryHouse,
    OpenTrade(ObjectId),
    AbuseLog {
        target: String,
        enabled: bool,
        complaint: String,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum PanelAction {
    Toggle(String),
    BeginDrag(DragPayload),
    Control(ControlEvent),
    Host(HostAction),
    Game(UiRequest),
    Open(String),
    OpenObject {
        id: String,
        object: ObjectId,
    },
    OpenSpell {
        id: String,
        spell: u32,
    },
    Close,
    Question {
        id: String,
        text: String,
        accept: Vec<PanelAction>,
        reject: Vec<PanelAction>,
    },
    Message {
        id: String,
        text: String,
        accept: Vec<PanelAction>,
    },
    Confirm {
        id: String,
        text: String,
        accept: Vec<PanelAction>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum DragPayload {
    Object(ObjectId),
    Shortcut { object: ObjectId, from: u32 },
    Spell(u32),
    Text(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum ControlEvent {
    ChatEntry(dereth_client_contract::chat::entry::EntryUpdate),
    Magic(dereth_client_contract::view::MagicNotice),
    Salvage(dereth_client_contract::panels::salvage::SalvageNotice),
    HousePaymentConfirmation {
        rent: bool,
        confirmed: Option<bool>,
    },
    Held {
        id: String,
        pressed: bool,
    },
    PreviewDrag {
        equipment_mask: u32,
    },
    PreviewHit {
        object_index: u32,
        part_index: u32,
        equipment_mask: u32,
        right_click: bool,
        double_click: bool,
    },
    Action(String),
    Submit {
        id: String,
    },
    DropStack {
        id: String,
        object: ObjectId,
        amount: u32,
        max_amount: u32,
        slot: u32,
    },
    SplitReady(ObjectId),
    SplitFailed,
    KeyPressed,
    WorldTarget(Option<ObjectId>),
    Pointer {
        x: i32,
        y: i32,
        pressed: bool,
    },
    RightClick {
        id: String,
        index: usize,
    },
    DragStart {
        id: String,
        index: usize,
    },
    Commit {
        id: String,
    },
    Activate(String),
    Check {
        id: String,
        checked: bool,
    },
    Edit {
        id: String,
        text: String,
    },
    Select {
        id: String,
        index: usize,
    },
    DoubleClick {
        id: String,
        index: usize,
    },
    Value {
        id: String,
        value: i32,
    },
    Scroll {
        id: String,
        value: i32,
    },
    Drop {
        id: String,
        payload: DragPayload,
        slot: u32,
    },
    Tick,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ListRow {
    pub text: String,
    pub icon: Option<DataId>,
    pub color: u32,
}
impl From<String> for ListRow {
    fn from(text: String) -> Self {
        Self {
            text,
            icon: None,
            color: 0xffd2d2c8,
        }
    }
}
impl From<&str> for ListRow {
    fn from(text: &str) -> Self {
        text.to_owned().into()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ItemEntry {
    pub decoration: Option<dereth_client_contract::view::SlotDecoration>,
    pub id: ObjectId,
    pub icon: Option<DataId>,
    pub caption: String,
    pub count: u32,
    /// Explicit classic widget amount; ordinary inventory stacks leave this absent.
    pub amount: Option<u32>,
    pub active_container: bool,
    pub disabled: bool,
}

impl ItemEntry {
    /// Object zero is the native empty slot, never a selectable or draggable item.
    pub fn empty() -> Self {
        Self {
            id: ObjectId(0),
            icon: None,
            decoration: None,
            caption: String::new(),
            count: 0,
            amount: None,
            active_container: false,
            disabled: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ControlKind {
    ScrollBar {
        min: i32,
        max: i32,
        value: i32,
        page: i32,
        step: i32,
        vertical: bool,
        arrow_size: i32,
        thumb_size: i32,
    },
    Choice {
        options: Vec<String>,
        selected: usize,
    },
    ItemStrip {
        entries: Vec<ItemEntry>,
        slot_size: i32,
        selected: Option<ObjectId>,
        offset: i32,
    },
    Button {
        caption: String,
    },
    Check {
        caption: String,
        checked: bool,
    },
    Edit {
        text: String,
        max_chars: usize,
        multiline: bool,
    },
    List {
        rows: Vec<ListRow>,
        selected: Option<usize>,
        row_height: i32,
    },
    /// Worker paints its own cells; the host supplies clipping, selection and scrolling input.
    HitList {
        row_count: usize,
        row_height: i32,
        selected: Option<usize>,
        offset: i32,
    },
    Slider {
        min: i32,
        max: i32,
        value: i32,
        step: i32,
    },
    Items {
        entries: Vec<ItemEntry>,
        columns: u32,
        slot_size: i32,
        selected: Option<ObjectId>,
    },
}

/// Which items an item strip takes, for its drag hints.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropFilter {
    /// The salvage list: items the player owns that suit salvaging alongside those already
    /// offered, whose material the first offered item sets (`0` when the list is empty).
    Salvage { material: u32 },
    /// The trade window's own side: items the player may offer.
    Trade,
}

#[derive(Clone, Debug)]
pub struct Control {
    pub silent: bool,
    pub drop_location: Option<u32>,
    pub drop_equipment_canvas: bool,
    /// For an item strip that takes only some items, the rule its drag hints follow: an item it
    /// takes lights any slot as a place to drop, any other item as refused.
    pub drop_filter: Option<DropFilter>,
    pub choice_enabled: Option<Vec<bool>>,
    pub smooth_scroll: bool,
    pub capture_edges: bool,
    pub background: Option<u32>,
    pub paint: bool,
    pub id: String,
    pub rect: Rect,
    pub kind: ControlKind,
    pub enabled: bool,
    /// Normal/pressed/disabled image overrides. None selects the established theme.
    pub images: Option<[String; 3]>,
    pub endcaps: Option<[String; 3]>,
    /// Whether the button's images are drawn with black as a colour key. The creation wizard's
    /// picture buttons are drawn as they are, black included.
    pub keyed: bool,
    /// An edit field that selects all its text when it gains the focus.
    pub select_on_focus: bool,
    /// A dropdown whose list is the chat's destination list art.
    pub chat_popup: bool,
    /// A dropdown drawn from the interface art, as the creation screens' are: a 117x25 face on
    /// the row image with the 26-pixel arrow to its right, and a popup of 25-pixel row images
    /// between an 8-pixel top and bottom edge. Other dropdowns are plain black rows.
    pub choice_art: bool,
    /// A button that is also a slot: it can be dragged from and double-clicked, as one entry.
    pub slot: bool,
    /// A drop-down drawn from one of the interface's list art sets (face, arrow, rows, edges).
    pub list_skin: Option<ListSkin>,
    pub font: String,
    pub color: u32,
}

/// The art of a drop-down list: the face and each popup row are a row image (and a lit one for
/// the row under the pointer); the arrow sits at the face's right end; the popup may have a top
/// and a bottom edge. The control's rectangle covers the face and the arrow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ListSkin {
    pub row: &'static str,
    pub row_lit: &'static str,
    pub arrow: &'static str,
    pub arrow_pressed: &'static str,
    pub arrow_width: i32,
    pub arrow_height: i32,
    pub top: Option<(&'static str, i32)>,
    pub bottom: Option<(&'static str, i32)>,
}
impl ListSkin {
    /// The vendor's category list: green rows, no popup edges.
    pub const VENDOR: Self = Self {
        row: "060012B3",
        row_lit: "060012B4",
        arrow: "060012B1",
        arrow_pressed: "060012B2",
        arrow_width: 17,
        arrow_height: 19,
        top: None,
        bottom: None,
    };
    /// The options pages' lists (window size, stereo).
    pub const OPTIONS: Self = Self {
        row: "06001287",
        row_lit: "06001289",
        arrow: "06001274",
        arrow_pressed: "06001275",
        arrow_width: 20,
        arrow_height: 19,
        top: Some(("06001281", 3)),
        bottom: Some(("06001288", 5)),
    };
    /// A book's page list.
    pub const BOOK: Self = Self {
        row: "06001276",
        row_lit: "0600127A",
        arrow: "06001274",
        arrow_pressed: "06001275",
        arrow_width: 20,
        arrow_height: 19,
        top: Some(("06001278", 3)),
        bottom: Some(("06001277", 5)),
    };
}

#[derive(Clone, Debug)]
pub struct PanelFrame {
    pub screen: Screen,
    pub controls: Vec<Control>,
    pub previews: Vec<Preview>,
}
#[derive(Clone, Debug)]
pub struct Preview {
    pub kind: PreviewKind,
    pub rect: Rect,
    pub object: Option<ObjectId>,
    pub appearance: Option<Appearance>,
}
#[derive(Clone, Debug)]
pub struct Appearance {
    pub animation: DataId,
    pub state: dereth_chargen::CharGenState,
    pub tables: std::rc::Rc<dereth_chargen::CreationTables>,
    pub rotation_velocity: f32,
    pub zoom_face: bool,
    pub heading_degrees: f32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreviewKind {
    CharGen,
    PaperDoll,
    Examine,
}
impl PanelFrame {
    pub fn preview(&mut self, preview: Preview) {
        self.screen.commands.push(Command::Preview {
            index: self.previews.len(),
        });
        self.previews.push(preview);
    }
    pub fn rich_text_box(
        &mut self,
        rect: Rect,
        runs: Vec<TextRun>,
        font: &str,
        align: TextAlign,
        wrap: bool,
        clip: Option<[i32; 4]>,
    ) {
        self.screen.commands.push(Command::RichTextBox {
            runs,
            rect: [rect.x, rect.y, rect.w, rect.h],
            font: font.into(),
            align,
            wrap,
            clip,
        });
    }
    #[allow(clippy::too_many_arguments)] // one field per argument of the drawn command
    pub fn text_box(
        &mut self,
        rect: Rect,
        text: impl Into<String>,
        font: &str,
        color: u32,
        align: TextAlign,
        wrap: bool,
        clip: Option<[i32; 4]>,
    ) {
        self.screen.commands.push(Command::TextBox {
            text: text.into(),
            rect: [rect.x, rect.y, rect.w, rect.h],
            font: font.into(),
            color,
            align,
            wrap,
            clip,
        });
    }
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            screen: Screen {
                width,
                height,
                commands: vec![],
            },
            controls: vec![],
            previews: vec![],
        }
    }
    pub fn control(
        &mut self,
        id: impl Into<String>,
        rect: Rect,
        kind: ControlKind,
        enabled: bool,
    ) -> &mut Control {
        self.controls.push(Control {
            silent: false,
            drop_location: None,
            drop_equipment_canvas: false,
            drop_filter: None,
            choice_enabled: None,
            smooth_scroll: false,
            background: Some(0xff000000),
            paint: true,
            capture_edges: false,
            id: id.into(),
            rect,
            kind,
            enabled,
            images: None,
            endcaps: None,
            keyed: true,
            select_on_focus: false,
            chat_popup: false,
            choice_art: false,
            slot: false,
            list_skin: None,
            font: "15-6".into(),
            color: 0xffd2d2c8,
        });
        self.controls.last_mut().unwrap()
    }
    pub fn button(
        &mut self,
        id: impl Into<String>,
        rect: Rect,
        caption: impl Into<String>,
        enabled: bool,
    ) -> &mut Control {
        let c = self.control(
            id,
            rect,
            ControlKind::Button {
                caption: caption.into(),
            },
            enabled,
        );
        c.font = "16-7".into();
        c
    }
    pub fn check(
        &mut self,
        id: impl Into<String>,
        rect: Rect,
        caption: impl Into<String>,
        checked: bool,
        enabled: bool,
    ) -> &mut Control {
        self.control(
            id,
            rect,
            ControlKind::Check {
                caption: caption.into(),
                checked,
            },
            enabled,
        )
    }
    pub fn edit(
        &mut self,
        id: impl Into<String>,
        rect: Rect,
        text: impl Into<String>,
        max_chars: usize,
        multiline: bool,
        enabled: bool,
    ) -> &mut Control {
        self.control(
            id,
            rect,
            ControlKind::Edit {
                text: text.into(),
                max_chars,
                multiline,
            },
            enabled,
        )
    }
    pub fn list(
        &mut self,
        id: impl Into<String>,
        rect: Rect,
        rows: Vec<ListRow>,
        selected: Option<usize>,
        row_height: i32,
    ) -> &mut Control {
        self.control(
            id,
            rect,
            ControlKind::List {
                rows,
                selected,
                row_height,
            },
            true,
        )
    }
    pub fn slider(
        &mut self,
        id: impl Into<String>,
        rect: Rect,
        min: i32,
        max: i32,
        value: i32,
        step: i32,
    ) -> &mut Control {
        self.control(
            id,
            rect,
            ControlKind::Slider {
                min,
                max,
                value,
                step,
            },
            true,
        )
    }
    pub fn label(
        &mut self,
        x: i32,
        y: i32,
        text: impl Into<String>,
        font: &str,
        color: u32,
        clip: Option<[i32; 4]>,
    ) {
        self.screen.commands.push(Command::Text {
            text: text.into(),
            x,
            y,
            font: font.into(),
            color,
            clip,
        });
    }
    pub fn image(&mut self, did: &str, rect: Rect, tile: bool, keyed: bool) {
        self.screen.commands.push(Command::Image {
            did: did.into(),
            x: rect.x,
            y: rect.y,
            width: rect.w.max(0) as u32,
            height: rect.h.max(0) as u32,
            tile,
            clip: None,
            color_key: keyed.then_some([0, 0, 0]),
            key_bits: keyed.then_some([5, 6, 5]),
        });
    }
    /// Draw `did` at its own size with its top left at `(x, y)`, cut to `clip`.
    pub fn image_native(&mut self, did: &str, x: i32, y: i32, clip: Rect, keyed: bool) {
        self.screen.commands.push(Command::Image {
            did: did.into(),
            x,
            y,
            width: 0,
            height: 0,
            tile: false,
            clip: Some([clip.x, clip.y, clip.x + clip.w, clip.y + clip.h]),
            color_key: keyed.then_some([0, 0, 0]),
            key_bits: keyed.then_some([5, 6, 5]),
        });
    }
    pub fn fill(&mut self, rect: Rect, color: u32) {
        self.screen.commands.push(Command::Fill {
            x: rect.x,
            y: rect.y,
            width: rect.w.max(0) as u32,
            height: rect.h.max(0) as u32,
            color,
        });
    }
}

pub trait Panel: std::fmt::Debug {
    fn pointer_art(&self, _x: i32, _y: i32) -> Option<(String, u32, u32)> {
        None
    }
    fn input(
        &mut self,
        _input: &crate::widgets::Input,
        _context: &Context<'_>,
    ) -> Option<Vec<PanelAction>> {
        None
    }
    fn resize(&mut self, _width: u32, _height: u32) {}
    /// Whether a point of a window that lets the pointer through where it has no control (the
    /// full-screen interface) is still the panel's: text it draws that the pointer can select.
    fn claims(&self, _x: i32, _y: i32) -> bool {
        false
    }
    /// What the window has selected outside its edit fields, for the copy key.
    fn selected_text(&self) -> Option<String> {
        None
    }
    fn set_object(&mut self, _object: ObjectId) {}
    fn set_spell(&mut self, _spell: u32) {}
    fn id(&self) -> &'static str;
    fn frame(&self, context: &Context<'_>) -> PanelFrame;
    fn event(&mut self, event: ControlEvent, context: &Context<'_>) -> Vec<PanelAction>;
}

pub const fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

pub mod pregame;

pub mod game;
pub mod hud;

pub mod services;

static SIDE_HEIGHT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(362);

/// The side panel's page height: 362, or the window's height less 118 with the stretched
/// interface. Pages lay themselves out to it.
#[must_use]
pub fn side_height() -> u32 {
    SIDE_HEIGHT.load(std::sync::atomic::Ordering::Relaxed)
}

/// Set the side panel's page height (see [`side_height`]).
pub fn set_side_height(height: u32) {
    SIDE_HEIGHT.store(height.max(362), std::sync::atomic::Ordering::Relaxed);
}

/// The background of an options-style sub-page `height` tall: the parchment tiled inside a
/// four-pixel border on the left, right and bottom.
pub fn sub_page_background(f: &mut PanelFrame, height: i32) {
    f.image("0600128A", rect(4, 0, 292, height - 4), true, false);
    for (did, r) in [
        ("060012BC", rect(0, 0, 4, height - 4)),
        ("060012BD", rect(296, 0, 4, height - 4)),
        ("060012BB", rect(0, height - 4, 300, 4)),
    ] {
        f.image(did, r, true, false);
    }
}

/// The parts of an options window's scrolling page, `height` tall (the side panel's height less
/// the tabs): the list runs from under the tabs down to the buttons, which keep to the bottom, so a
/// taller page (the stretched interface) shows more of the list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OptionsPage {
    /// Where the list is seen, in page coordinates.
    pub view: Rect,
    /// The top of the row of buttons under the list.
    pub buttons_y: i32,
}

impl OptionsPage {
    /// The page for a side panel page `height` tall.
    #[must_use]
    pub fn new(height: i32) -> Self {
        Self {
            view: rect(4, 16, 280, (height - 73).max(20)),
            buttons_y: height - 40,
        }
    }
    /// The page as the side panel is now.
    #[must_use]
    pub fn current() -> Self {
        Self::new(i32::try_from(side_height()).unwrap_or(362) - 25)
    }
    /// The view as a clip rectangle.
    #[must_use]
    pub fn clip(&self) -> [i32; 4] {
        let v = self.view;
        [v.x, v.y, v.x + v.w, v.y + v.h]
    }
    /// How far a list `content` tall scrolls in the view.
    #[must_use]
    pub fn max_scroll(&self, content: i32) -> i32 {
        (content - self.view.h).max(0)
    }
    /// The page's background and the inset frame around its list: a rule above it and a rule
    /// below it.
    pub fn background(&self, f: &mut PanelFrame) {
        let bottom = self.view.y + self.view.h;
        f.image("060012C4", rect(0, 8, 300, 8), true, false);
        f.image("060012C4", rect(4, bottom, 296, 4), true, false);
    }
    /// The list's scroll bar, beside the view.
    pub fn scroll_bar(&self, f: &mut PanelFrame, id: &str, content: i32, value: i32, step: i32) {
        f.control(
            id,
            rect(284, self.view.y, 16, self.view.h),
            ControlKind::ScrollBar {
                min: 0,
                max: self.max_scroll(content),
                value: value.clamp(0, self.max_scroll(content)),
                page: self.view.h,
                step,
                vertical: true,
                arrow_size: 16,
                thumb_size: 16,
            },
            true,
        );
    }
    /// One of the buttons under the list, in the `slot`th of its three places (Apply, Reset,
    /// Defaults), drawn the options window's way.
    pub fn button(&self, f: &mut PanelFrame, slot: i32, id: &str, caption: &str, enabled: bool) {
        let c = f.button(
            id,
            rect(25 + 85 * slot, self.buttons_y, 80, 36),
            caption,
            enabled,
        );
        c.images = Some(["06001207", "06001208", "0600120A"].map(String::from));
        c.endcaps = Some(["06001206", "06001209", "06001205"].map(String::from));
    }
    /// Whether a row at `y` (page coordinates, `h` tall) is wholly in the view.
    #[must_use]
    pub fn shows(&self, y: i32, h: i32) -> bool {
        y >= self.view.y && y + h <= self.view.y + self.view.h
    }
}

/// Configure Keyboard, from the world: the key page is the character screen's, so it asks to leave
/// the world first, and the page opens on the character screen once the world has been left.
#[must_use]
pub fn configure_keyboard() -> PanelAction {
    PanelAction::Confirm {
        id: "configure-keyboard".into(),
        text: "\n\nTo configure your keyboard, you need to leave the world.\nProceed?".into(),
        accept: vec![
            PanelAction::Game(UiRequest::EndCharacterSession { ask: false }),
            PanelAction::Host(HostAction::KeyboardOnLeaving),
        ],
    }
}

/// Every classic panel, by id.
#[must_use]
pub fn factory(id: &str) -> Option<Box<dyn Panel>> {
    if id == "character-options" {
        return Some(Box::new(character_options::CharacterOptions::new()));
    }
    pregame::make(id)
        .or_else(|| hud::make(id))
        .or_else(|| game::make(id))
        .or_else(|| services::make(id))
        .or_else(|| crate::help::make(id))
}
