//! Character creation's screens: choosing the avatar (race and sex) in a hall of them, its
//! appearance beside a torch-lit figure, and over that the name with the last confirmation.
//!
//! - **The hall** ([`AvatarSelection`]): every avatar standing in the hall's scene. The pointer
//!   over one asks the scene which it is: its race and sex described at the top, "Click to choose
//!   this avatar." beside the pointer. A press on one has it play its leaving behavior, chooses it,
//!   and once the behavior is done goes on to its appearance. Cancel goes back to the list.
//! - **The appearance** ([`AvatarModification`]): the chosen avatar in its race's scene, a page of
//!   rows (head, body, clothing), a row per kind of choice the race has, each a label and a slider
//!   with a tooltip and a button each side; Randomize this page or every page; the figure turned by
//!   the rotation slider or by dragging over it; Back to the hall; Next asks the name over the
//!   screen.
//! - **The name** ([`GenerationConfirmation`]): a panel of the appearance's: the name typed (only
//!   the letters a name may hold), Enter makes the character and enters the world with it (Create,
//!   an administrator's, makes it and goes back to the list); Cancel takes the panel down. Why the
//!   server refused shows under the name.

use std::any::Any;

use auberean_text::{StringInfo, Value as Var};
use auberean_ui::framework::FrameId;
use auberean_ui::manager::global;
use auberean_ui::token::{attr, msg, state};
use auberean_ui::types::text::text_msg;
use auberean_ui::{Handle, Mail, Token, Ui, Value};

use crate::flow::mode;
use crate::pregame::{self, message, Avatar, Data, Notice, Request, Screen};

/// The hall's layout, by enumeration value (record `0x22000006`).
pub const AVATAR_SELECTION_LAYOUT_ENUM: u32 = 0x4100_000A;
/// The appearance's layout (record `0x22000007`).
pub const AVATAR_MODIFICATION_LAYOUT_ENUM: u32 = 0x4100_000B;
/// The name's layout (record `0x22000008`).
pub const CONFIRMATION_LAYOUT_ENUM: u32 = 0x4100_000C;

/// The behavior a chosen avatar plays leaving the hall.
pub const LEAVE_BEHAVIOR: u32 = 0x4100_0005;

/// The race flags only some accounts may make (the server says which): drudges, empyreans.
pub const DRUDGE: u32 = 0x8;
pub const EMPYREAN: u32 = 0x10;

mod name {
    use auberean_ui::Token;
    pub const AVATAR_SELECTION_VIEWPORT: Token = Token::of("AvatarSelectionViewport");
    pub const AVATAR_SELECTION_FIELD_TOP: Token = Token::of("AvatarSelectionFieldTop");
    pub const AVATAR_SELECTION_FIELD_BOTTOM: Token = Token::of("AvatarSelectionFieldBottom");
    pub const RACE_DESC_FIELD: Token = Token::of("RaceDescField");
    pub const PRIM_SONG: Token = Token::of("primSong");
    pub const AVATAR_MODIFICATION_FIELD: Token = Token::of("AvatarModificationField");
    pub const TORCH_VIEWPORT: Token = Token::of("TorchViewport");
    pub const MOUSE_ROTATION_OVER_FIELD: Token = Token::of("MouseRotationOverField");
    pub const HEAD_FIELD: Token = Token::of("HeadField");
    pub const BODY_FIELD: Token = Token::of("BodyField");
    pub const CLOTHING_FIELD: Token = Token::of("ClothingField");
    pub const HEAD_BUTTON: Token = Token::of("HeadButton");
    pub const BODY_BUTTON: Token = Token::of("BodyButton");
    pub const CLOTHING_BUTTON: Token = Token::of("ClothingButton");
    pub const BACK_BUTTON: Token = Token::of("BackButton");
    pub const NEXT_BUTTON: Token = Token::of("NextButton");
    pub const CANCEL_BUTTON: Token = Token::of("CancelButton");
    pub const ZOOM_BUTTON: Token = Token::of("ZoomButton");
    pub const RANDOMIZE_ALL_BUTTON: Token = Token::of("RandomizeAllButton");
    pub const RANDOMIZE_CURRENT_BUTTON: Token = Token::of("RandomizeCurrentButton");
    pub const ROTATION_SLIDER: Token = Token::of("RotationSlider");
    pub const GENERATION_CONFIRMATION_FIELD: Token = Token::of("GenerationConfirmationField");
    pub const CREATE_BUTTON: Token = Token::of("CreateButton");
    pub const ENTER_BUTTON: Token = Token::of("EnterButton");
    pub const ADMIN_BUTTON: Token = Token::of("AdminButton");
    pub const NAME_ENTRY: Token = Token::of("NameEntry");
    pub const BUTTON_FIELD: Token = Token::of("ButtonField");
    /// The player's client: enter and cancel.
    pub const NON_ADMIN_CLIENT: Token = Token::of("nonadminclient");
    /// An administrator's client: the administrator's button too.
    pub const ADMIN_CLIENT: Token = Token::of("adminclient");
    pub const ERROR_MSG: Token = Token::of("GenerationConfirmationErrorMsg");
    pub const MUSIC: Token = Token::of("music");
}

/// The races' and sexes' names, as the hall's description and the appearance's scene name them.
pub const RACE_NAMES: [(u32, &str); 5] = [
    (1, "human"),
    (2, "lugian"),
    (4, "tumerok"),
    (DRUDGE, "drudge"),
    (EMPYREAN, "empyrean"),
];
pub const SEX_NAMES: [(u32, &str); 2] = [(0x1000, "male"), (0x2000, "female")];

fn race_name(race: u32) -> Option<&'static str> {
    RACE_NAMES.iter().find(|(r, _)| *r == race).map(|(_, n)| *n)
}

/// The hall's description of a race and sex: the state of its description field.
#[must_use]
pub fn description_state(a: Avatar) -> Option<Token> {
    let race = race_name(a.race)?;
    let sex = SEX_NAMES.iter().find(|(s, _)| *s == a.sex)?.1;
    Some(Token::of(&format!("{race}{sex}")))
}

/// The appearance's scene viewport for a race: `<race>Viewport`.
#[must_use]
pub fn race_viewport(race: u32) -> Option<Token> {
    race_name(race).map(|r| Token::of(&format!("{r}Viewport")))
}

/// The hall: every avatar in its scene, and its frame.
#[derive(Debug)]
pub struct AvatarSelection {
    frame: FrameId,
    viewport: Option<Handle>,
    top: Option<Handle>,
    bottom: Option<Handle>,
    /// The figure under the pointer (none), a selection asked of the scene, a chosen figure's
    /// behavior awaited.
    under: Option<usize>,
    selecting: bool,
    wait_for_anim: bool,
    wait_context: u32,
}

impl AvatarSelection {
    /// The hall made: its viewport, its frame and Cancel, the pointer's moves and presses over the
    /// scene heard, Cancel heard, the clock heard; creation under way (the client keeps an avatar
    /// of every race and sex); the hall's song.
    // D2-01854, D2-01856, D2-01858, D2-01849
    pub fn new(ui: &mut Ui, data: &mut Data, out: &mut Vec<Request>) -> Self {
        let frame = ui.new_main_frame();
        let l = AVATAR_SELECTION_LAYOUT_ENUM;
        let viewport = ui.frame_create_root(frame, l, name::AVATAR_SELECTION_VIEWPORT);
        let top = ui.frame_create_root(frame, l, name::AVATAR_SELECTION_FIELD_TOP);
        let bottom = ui.frame_create_root(frame, l, name::AVATAR_SELECTION_FIELD_BOTTOM);
        for (e, w) in [
            (name::AVATAR_SELECTION_VIEWPORT, msg::MOUSE_PRESS_TOP),
            (name::AVATAR_SELECTION_VIEWPORT, msg::MOUSE_MOVE_TOP),
            (name::CANCEL_BUTTON, msg::BUTTON_RELEASE_ON),
        ] {
            pregame::listen(ui, frame, e, w);
        }
        pregame::listen_global(ui, frame, global::TIME);
        data.in_creation = true;
        crate::intro::play_music(ui, bottom, name::PRIM_SONG, out);
        Self {
            frame,
            viewport,
            top,
            bottom,
            under: None,
            selecting: false,
            wait_for_anim: false,
            wait_context: 0,
        }
    }

    /// The hall's viewport.
    #[must_use]
    pub const fn viewport(&self) -> Option<Handle> {
        self.viewport
    }

    /// The figure under the pointer, as the scene last said.
    #[must_use]
    pub const fn under(&self) -> Option<usize> {
        self.under
    }

    /// The hall's fields: its top (Cancel) and its foot (the description).
    #[must_use]
    pub const fn fields(&self) -> (Option<Handle>, Option<Handle>) {
        (self.top, self.bottom)
    }

    // D2-01825
    fn make_please_wait_dialog(&mut self, ui: &Ui, out: &mut Vec<Request>) {
        self.wait_context = pregame::please_wait(ui, self.frame, out);
    }

    /// The pointer left a figure: no tooltip, no description.
    // D2-01838
    fn mouse_off_object(&mut self, ui: &mut Ui) {
        if let Some(v) = self.viewport {
            ui.clear_element_tooltip(v);
            if let Some(d) = ui.find(name::RACE_DESC_FIELD) {
                ui.set_state(d, state::STATELESS);
            }
        }
    }

    /// The pointer came over a figure: the tooltip that says a press chooses it, and its race and
    /// sex described.
    // D2-01839
    fn mouse_on_object(&mut self, ui: &mut Ui, data: &Data) {
        let Some(figure) = self.under else { return };
        let Some(&avatar) = data.hall.get(figure) else {
            return;
        };
        let tip = message(ui, "ID_AvSelect_TT_3DAvatar", &[]);
        if let Some(v) = self.viewport {
            ui.set_tooltip(v, &tip);
            if let (Some(s), Some(d)) = (description_state(avatar), ui.find(name::RACE_DESC_FIELD))
            {
                ui.set_state(d, s);
            }
        }
    }

    /// What the hall heard: the pointer moving over the scene (a selection asked, unless one is
    /// or a chosen figure is leaving); a press on the figure under the pointer (it plays its
    /// leaving behavior, chosen); Cancel (please wait, back to the list).
    // D2-01841
    fn listen_to_message(
        &mut self,
        ui: &mut Ui,
        data: &mut Data,
        element: Token,
        what: Token,
        out: &mut Vec<Request>,
    ) {
        if self.viewport.is_none() {
            return;
        }
        if element == name::AVATAR_SELECTION_VIEWPORT {
            if what == msg::MOUSE_MOVE_TOP {
                if self.selecting || self.wait_for_anim {
                    return;
                }
                self.selecting = true;
                data.selection = None;
                out.push(Request::FindUnderPointer);
            }
            if what == msg::MOUSE_PRESS_TOP {
                if let Some(figure) = self.under {
                    if let Some(&avatar) = data.hall.get(figure) {
                        out.push(Request::Behave {
                            figure,
                            behavior: LEAVE_BEHAVIOR,
                        });
                        data.behaving.push(figure);
                        self.wait_for_anim = true;
                        data.avatar = Some(avatar);
                        out.push(Request::Choose(avatar));
                    }
                }
            }
        }
        if element == name::CANCEL_BUTTON {
            self.make_please_wait_dialog(ui, out);
            pregame::queue(mode::CHARACTER_SELECTION, out);
        }
    }

    /// The clock's tick: the scene's answer to the selection (the figure under the pointer, the
    /// one before left); a chosen figure's behavior done: please wait, on to its appearance.
    // D2-01842
    fn loop_tick(&mut self, ui: &mut Ui, data: &mut Data, out: &mut Vec<Request>) {
        if self.selecting && !self.wait_for_anim {
            let Some(found) = data.selection.take() else {
                return;
            };
            self.selecting = false;
            if found == self.under {
                return;
            }
            if self.under.is_some() {
                self.mouse_off_object(ui);
            }
            self.under = found;
            data.under = found;
            if self.under.is_some() {
                self.mouse_on_object(ui, data);
            }
        }
        if !self.wait_for_anim {
            return;
        }
        if let Some(figure) = self.under {
            if !data.behaving.contains(&figure) {
                self.wait_for_anim = false;
                self.make_please_wait_dialog(ui, out);
                pregame::queue(mode::AVATAR_MODIFICATION, out);
            }
        }
    }
}

impl Screen for AvatarSelection {
    fn frame(&self) -> FrameId {
        self.frame
    }

    fn tick(&mut self, ui: &mut Ui, data: &mut Data, out: &mut Vec<Request>) {
        let mut clock = false;
        for m in pregame::take(ui, self.frame) {
            match m {
                Mail::Element { element, what, .. } => {
                    self.listen_to_message(ui, data, element, what, out);
                }
                Mail::Global {
                    what: global::TIME, ..
                } => clock = true,
                _ => {}
            }
        }
        if clock {
            self.loop_tick(ui, data, out);
        }
    }

    /// The hall goes: its selection forgotten, its elements, its please wait.
    // D2-01855, D2-01857
    fn close(mut self: Box<Self>, ui: &mut Ui, data: &mut Data, out: &mut Vec<Request>) {
        self.selecting = false;
        data.under = None;
        data.behaving.clear();
        ui.drop_frame(self.frame);
        pregame::dialog_down(self.wait_context, out);
        self.wait_context = 0;
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// The appearance's three pages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Head,
    Body,
    Clothing,
}

impl Page {
    /// The physique types the page shows.
    // D2-01861
    #[must_use]
    pub const fn mask(self) -> u32 {
        match self {
            Self::Head => 0x1870,
            Self::Body => 0x8F,
            Self::Clothing => 0x700,
        }
    }
}

/// Every physique type at once (Randomize All).
pub const ALL: u32 = 0xFFFF_FFFF;

/// The appearance's rows: each one's name in the layout and the physique type it sets.
pub const ROWS: [(&str, u32); 13] = [
    ("Build", 0x1),
    ("Height", 0x2),
    ("SkinTone", 0x4),
    ("SkinDetail", 0x8),
    ("HeadDetail", 0x10),
    ("HeadFrill", 0x20),
    ("FrillColor", 0x40),
    ("Special", 0x80),
    ("ShirtClothingColor", 0x100),
    ("PantsClothingColor", 0x200),
    ("BootsClothingColor", 0x400),
    ("FaceDetail", 0x800),
    ("EyeColor", 0x1000),
];

/// One kind of appearance choice as the screen shows it: its type, its label, how many stops its
/// slider has, and the stop the avatar stands at.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Choice {
    pub physique: u32,
    pub label: String,
    pub stops: usize,
    pub at: usize,
}

/// The elements a physique type's row is made of, by name: its slider, the buttons right and
/// left of it, its label, its field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RowElements {
    pub physique: u32,
    pub slider: Token,
    pub right: Token,
    pub left: Token,
    pub label: Token,
    pub field: Token,
}

/// Each physique type's row's elements.
// D2-01879
#[must_use]
pub fn init_ui_tables() -> Vec<RowElements> {
    ROWS.iter()
        .map(|&(n, physique)| RowElements {
            physique,
            slider: Token::of(&format!("{n}Slider")),
            right: Token::of(&format!("{n}RightButton")),
            left: Token::of(&format!("{n}LeftButton")),
            label: Token::of(&format!("{n}Label")),
            field: Token::of(&format!("{n}Field")),
        })
        .collect()
}

/// The appearance screen.
#[derive(Debug)]
pub struct AvatarModification {
    frame: FrameId,
    field: Option<Handle>,
    torch: Option<Handle>,
    /// The race's scene viewport.
    viewport: Option<Handle>,
    pages: [(Option<Handle>, Option<Handle>); 3],
    page: Page,
    zoomed: bool,
    rows: Vec<RowElements>,
    /// The stops of each type the avatar has, by type (the data table).
    data_table: Vec<(u32, usize)>,
    /// The pointer when last over the figure.
    last: (i32, i32),
    /// The appearance's version last shown.
    shown: u64,
    confirmation: Option<GenerationConfirmation>,
    wait_context: u32,
}

/// A page's field and its button, by page.
const PAGES: [(Page, Token, Token); 3] = [
    (Page::Head, name::HEAD_FIELD, name::HEAD_BUTTON),
    (Page::Body, name::BODY_FIELD, name::BODY_BUTTON),
    (Page::Clothing, name::CLOTHING_FIELD, name::CLOTHING_BUTTON),
];

impl AvatarModification {
    /// The appearance made: its field and the torch's viewport, the body page shown (its button
    /// ghosted), what it hears; the rows' elements by type; the avatar's race's scene; its rows
    /// from the avatar's choices (a type the race lacks hidden, a page with none hidden), labelled,
    /// at the avatar's stops; the camera; the race's music.
    // D2-01888, D2-01890, D2-01892, D2-01867
    pub fn new(ui: &mut Ui, data: &mut Data, out: &mut Vec<Request>) -> Self {
        let frame = ui.new_main_frame();
        let l = AVATAR_MODIFICATION_LAYOUT_ENUM;
        let field = ui.frame_create_root(frame, l, name::AVATAR_MODIFICATION_FIELD);
        let torch = ui.frame_create_root(frame, l, name::TORCH_VIEWPORT);
        let mut pages = [(None, None); 3];
        for (i, (_, f, b)) in PAGES.iter().enumerate() {
            pages[i] = (
                field.and_then(|r| ui.child_recursive(r, *f)),
                field.and_then(|r| ui.child_recursive(r, *b)),
            );
        }
        let mut me = Self {
            frame,
            field,
            torch,
            viewport: None,
            pages,
            page: Page::Body,
            zoomed: false,
            rows: Vec::new(),
            data_table: Vec::new(),
            last: ui.pointer(),
            shown: 0,
            confirmation: None,
            wait_context: 0,
        };
        if pages.iter().all(|(f, b)| f.is_some() && b.is_some()) {
            me.show_page(ui, Page::Body);
        }
        for (e, w) in [
            (name::MOUSE_ROTATION_OVER_FIELD, msg::MOUSE_MOVE_TOP),
            (name::ROTATION_SLIDER, msg::NEW_POSITION),
            (name::BACK_BUTTON, msg::BUTTON_RELEASE_ON),
            (name::CANCEL_BUTTON, msg::BUTTON_RELEASE_ON),
            (name::NEXT_BUTTON, msg::BUTTON_RELEASE_ON),
            (name::RANDOMIZE_CURRENT_BUTTON, msg::BUTTON_RELEASE_ON),
            (name::RANDOMIZE_ALL_BUTTON, msg::BUTTON_RELEASE_ON),
            (name::ZOOM_BUTTON, msg::BUTTON_RELEASE_ON),
            (name::HEAD_BUTTON, msg::BUTTON_RELEASE_ON),
            (name::BODY_BUTTON, msg::BUTTON_RELEASE_ON),
            (name::CLOTHING_BUTTON, msg::BUTTON_RELEASE_ON),
        ] {
            pregame::listen(ui, frame, e, w);
        }
        me.rows = init_ui_tables();
        me.init(ui, data);
        me.clear_data_table();
        me.get_data_table(ui, data);
        me.init_elements(ui, data);
        me.init_ui_from_data(ui, data);
        me.init_camera(out);
        crate::intro::play_music(ui, me.viewport, name::MUSIC, out);
        me
    }

    /// The chosen avatar's race's scene: its viewport made (`<race>Viewport`), where the figure
    /// stands; the pointer noted.
    // D2-01867
    fn init(&mut self, ui: &mut Ui, data: &Data) {
        self.last = ui.pointer();
        let Some(avatar) = data.avatar else { return };
        if let Some(v) = race_viewport(avatar.race) {
            self.viewport = ui.frame_create_root(self.frame, AVATAR_MODIFICATION_LAYOUT_ENUM, v);
        }
    }

    /// The race's scene viewport.
    #[must_use]
    pub const fn viewport(&self) -> Option<Handle> {
        self.viewport
    }

    /// The torch-lit figure's viewport.
    #[must_use]
    pub const fn torch(&self) -> Option<Handle> {
        self.torch
    }

    /// The page shown, and whether the view frames the head.
    #[must_use]
    pub const fn page(&self) -> Page {
        self.page
    }

    #[must_use]
    pub const fn zoomed(&self) -> bool {
        self.zoomed
    }

    /// The name's panel, while it is up.
    #[must_use]
    pub const fn confirmation(&self) -> Option<&GenerationConfirmation> {
        self.confirmation.as_ref()
    }

    /// The camera framed again (the head, zoomed, or the whole figure).
    // D2-01865
    fn init_camera(&self, out: &mut Vec<Request>) {
        out.push(Request::Zoom(self.zoomed));
    }

    /// The view turned to the rotation slider's heading.
    // D2-01863
    fn update_camera(&self, ui: &Ui, out: &mut Vec<Request>) {
        let at = self
            .field
            .and_then(|f| ui.child_recursive(f, name::ROTATION_SLIDER))
            .and_then(|s| ui.node(s))
            .and_then(|n| n.float_attr(attr::X_POSITION))
            .unwrap_or(0.0);
        out.push(Request::Turn {
            heading: at * 360.0,
        });
    }

    /// The avatar changed: the figure takes its new look (the client's scene follows the avatar);
    /// a view framing the head is framed again.
    // D2-01868
    fn update_by_mask(&self, out: &mut Vec<Request>) {
        if self.zoomed {
            self.init_camera(out);
        }
    }

    /// The stops of every type dropped.
    // D2-01885
    fn clear_data_table(&mut self) {
        self.data_table.clear();
    }

    /// The types the avatar has, each with its stops; a type the race lacks has its row hidden
    /// and gone; a page whose types the race lacks all has its button hidden.
    // D2-01886
    fn get_data_table(&mut self, ui: &mut Ui, data: &Data) {
        let mut has = [false; 3];
        let mut missing = Vec::new();
        for r in &self.rows {
            match data.choices.iter().find(|c| c.physique == r.physique) {
                Some(c) => {
                    for (i, (p, _, _)) in PAGES.iter().enumerate() {
                        if r.physique & p.mask() != 0 {
                            has[i] = true;
                        }
                    }
                    self.data_table.push((r.physique, c.stops));
                }
                None => missing.push(*r),
            }
        }
        for (i, (_, button)) in self.pages.iter().enumerate() {
            if let Some(b) = button {
                ui.set_visible(*b, has[i]);
            }
        }
        for r in missing {
            if let Some(f) = self.field.and_then(|f| ui.child_recursive(f, r.field)) {
                ui.set_visible(f, false);
                ui.remove(f);
            }
        }
        self.rows
            .retain(|r| data.choices.iter().any(|c| c.physique == r.physique));
    }

    /// Each row's slider heard; its label (its type's), and its slider's and buttons' tooltips
    /// naming it.
    // D2-01871
    fn init_elements(&mut self, ui: &mut Ui, data: &Data) {
        let Some(field) = self.field else { return };
        for r in &self.rows {
            pregame::listen(ui, self.frame, r.slider, msg::NEW_POSITION);
            let Some(c) = data.choices.iter().find(|c| c.physique == r.physique) else {
                continue;
            };
            let Some(label) = ui.child_recursive(field, r.label) else {
                continue;
            };
            ui.set_text(label, &c.label);
            let with = |id: &str| {
                message(
                    ui,
                    id,
                    &[("label", Var::Text(Box::new(StringInfo::literal(&c.label))))],
                )
            };
            let (slider_tip, right_tip, left_tip) = (
                with("ID_AvModify_TT_Slider"),
                with("ID_AvModify_TT_RButton"),
                with("ID_AvModify_TT_LButton"),
            );
            let Some(slider) = ui.child_recursive(field, r.slider) else {
                continue;
            };
            ui.set_tooltip(slider, &slider_tip);
            let Some(right) = ui.child_recursive(field, r.right) else {
                continue;
            };
            ui.set_tooltip(right, &right_tip);
            if let Some(left) = ui.child_recursive(field, r.left) {
                ui.set_tooltip(left, &left_tip);
            }
        }
    }

    /// Each slider at the stop the avatar's value stands at, its stops its type's.
    // D2-01870
    fn init_ui_from_data(&mut self, ui: &mut Ui, data: &Data) {
        let Some(field) = self.field else { return };
        if data.avatar.is_none() {
            return;
        }
        self.shown = data.choices_version;
        for &(physique, stops) in &self.data_table {
            let Some(r) = self.rows.iter().find(|r| r.physique == physique) else {
                continue;
            };
            let at = data
                .choices
                .iter()
                .find(|c| c.physique == physique)
                .map_or(0, |c| c.at);
            let Some(s) = ui.child_recursive(field, r.slider) else {
                continue;
            };
            #[allow(clippy::cast_precision_loss)]
            let position = at as f32 / (stops.max(1) as f32 - 1.0);
            ui.set_attr(
                s,
                attr::X_POSITION,
                Value::Float(if position.is_finite() { position } else { 0.0 }),
            );
            ui.set_attr(
                s,
                attr::NUM_STOP_LOCATIONS,
                Value::Int(i32::try_from(stops).unwrap_or(1)),
            );
            ui.lay_out_scrollbar(s);
        }
    }

    /// A slider moved: the avatar's value set to the stop it stands nearest below.
    // D2-01869
    fn temp(&self, ui: &Ui, physique: u32, slider: Handle, out: &mut Vec<Request>) {
        let Some(&(_, n)) = self.data_table.iter().find(|(t, _)| *t == physique) else {
            return;
        };
        if n == 0 {
            return;
        }
        let at = ui
            .node(slider)
            .and_then(|s| s.float_attr(attr::X_POSITION))
            .unwrap_or(0.0);
        #[allow(clippy::cast_precision_loss)]
        let k = ((n - 1) as f32 * at + 2.5e-5).max(0.0);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let stop = (k as usize).min(n - 1);
        out.push(Request::Choice { physique, stop });
    }

    /// Show one page of choices: its button ghosted, the others lit, its field shown; the view
    /// frames the head on the head's page.
    fn show_page(&mut self, ui: &mut Ui, page: Page) {
        self.page = page;
        for (i, (p, _, _)) in PAGES.iter().enumerate() {
            let (f, b) = self.pages[i];
            if let Some(b) = b {
                ui.set_attr(b, attr::GHOSTED, Value::Bool(*p == page));
            }
            if let Some(f) = f {
                ui.set_visible(f, *p == page);
            }
        }
        self.zoomed = page == Page::Head;
    }

    // D2-01862
    fn make_please_wait_dialog(&mut self, ui: &Ui, out: &mut Vec<Request>) {
        self.wait_context = pregame::please_wait(ui, self.frame, out);
    }

    /// What the appearance heard (with an avatar chosen): the rotation slider (the view turned);
    /// the pointer dragged over the figure with the left button held (the rotation slider moved
    /// the other way, round past its ends); Back (please wait, the hall); Cancel (the name's
    /// panel down); Randomize this page or all (the rows again, the figure's look); Zoom (the head
    /// framed or not); Next (the name's panel up); a page's button; a row's slider (its value).
    // D2-01878
    #[allow(clippy::too_many_lines)]
    fn listen_to_message(
        &mut self,
        ui: &mut Ui,
        data: &mut Data,
        from: Handle,
        element: Token,
        out: &mut Vec<Request>,
    ) {
        if data.avatar.is_none() || self.field.is_none() {
            return;
        }
        if element == name::ROTATION_SLIDER {
            self.update_camera(ui, out);
        }
        if element == name::MOUSE_ROTATION_OVER_FIELD {
            let (x, y) = ui.pointer();
            if ui.node(from).is_some_and(|n| n.over) && ui.left_down() {
                let rect = ui.screen_rect(from).unwrap_or_default();
                let slider = self
                    .field
                    .and_then(|f| ui.child_recursive(f, name::ROTATION_SLIDER));
                let Some(slider) = slider else { return };
                let at = ui
                    .node(slider)
                    .and_then(|n| n.float_attr(attr::X_POSITION))
                    .unwrap_or(0.0);
                #[allow(clippy::cast_precision_loss)]
                let mut to = at - (self.last.0 - x) as f32 / (rect.x1 - rect.x0 + 1) as f32;
                if to < 0.0 {
                    to = 1.0;
                } else if to > 1.0 {
                    to = 0.0;
                }
                ui.set_attr(slider, attr::X_POSITION, Value::Float(to));
                ui.lay_out_scrollbar(slider);
                self.update_camera(ui, out);
            }
            self.last = (x, y);
            return;
        }
        if element == name::BACK_BUTTON {
            self.make_please_wait_dialog(ui, out);
            pregame::queue(mode::AVATAR_SELECTION, out);
            return;
        }
        if element == name::CANCEL_BUTTON {
            if let Some(c) = self.confirmation.take() {
                c.close(ui);
            }
            return;
        }
        if element == name::RANDOMIZE_CURRENT_BUTTON || element == name::RANDOMIZE_ALL_BUTTON {
            let mask = if element == name::RANDOMIZE_ALL_BUTTON {
                ALL
            } else {
                self.page.mask()
            };
            out.push(Request::Randomize { mask });
            self.update_by_mask(out);
            return;
        }
        if element == name::ZOOM_BUTTON {
            self.zoomed = !self.zoomed;
            self.init_camera(out);
            return;
        }
        if element == name::NEXT_BUTTON {
            if self.confirmation.is_none() {
                self.confirmation = Some(GenerationConfirmation::new(
                    ui,
                    self.frame,
                    data.admin_client,
                ));
            }
            return;
        }
        if let Some(&(page, _, _)) = PAGES.iter().find(|(_, _, b)| *b == element) {
            if self.pages.iter().all(|(f, b)| f.is_some() && b.is_some()) {
                self.show_page(ui, page);
                self.init_camera(out);
            }
            return;
        }
        let moved: Vec<u32> = self
            .rows
            .iter()
            .filter(|r| r.slider == element)
            .map(|r| r.physique)
            .collect();
        for physique in &moved {
            self.temp(ui, *physique, from, out);
        }
        if !moved.is_empty() {
            self.update_by_mask(out);
        }
    }
}

impl Screen for AvatarModification {
    fn frame(&self) -> FrameId {
        self.frame
    }

    fn tick(&mut self, ui: &mut Ui, data: &mut Data, out: &mut Vec<Request>) {
        for m in pregame::take(ui, self.frame) {
            if let Mail::Element { from, element, .. } = m {
                self.listen_to_message(ui, data, from, element, out);
            }
        }
        if let Some(c) = &mut self.confirmation {
            c.tick(ui, data, out);
        }
        // The avatar's values changed (Randomize): the rows follow.
        if data.choices_version != self.shown {
            self.init_ui_from_data(ui, data);
        }
    }

    fn notice(&mut self, ui: &mut Ui, data: &mut Data, n: &Notice, out: &mut Vec<Request>) {
        if let Some(c) = &mut self.confirmation {
            c.notice(ui, data, n, out);
        }
    }

    /// The appearance goes: the name's panel, the rows' table, its elements, its please wait.
    // D2-01889, D2-01891
    fn close(mut self: Box<Self>, ui: &mut Ui, _data: &mut Data, out: &mut Vec<Request>) {
        if let Some(c) = self.confirmation.take() {
            c.close(ui);
        }
        self.clear_data_table();
        ui.drop_frame(self.frame);
        pregame::dialog_down(self.wait_context, out);
        self.wait_context = 0;
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// The letters a character's name may hold as it is typed: a name's letters, and those a
/// language composes them with.
// D2-01893
#[must_use]
pub fn name_input_filter(c: u16) -> bool {
    auberean_text::names::is_name_character(c, auberean_text::names::ENGLISH)
        || auberean_text::names::is_name_character_composing(c, auberean_text::names::ENGLISH)
}

/// The name's panel over the appearance: the name, and making the character.
#[derive(Debug)]
pub struct GenerationConfirmation {
    frame: FrameId,
    field: Option<Handle>,
    /// Enter (rather than Create) was pressed: once made, into the world.
    enter: bool,
    /// The administrator's button is lit.
    admin: bool,
}

impl GenerationConfirmation {
    /// The panel made under the appearance: its field, the player's buttons (an administrator's
    /// client shows the administrator's button too), Create, Enter, the administrator's button
    /// and the entry taking the keyboard heard; the entry takes only a name's letters.
    // D2-01894
    pub fn new(ui: &mut Ui, parent: FrameId, admin_client: bool) -> Self {
        let frame = ui.new_child_frame(parent);
        let field = ui.frame_create_root(
            frame,
            CONFIRMATION_LAYOUT_ENUM,
            name::GENERATION_CONFIRMATION_FIELD,
        );
        let me = Self {
            frame,
            field,
            enter: false,
            admin: false,
        };
        let Some(b) = field.and_then(|f| ui.child_recursive(f, name::BUTTON_FIELD)) else {
            return me;
        };
        ui.set_state(
            b,
            if admin_client {
                name::ADMIN_CLIENT
            } else {
                name::NON_ADMIN_CLIENT
            },
        );
        for (e, w) in [
            (name::CREATE_BUTTON, msg::BUTTON_RELEASE_ON),
            (name::ENTER_BUTTON, msg::BUTTON_RELEASE_ON),
            (name::ADMIN_BUTTON, msg::BUTTON_RELEASE_ON),
            (name::NAME_ENTRY, text_msg::ACTIVATED),
        ] {
            pregame::listen(ui, frame, e, w);
        }
        if let Some(entry) = ui.find(name::NAME_ENTRY) {
            ui.set_input_filter(entry, Some(name_input_filter));
        }
        me
    }

    /// The name typed so far.
    #[must_use]
    pub fn name(&self, ui: &Ui) -> String {
        self.name_entry(ui)
            .and_then(|e| ui.text(e))
            .unwrap_or_default()
    }

    /// The name's entry.
    #[must_use]
    pub fn name_entry(&self, ui: &Ui) -> Option<Handle> {
        self.field
            .and_then(|f| ui.child_recursive(f, name::NAME_ENTRY))
    }

    /// Show a line under the name: why the character was not made.
    // D2-01897
    pub fn set_error(&self, ui: &mut Ui, text: &str) {
        if let Some(e) = ui.find(name::ERROR_MSG) {
            ui.set_text(e, text);
        }
    }

    /// Ghost the buttons while the server is asked (or light them when it answered).
    // D2-01895
    pub fn ghost_buttons(&self, ui: &mut Ui, on: bool) {
        for b in [
            name::CREATE_BUTTON,
            name::ENTER_BUTTON,
            name::CANCEL_BUTTON,
            name::ADMIN_BUTTON,
        ] {
            if let Some(h) = ui.find(b) {
                ui.set_attr(h, attr::GHOSTED, Value::Bool(on));
            }
        }
    }

    /// What the panel heard: Create or Enter (the character asked of the server unless the name
    /// is empty, the avatar has no race, the account may not make its race, or it has no sex:
    /// then why, under the name); the administrator's button; the entry taking the keyboard the
    /// first time (its hint cleared, and heard no more).
    // D2-01898
    fn listen_to_message(
        &mut self,
        ui: &mut Ui,
        data: &Data,
        from: Handle,
        element: Token,
        out: &mut Vec<Request>,
    ) {
        let Some(avatar) = data.avatar else { return };
        if element == name::CREATE_BUTTON || element == name::ENTER_BUTTON {
            let name = self.name(ui);
            let why = if self.name_entry(ui).is_none() || name.is_empty() {
                Some("ID_Character_Err_NoName")
            } else if avatar.race == 0 {
                Some("ID_Character_Err_NoRace")
            } else if avatar.race & EMPYREAN != 0 && data.may_make & EMPYREAN == 0 {
                Some("ID_Character_Err_DoesNotHaveLegionsExpansion")
            } else if avatar.race & DRUDGE != 0 && data.may_make & DRUDGE == 0 {
                Some("ID_Character_Err_CanNotMakeDrudges")
            } else if avatar.sex == 0 {
                Some("ID_Character_Err_NoGender")
            } else {
                None
            };
            if let Some(why) = why {
                let text = message(ui, why, &[]);
                self.set_error(ui, &text);
                return;
            }
            self.ghost_buttons(ui, true);
            self.enter = element == name::ENTER_BUTTON;
            out.push(Request::Create {
                name,
                admin: self.admin,
            });
            return;
        }
        if element == name::ADMIN_BUTTON {
            self.admin = ui
                .node(from)
                .is_some_and(|n| n.bool_attr(attr::HIGHLIGHTED));
            return;
        }
        if element == name::NAME_ENTRY {
            ui.clear_all_text(from);
            if let Some(l) = ui.frame_listener(self.frame) {
                ui.unregister_for_message(
                    auberean_ui::Listener::Other(l),
                    name::NAME_ENTRY,
                    text_msg::ACTIVATED,
                );
            }
        }
    }

    fn tick(&mut self, ui: &mut Ui, data: &mut Data, out: &mut Vec<Request>) {
        for m in pregame::take(ui, self.frame) {
            if let Mail::Element { from, element, .. } = m {
                self.listen_to_message(ui, data, from, element, out);
            }
        }
    }

    /// The server answered: made, it joins the account's characters and, asked with Enter, the
    /// world is entered with it (else the list comes back on its row); refused, why (with the
    /// server's game's own reason for a corrupt one), under the name, and the buttons lit.
    // D2-01899
    fn notice(&mut self, ui: &mut Ui, data: &mut Data, n: &Notice, out: &mut Vec<Request>) {
        let Notice::CharGenVerification {
            response,
            character,
            error,
        } = n
        else {
            return;
        };
        if data.characters.is_none() {
            return;
        }
        let why = match *response {
            1 => {
                let Some(c) = character else { return };
                if let Some(set) = data.characters.as_mut() {
                    if set.which_slot(c.id).is_none() {
                        set.characters.push(c.clone());
                    }
                }
                if self.enter {
                    out.push(Request::Play { character: c.id });
                    pregame::queue(mode::GAMEPLAY, out);
                } else {
                    data.selected = data
                        .characters
                        .as_ref()
                        .and_then(|s| s.which_slot(c.id))
                        .unwrap_or(0);
                    pregame::queue(mode::CHARACTER_SELECTION, out);
                }
                return;
            }
            2 => "ID_Character_Err_Pending",
            3 => "ID_Character_Err_NameInUse",
            4 => "ID_Character_Err_NameBanned",
            5 => match *error {
                0 => "ID_Character_Err_Corrupt",
                0xC749 => "ID_Character_Err_Gun_Generic_Error",
                0xC74A => "ID_Character_Err_NameInUse",
                200 | 0xC74B => "ID_Character_Err_NameBanned",
                _ => "ID_Character_Err_Weenie_Generic_Error",
            },
            6 => "ID_Character_Err_DatabaseDown",
            7 => "ID_Character_Err_AdminPriv",
            8 => "ID_Character_Err_NoMoreSlots",
            _ => "ID_Character_Err_Unknown",
        };
        let text = message(
            ui,
            why,
            &[
                ("response", Var::Int(i32::try_from(*response).unwrap_or(0))),
                ("weenieError", Var::Text(Box::new(StringInfo::literal("")))),
            ],
        );
        self.set_error(ui, &text);
        self.ghost_buttons(ui, false);
    }

    /// The panel goes: its field, its listening.
    // D2-01896
    pub fn close(self, ui: &mut Ui) {
        ui.drop_frame(self.frame);
        if let Some(f) = self.field {
            ui.remove(f);
        }
    }
}
