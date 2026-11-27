//! The Horizon interface for the second game: the client's own interface beside the game's, drawn
//! from Horizon's art, reading the game through its view and asking for things as the client's
//! actions.
//!
//! **Depends on** the shell's interface seam (`auberean-shell`), the client's contract through the
//! SDK (`auberean-client-sdk`: the view, the events, the notices and the actions), the game's
//! records an interface reads (`auberean-ui`'s `Assets`: its strings and pictures), the game's own
//! interface's pointers and crosshair as the records name them (`auberean-ui-game`), the client's own
//! preferences (`auberean-options`), the world's records the map reads, the credits' layout and a
//! look's figure for the player's figure in the inventory window and a character's before the
//! world (`auberean-assets`, `auberean-scene`, `auberean-world`: the region's map pictures and
//! labels, its calendar), the story's sounds as the shell plays them (`auberean-audio`), the
//! client's random numbers (`auberean-primitives`), the shared float-to-integer conversion
//! (`dereth-primitives`), the world map's drawing both games share (`dereth-map-picture`), and the
//! widget kit and Horizon's art, which name no game (`dereth-ui-kit`, `dereth-horizon-art`). **Used by** the hosts (the desktop client, the
//! browser's, the world builder's play view), whose interface maker builds it with the game's
//! own interface beside it ([`maker`]).
//!
//! **Must never** hold the client or reach a platform or a device: it reads the game through the
//! shell's [`Game`] and asks for things as [`Request`]s and the client's actions.
//!
//! In the world [`Horizon`] shows its own HUD ([`ui::hud`]): the parameter bar with the experience
//! line, the target bar, the status row of effects, the selection's marks in the world
//! ([`ui::marker`], from where the shell's last world frame drew it) and the damage text over
//! struck objects ([`ui::damage`]), the hotbar and the attack button ([`ui::hotbar`]), the minimap
//! ([`ui::minimap`]), the log window and its chat entry ([`ui::chat`]), the game's questions in a
//! box ([`ui::questions`]), and the menu bar, a button for each window and a menu of the settings
//! window ([`ui::settings`]: the interface choice, logging out, the client's preferences, the
//! player's and the chat's options, and the key bindings), the HUD layout, Log Out and Exit Game;
//! and its windows, the skill window ([`ui::skills`]), the examine window ([`ui::examine`]), the
//! social window ([`ui::social`]), with the party list beside the screen ([`ui::party`]), the
//! windows of the player's things ([`ui::goods`]: the inventory with the player's figure
//! ([`figure`]), loot, trade, the shops and the market), the crafting window ([`ui::crafting`])
//! and the making window ([`ui::making`]), the pets and the Dismount button ([`ui::pets`]), the map
//! ([`ui::map`]), the journal ([`ui::journal`]) and a book read ([`ui::book`]); and the story's
//! scenes over the whole screen ([`ui::story`]).
//! It keeps its own plain state of the game ([`state`]), followed from the view, the notices and
//! the events each frame. Before the world it shows its own screens ([`ui::pregame`]): connecting
//! and the data download, character select over a view of the world the shell draws ([`vista`])
//! with the selected character's figure, character
//! creation ([`ui::creation`], over the avatars [`creation`] makes), the credits and the screen
//! that says why the session ended; the game before the world is followed in [`pregame`]. It logs
//! in itself with the account the host hands it ([`Start`]).
//!
//! The HUD's elements stand where the player's HUD layout puts them ([`ui::layout`]), at the
//! interface scale the settings choose ([`scale`], kept in Horizon's own file, [`own`]); the
//! plugins' windows and HUDs are drawn among them ([`ui::plugins`]). With a pad, gamepad mode
//! ([`gamepad`]) plays the game from the shell's pad ([`Game::pad`]).
//!
//! The keys are the shell's, Horizon's key layers in front while it is shown; Horizon carries out
//! the interface's actions they begin that it has something for ([`keys`]). In the world Horizon
//! asks for the orbit camera, the movement camera-based ([`camera_scheme`]); a press and the wheel
//! over the world are the world's, whose controls turn and zoom it. A left click on the world that
//! did not turn the camera selects what is under the pointer, and a second on the same thing uses
//! it.

pub mod abilities;
pub mod art;
pub mod atlas;
pub mod creation;
pub mod draw;
pub mod examined;
pub mod figure;
pub mod gamepad;
pub mod journal;
pub mod keys;
pub mod own;
pub mod pad;
pub mod pregame;
pub mod scale;
pub mod shared;
pub mod social;
pub mod state;
pub mod ui;
pub mod vista;
pub mod world_map;

use std::sync::Arc;

use auberean_client_sdk::contract::text::TextRef;
use auberean_client_sdk::contract::ChatHistory;
use auberean_client_sdk::Action;
use auberean_shell::dev::DevLogin;
use auberean_shell::input::{Button, Event, Key};
use auberean_shell::interface::{
    CameraRequest, Choice, CursorImage, Game, Interface, Login, OrbitSettings, Overlay, Phase,
    Request,
};

pub use shared::{maker, Shared};

use auberean_options::page::{section, Change, Request as PageRequest};
use auberean_options::preference::{Choice as ChoicePref, Flag};
use auberean_shell::client_preferences::orbit as orbit_keys;
use auberean_shell::interface::MovementMode;

use crate::art::{Pieces, Records};
use crate::draw::{DrawList, Rect};
use crate::state::{Frame, GameState};
use crate::ui::{vk, HorizonUi, InputFrame, PointerOver};

/// The interface's string table, where the game's own messages are.
pub const STRINGS: u32 = 0x2500_0012;

/// The emotes' string table, by its enumeration value, and the mapping that finds a table by one.
const EMOTES: u32 = 0x4100_001C;
const TABLE_MAPPING: u32 = 0x4000_0004;

/// The question Log Out asks first, by its name in [`STRINGS`].
pub const LOG_OUT_TEXT: &str = "ID_Option_LogoutText";

/// The line a typed line naming no command is answered with, by its name in [`STRINGS`].
const UNKNOWN_COMMAND: &str = "ID_Chat_UnknownCommand";
/// The game's commands table, and the reply command's names in it.
const COMMANDS: u32 = 0x4100_0007;
const REPLY_COMMAND: &str = "ID_Reply";

/// How long a second press on the same thing may come after the first to use it, in seconds.
const DOUBLE_PRESS: f64 = 0.5;

/// The modifier bits Shift and Ctrl, as the key maps hold them.
const SHIFT: u32 = 1;
const CTRL: u32 = 4;
/// The widget kit's number for Insert, which copies with Control and pastes with Shift.
const INSERT: usize = 0x2D;

/// How far the pointer may move between a left press on the world and its release for the two
/// to be a click, in pixels; further, the press turned the camera.
const CLICK_SLOP: f32 = 4.0;

/// The camera Horizon shows the world with: the orbit camera, the movement camera-based.
#[must_use]
pub fn camera_scheme() -> OrbitSettings {
    OrbitSettings::default()
}

/// What refusing Horizon on a host without its art says.
pub const NO_ART: &str = "The Horizon interface's art is not here";

/// How the client was started, for the screens before the world: the account to log in with, the
/// developer's automatic login (which then drives the session), the character to enter the world
/// as once the list comes (else the one to make and enter), whether creation offers the
/// administrator's character, and the running client's version as its host names it (`0.1.9`).
#[derive(Clone, Debug, Default)]
pub struct Start {
    pub login: Option<Login>,
    pub auto: Option<DevLogin>,
    pub play: Option<String>,
    pub create: Option<String>,
    pub admin: bool,
    pub version: String,
}

/// How many frames a login asked may leave the session unbegun before the connecting screen says
/// the server can't be reached.
const CONNECT_FRAMES: u8 = 2;

/// The credits' layout and the element whose string table holds their text, by enumeration value
/// and name; the mapping layouts are found by.
const CREDITS_LAYOUT: u32 = 0x4100_000E;
const CREDITS_TEXT: &str = "TextField";
const CREDITS_TABLE: &str = "stringtable";
const LAYOUT_MAPPING: u32 = 0x4000_0003;

/// The game's credits in `records`, a line each: every credits entry of the string table the
/// credits' layout names for their text, in order.
#[must_use]
pub fn credits(records: &(dyn auberean_ui::Assets + Send + Sync)) -> Vec<String> {
    use auberean_assets::ui::AttributeValue;
    use auberean_ui::token::Token;
    let r = records;
    let layout = r.data_id_by_enum(CREDITS_LAYOUT, LAYOUT_MAPPING);
    let table = r
        .layout(layout)
        .and_then(|l| {
            let e = l
                .elements
                .iter()
                .find(|e| e.id == Token::of(CREDITS_TEXT).raw())?;
            e.states.iter().find_map(|st| {
                st.attributes.iter().find_map(|a| match a.value {
                    AttributeValue::Id(t) if a.id == Token::of(CREDITS_TABLE).raw() => Some(t),
                    AttributeValue::DataFile(t) if a.id == Token::of(CREDITS_TABLE).raw() => {
                        Some(t.0)
                    }
                    _ => None,
                })
            })
        })
        .unwrap_or(0);
    let mut lines = Vec::new();
    let mut n = 1u32;
    while let Some(t) = r.text(table, Token::of(&format!("ID_Credits{n}")).raw()) {
        lines.extend(t.lines().map(str::to_owned));
        n += 1;
    }
    lines
}

/// Horizon's windows that were open in the world as it was left for another interface. Its
/// menus (the system menu, the settings, the HUD layout's editor, the chat entry) close as it is
/// left and stay closed; the windows the game opens itself (a book, a recipe being made, a trade,
/// a shop, loot) show again as the game has them. A session that left the world while Horizon was
/// not shown changes that: Horizon then opens what it would have kept open had it been shown
/// throughout, its settings among them, as soon as it is in the world again.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Reopen {
    skills: bool,
    examine: bool,
    map: bool,
    journal: bool,
    social: bool,
    inventory: bool,
    crafting: bool,
    settings: bool,
    /// A session left the world since Horizon was left.
    relogged: bool,
}

impl Reopen {
    /// The windows open in `ui` now.
    fn of(ui: &HorizonUi) -> Self {
        Self {
            skills: ui.skills.is_open(),
            examine: ui.examine.is_open(),
            map: ui.map.is_open(),
            journal: ui.journal.is_open(),
            social: ui.social.is_open(),
            inventory: ui.goods.inventory.is_open(),
            crafting: ui.crafting.is_open(),
            settings: ui.settings.is_open(),
            relogged: false,
        }
    }

    /// Those windows opened again, each where it was (the map following the player as it did,
    /// the social window on its tab); what the game is to be asked for them in `actions`.
    fn open(self, ui: &mut HorizonUi, actions: &mut Vec<Action>) {
        if self.skills && !ui.skills.is_open() {
            ui.skills.toggle();
        }
        if self.examine && !ui.examine.is_open() {
            ui.examine.open(actions);
        }
        if self.map && !ui.map.is_open() {
            let follow = ui.map.follow;
            ui.map.toggle(follow);
        }
        if self.journal && !ui.journal.is_open() {
            ui.journal.toggle();
        }
        if self.social && !ui.social.is_open() {
            ui.social.open();
        }
        if self.inventory && !ui.goods.inventory.is_open() {
            ui.goods.inventory.toggle();
        }
        if self.crafting && !ui.crafting.is_open() {
            ui.crafting.toggle();
        }
        if self.relogged && self.settings && !ui.settings.is_open() {
            ui.settings.toggle();
        }
    }
}

/// The Horizon interface.
pub struct Horizon {
    ui: HorizonUi,
    records: Option<Records>,
    /// The game's pointers, from its records.
    cursors: Cursors,
    /// How the client was started.
    start: Start,
    /// The game before the world.
    pregame: pregame::Pregame,
    /// The login was asked for, and the frames it may still take to begin.
    connect_asked: bool,
    connect_frames: u8,
    /// The character the client was started with was looked for in the list; one made for it is
    /// entered once made.
    started: bool,
    making_start: bool,
    /// The player was in the world last frame.
    was_in_world: bool,
    /// How many times a session had left the world when Horizon was last shown; none before it
    /// first was.
    left_seen: Option<u32>,
    /// The windows open in the world as Horizon was left, opened again once it is shown in the
    /// world again.
    reopen: Option<Reopen>,
    /// The player asked to log out and the server has not yet let the character go
    /// ([`ui::leaving`]).
    logging_out: bool,
    /// The credits' lines, read once they are first shown.
    credits: Option<Vec<String>>,
    /// The figure before the world: the selected character's, or the one being made.
    pre_figure: figure::Figure,
    state: GameState,
    input: InputFrame,
    /// What the pointer was over that shows a pointer of its own, as the last frame was drawn.
    pointer_over: Option<PointerOver>,
    /// The crosshair for the screen's size it was last placed on.
    crosshair: Option<((u32, u32), Option<Crosshair>)>,
    list: DrawList,
    overlay: draw::Overlay,
    /// The modifier bits held.
    held: u32,
    /// The keys and buttons Horizon took when they went down, so it takes them going up too.
    taken_keys: Vec<Key>,
    taken_buttons: [bool; 3],
    /// What the device events asked, carried out at the next update.
    pending: Vec<Request>,
    /// A press on the world waits for what is under it; and the last thing pressed on, and when.
    picking: bool,
    /// A thing put down on the world waits for what is under the pointer.
    drop_picking: bool,
    /// The player's figure, as the inventory window shows it.
    figure: figure::Figure,
    last_press: Option<(u64, f64)>,
    /// Where the left button went down over the world.
    pressed_at: Option<(f32, f32)>,
    now: f64,
    last_now: Option<f64>,
    random: Option<auberean_primitives::random::Random>,
    /// The selection's recent boxes, and its box as the marks were last placed from.
    recent: ui::marker::Recent,
    selection_bounds: Option<(auberean_primitives::Vec3, auberean_primitives::Vec3)>,
    /// Whether the art has the ring's inner layer, once looked; and whether the ring's pictures
    /// are in the shell's overlay.
    ring_inner: Option<bool>,
    ring_uploaded: bool,
    /// The orbit camera's preferences as the client last listed them, with what the settings set
    /// since: movement, reversed across, reversed up and down; and the listing they were read
    /// from.
    orbit_rows: Option<OrbitRows>,
    orbit_for: Option<u32>,
    /// The settings window listens to the orbit camera's preferences.
    orbit_attached: bool,
    /// Horizon's own settings' file has been asked for, and its answer has come.
    own_asked: bool,
    own_read: bool,
    /// The view behind character select this time it is up.
    visit: vista::Visit,
    /// Gamepad mode.
    pad: gamepad::PadMode,
    /// The sections of the client's preferences the settings' page listens to, and the listing
    /// its words were last found for.
    pages_attached: Vec<u32>,
    words_for: Option<u32>,
}

/// The orbit camera's preferences as the client lists them for a page.
#[derive(Clone, Debug)]
struct OrbitRows {
    movement: ChoicePref,
    reverse_x: Flag,
    reverse_y: Flag,
}

impl OrbitRows {
    /// A change to one of them, set or made by the client, taken in.
    fn take(&mut self, c: &Change) {
        match c {
            Change::Choice(m) if m.description.key == orbit_keys::MOVEMENT => {
                self.movement.clone_from(m);
            }
            Change::Flag(f) if f.description.key == orbit_keys::REVERSE_X => self.reverse_x = *f,
            Change::Flag(f) if f.description.key == orbit_keys::REVERSE_Y => self.reverse_y = *f,
            _ => {}
        }
    }
}

/// The key of a preference.
const fn change_key(c: &Change) -> u32 {
    match c {
        Change::Flag(f) => f.description.key,
        Change::Number(n) => n.description.key,
        Change::Choice(c) => c.description.key,
    }
}

/// The game's crosshair as its play screen places it: its picture and where it stands.
type Crosshair = (u32, auberean_ui::Rect);

/// The game's pointers as its own interface shows them: the default one everywhere, while an item
/// waits for its target the usage one over the world and over the items' icons, and over Horizon's
/// counterparts of the game's own elements the pointers those elements name.
#[derive(Clone, Debug, Default)]
struct Cursors {
    default: Option<Arc<CursorImage>>,
    usage: Option<Arc<CursorImage>>,
    /// A window's move bar's: over a title bar, a log window's tab.
    moving: Option<Arc<CursorImage>>,
    /// The log window's corner at its bottom right, which sizes it.
    sizing: Option<Arc<CursorImage>>,
    /// The player's figure in the inventory window's, and the money bag's.
    dropping: Option<Arc<CursorImage>>,
    /// The figure turned by dragging across it, in character creation.
    turning: Option<Arc<CursorImage>>,
}

impl Cursors {
    /// The pointers the records name, with their pictures.
    fn of(records: &Records) -> Self {
        use auberean_ui::Token;
        use auberean_ui_game::creation::AVATAR_MODIFICATION_LAYOUT_ENUM as CREATION;
        use auberean_ui_game::flow::{element_cursor, sizing_corner_cursor};
        use auberean_ui_game::gameplay::PLAY_LAYOUT_ENUM as PLAY;
        use auberean_ui_game::shortcuts::TOOLBAR_LAYOUT_ENUM as TOOLBAR;
        let image = |c: Option<auberean_ui::Cursor>| {
            let c = c?;
            let img = records.image(c.picture)?;
            Some(Arc::new(CursorImage::from_bgra(
                c.picture,
                (img.width, img.height),
                &img.bgra,
                (c.hot_x, c.hot_y),
            )))
        };
        let element = |layout, path: &[&str]| {
            let names: Vec<Token> = path.iter().map(|n| Token::of(n)).collect();
            image(element_cursor(
                Arc::clone(records),
                layout,
                names[0],
                &names[1..],
            ))
        };
        Self {
            default: image(auberean_ui_game::flow::default_cursor(records.as_ref())),
            usage: image(auberean_ui_game::selection::usage_cursor(Arc::clone(
                records,
            ))),
            moving: element(PLAY, &["Book", "MoveBar"]),
            sizing: image(sizing_corner_cursor(
                Arc::clone(records),
                TOOLBAR,
                Token::of("ChatWindow1"),
            )),
            dropping: element(TOOLBAR, &["PaperDollViewport"]),
            turning: element(
                CREATION,
                &["AvatarModificationField", "MouseRotationOverField"],
            ),
        }
    }

    /// The pointer Horizon's counterpart of the game's element shows.
    fn over(&self, over: PointerOver) -> Option<Arc<CursorImage>> {
        match over {
            PointerOver::Move => self.moving.clone(),
            PointerOver::Size => self.sizing.clone(),
            PointerOver::Drop => self.dropping.clone(),
            PointerOver::Turn => self.turning.clone(),
        }
    }
}

impl std::fmt::Debug for Horizon {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Horizon")
            .field("phase", &self.pregame.phase)
            .finish_non_exhaustive()
    }
}

impl Horizon {
    /// Horizon drawn from its art's `pieces` (their files, or the files as a host shares them),
    /// the game's `records` behind it (its strings and its pictures).
    #[must_use]
    pub fn new(pieces: impl Into<Arc<Pieces>>, records: Option<Records>) -> Self {
        let art = Arc::new(art::art(pieces, records.clone()));
        let mut ui = HorizonUi::new(art);
        ui.social.emote_table = records
            .as_ref()
            .map(|r| r.data_id_by_enum(EMOTES, TABLE_MAPPING))
            .filter(|t| *t != 0);
        ui.log_out.text = records
            .as_ref()
            .and_then(|r| r.string(STRINGS, auberean_ui::token::Token::of(LOG_OUT_TEXT).raw()))
            .unwrap_or_default();
        Self {
            ui,
            cursors: records.as_ref().map(Cursors::of).unwrap_or_default(),
            records,
            start: Start::default(),
            pregame: pregame::Pregame::default(),
            connect_asked: false,
            connect_frames: 0,
            started: false,
            making_start: false,
            was_in_world: false,
            left_seen: None,
            reopen: None,
            logging_out: false,
            credits: None,
            pre_figure: figure::Figure::default(),
            state: GameState::default(),
            input: InputFrame::default(),
            pointer_over: None,
            crosshair: None,
            list: DrawList::default(),
            overlay: draw::Overlay::default(),
            held: 0,
            taken_keys: Vec::new(),
            taken_buttons: [false; 3],
            pending: Vec::new(),
            picking: false,
            drop_picking: false,
            figure: figure::Figure::default(),
            last_press: None,
            pressed_at: None,
            now: 0.0,
            last_now: None,
            random: None,
            recent: ui::marker::Recent::default(),
            selection_bounds: None,
            ring_inner: None,
            ring_uploaded: false,
            orbit_rows: None,
            orbit_for: None,
            orbit_attached: false,
            own_asked: false,
            own_read: false,
            visit: vista::Visit::default(),
            pad: gamepad::PadMode::default(),
            pages_attached: Vec::new(),
            words_for: None,
        }
    }

    /// Horizon over the art the client carries built in, or why it can't be shown (a build that
    /// carries none, the browser's, until its page hands the art over).
    ///
    /// # Errors
    ///
    /// [`NO_ART`], when the client carries no art.
    pub fn built_in(records: Option<Records>) -> Result<Self, String> {
        let pieces = Pieces::built_in().ok_or_else(|| NO_ART.to_owned())?;
        Ok(Self::new(pieces, records))
    }

    /// Horizon started as `start` says: logging in with its account, or driven by the developer's
    /// login.
    #[must_use]
    pub fn with_start(mut self, start: Start) -> Self {
        self.ui.pregame.creation_window.admin_offered = start.admin;
        start.version.clone_into(&mut self.ui.pregame.version);
        self.start = start;
        self
    }

    /// The game before the world as Horizon last followed it.
    #[must_use]
    pub const fn pregame(&self) -> &pregame::Pregame {
        &self.pregame
    }

    /// The game as Horizon last followed it.
    #[must_use]
    pub const fn state(&self) -> &GameState {
        &self.state
    }

    /// The interface it draws.
    #[must_use]
    pub const fn ui(&self) -> &HorizonUi {
        &self.ui
    }

    /// The interface it draws, to change (a test choosing a page).
    pub const fn ui_mut(&mut self) -> &mut HorizonUi {
        &mut self.ui
    }

    /// The screen before the world up, while the player is not in the world.
    #[must_use]
    pub fn screen(&self) -> Option<ui::pregame::Screen> {
        (self.pregame.phase != Phase::World).then(|| self.ui.pregame.screen(&self.pregame))
    }

    /// Whether the player asked to log out and the server has not yet let the character go: the
    /// wait is shown, nothing else.
    #[must_use]
    pub const fn logging_out(&self) -> bool {
        self.logging_out
    }

    /// Log out at once, as Yes to the log-out question does: asked at the next update in the
    /// world.
    pub fn log_out(&mut self) {
        self.pending.push(Request::Act(Action::LogOff));
    }

    /// The crosshair as the game's own play screen places it on a screen of `size` pixels: its
    /// picture, and where it is drawn at its own size. Placed again only when the size changes.
    fn crosshair_on(&mut self, size: (u32, u32)) -> Option<(u32, draw::Rect)> {
        if self.crosshair.is_none_or(|(at, _)| at != size) {
            let placed = self.records.as_ref().and_then(|r| {
                let w = i32::try_from(size.0).ok()?;
                let h = i32::try_from(size.1).ok()?;
                auberean_ui_game::camera::crosshair(Arc::clone(r), (w, h))
            });
            self.crosshair = Some((size, placed));
        }
        let (picture, r) = self.crosshair.and_then(|(_, c)| c)?;
        #[allow(clippy::cast_precision_loss)] // a screen's pixels
        Some((
            picture,
            draw::Rect::new(
                r.x0 as f32,
                r.y0 as f32,
                (r.x1 - r.x0 + 1) as f32,
                (r.y1 - r.y0 + 1) as f32,
            ),
        ))
    }

    /// This frame's draw list, as the last update drew it.
    #[must_use]
    pub const fn draw_list(&self) -> &DrawList {
        &self.list
    }

    /// The game's chat windows go with its play screen when the character leaves the world, and
    /// the next entry's are made anew, their entries empty: so do the log and the chat entry.
    // D2-01936, D2-01928, D2-03105, D2-03117
    fn clear_chat(&mut self) {
        self.state.log.clear();
        self.ui.chat.clear(self.state.said);
    }

    /// A string of the game's own messages, by its name.
    fn string(&self, name: &str) -> String {
        self.records
            .as_ref()
            .and_then(|r| r.string(STRINGS, auberean_ui::token::Token::of(name).raw()))
            .unwrap_or_default()
    }

    /// The reply command as the chat entry starts it: a slash, the command's first name in the
    /// game's commands table, and a space (`/reply `); none when the table does not name it.
    fn reply_line(&self) -> Option<String> {
        let r = self.records.as_ref()?;
        let table = r.data_id_by_enum(COMMANDS, TABLE_MAPPING);
        let names = r.text(table, auberean_ui::token::Token::of(REPLY_COMMAND).raw())?;
        let first = names
            .split([';', ':', ','])
            .map(str::trim)
            .find(|w| !w.is_empty())?;
        Some(format!("/{first} "))
    }

    /// One of the game's messages before the world in `records`, by its name.
    fn message(records: Option<&Records>, name: &str) -> Option<String> {
        let r = records?;
        let table = r.data_id_by_enum(pregame::MESSAGES, TABLE_MAPPING);
        r.text(table, auberean_ui::token::Token::of(name).raw())
    }

    /// The credits' lines while they are up, as they are drawn.
    #[must_use]
    pub fn credits(&self) -> &[String] {
        self.credits.as_deref().unwrap_or_default()
    }

    /// A frame before the world: the developer's login or the login asked, the character the
    /// client was started with entered, the screens drawn, and what they asked carried out.
    fn before_world(&mut self, game: &Game<'_>, requests: &mut Vec<Request>, dt: f64) {
        if self.was_in_world {
            self.was_in_world = false;
            requests.push(Request::Camera(CameraRequest::Scheme(None)));
            self.clear_chat();
        }
        if matches!(game.phase, Phase::Characters | Phase::Disconnected) {
            self.logging_out = false;
        }
        let phase = game.phase;
        if let Some(auto) = &mut self.start.auto {
            auto.update(game, requests);
        } else if phase == Phase::Disconnected && !self.connect_asked {
            self.connect(requests);
        }
        if phase == Phase::Disconnected && self.connect_frames > 0 {
            self.connect_frames -= 1;
            if self.connect_frames == 0 && self.pregame.ended.is_none() {
                self.pregame.ended = Some("The server can't be reached.".to_owned());
            }
        } else if phase != Phase::Disconnected {
            self.connect_frames = 0;
        }
        let mut out = ui::pregame::PreOut::default();
        self.ui.pregame.follow(&self.pregame, &mut out);
        self.start_character(game, &mut out);
        #[allow(clippy::cast_precision_loss)]
        let screen = (game.size.0 as f32, game.size.1 as f32);
        let lines = self.credits.take().unwrap_or_default();
        self.pad.screens(game.pad, dt, &mut self.input);
        let shown = self.ui.pregame_frame(
            &mut self.list,
            screen,
            dt,
            &self.pregame,
            &mut self.input,
            self.start.login.is_some(),
            &lines,
            &mut out,
        );
        self.pointer_over = self.input.pointer_over;
        // Kept while the credits are up, and read once they are: the frame Credits was pressed in
        // drew none.
        self.credits = match (self.ui.pregame.credits.is_some(), lines.is_empty()) {
            (false, _) => None,
            (true, false) => Some(lines),
            (true, true) => Some(self.records.as_deref().map(credits).unwrap_or_default()),
        };
        self.input.next_frame();
        // The view behind character select, once Horizon's own file says which: a view picked
        // at random is kept in the file, so the next is another.
        if !ui::pregame::Screens::over_the_world(shown) {
            self.visit.leave();
        } else if self.own_read {
            let random = self.random.clone().unwrap_or_default();
            let own = &mut self.ui.settings.own;
            let (view, picked) = self.visit.view(own.vista, &mut own.vista_last, &random);
            if picked {
                requests.push(Request::WritePlayerFile {
                    name: own::FILE.to_owned(),
                    bytes: own.write().into_bytes(),
                });
            }
            requests.push(Request::Vista {
                camera: view.camera(game.now),
                time: vista::TIME,
            });
        }
        requests.extend(out.acts.into_iter().map(Request::Act));
        if out.connect {
            self.connect(requests);
        }
        if out.quit {
            requests.push(Request::Quit);
        }
        if out.create {
            let random = self.random.clone().unwrap_or_default();
            self.ui.pregame.creation = Some(creation::Creation::new(
                &game.world.records,
                self.pregame.may_make,
                random,
            ));
        }
    }

    /// The login asked with the account the client was started with, when it has one.
    fn connect(&mut self, requests: &mut Vec<Request>) {
        self.connect_asked = true;
        let Some(l) = &self.start.login else {
            return;
        };
        requests.push(Request::Connect {
            server: l.server,
            account: l.account.clone(),
            password: l.password.clone(),
        });
        self.connect_frames = CONNECT_FRAMES;
        self.pregame.ended = None;
    }

    /// The character the client was started with, once the list first comes: entered when the
    /// list holds it living (or as an administrator's, its name marked), else, for one named to
    /// make, made as a human and entered once made.
    fn start_character(&mut self, game: &Game<'_>, out: &mut ui::pregame::PreOut) {
        if self.making_start {
            if let Some(pregame::Made::Made(id)) = self.pregame.made {
                self.making_start = false;
                self.ui.pregame.entering = true;
                out.acts.push(Action::PlayCharacter { character: id });
            } else if matches!(self.pregame.made, Some(pregame::Made::Refused(_))) {
                self.making_start = false;
            }
        }
        if self.started || self.start.auto.is_some() || self.pregame.phase != Phase::Characters {
            return;
        }
        let Some(list) = &self.pregame.characters else {
            return;
        };
        self.started = true;
        let Some(name) = self
            .start
            .play
            .clone()
            .or_else(|| self.start.create.clone())
        else {
            return;
        };
        let (lower, admin) = (name.to_lowercase(), format!("+{name}").to_lowercase());
        let found = list.iter().find(|c| {
            c.deleting == 0 && (c.name.to_lowercase() == lower || c.name.to_lowercase() == admin)
        });
        if let Some(c) = found {
            self.ui.pregame.entering = true;
            out.acts.push(Action::PlayCharacter { character: c.id });
        } else if self
            .start
            .create
            .as_ref()
            .is_some_and(|c| c.eq_ignore_ascii_case(&name))
            && self.pregame.has_room()
        {
            let mut random = self.random.clone().unwrap_or_default();
            if let Some(made) =
                auberean_shell::dev::human(&game.world.records, &name, false, &mut random)
            {
                self.making_start = true;
                out.acts.push(Action::CreateCharacter(made));
            }
        }
    }

    /// What is drawn before the world: the screens, and the figure they show playing its idle (the
    /// selected character as it looks, or the one being made, turned as the player turned it and
    /// framed for the tallest its race may be).
    fn draw_before_world(&mut self, game: &Game<'_>, overlay: &mut Overlay) {
        let records = &game.world.records;
        let made = self
            .ui
            .pregame
            .creation
            .as_ref()
            .zip(self.ui.pregame.creation_window.figure)
            .and_then(|(c, (r, mark))| {
                let stand = figure::Stand {
                    heading: c.turn,
                    idle: true,
                    tallest: c.tallest(),
                };
                Some((c.look(records)?, stand, r, mark))
            });
        let listed = self.ui.pregame.figure.and_then(|(r, mark)| {
            let look = game.looks.characters.get(self.ui.pregame.selected)?;
            let stand = figure::Stand {
                idle: true,
                ..figure::Stand::STILL
            };
            Some((figure::or_stand_in(game.world, look), stand, r, mark))
        });
        let shown = match self.ui.pregame.screen(&self.pregame) {
            ui::pregame::Screen::Creation => made,
            ui::pregame::Screen::Select => listed,
            _ => None,
        };
        let figure = shown.and_then(|(look, stand, r, mark)| {
            let scene = self.pre_figure.scene_of(game.world, &look, stand, r)?;
            Some((scene, r, mark))
        });
        let spaces: Vec<_> = figure
            .iter()
            .map(|(_, r, mark)| (auberean_shell::interface::PreviewSpace::CharGen, *r, *mark))
            .collect();
        self.overlay
            .compose(overlay, game.size, self.ui.art(), &self.list, &spaces);
        overlay.scenes.extend(figure.map(|(scene, _, _)| scene));
        self.upload_ring(overlay);
    }

    /// The ring's pictures in the shell's overlay, where the world's marks are drawn from: put
    /// there once, the art's own ring (or a plain one where it has none) and its inner layer
    /// where it has one.
    fn upload_ring(&mut self, overlay: &mut Overlay) {
        use dereth_ui_kit::selection::{inner_picture, picture, RING_INNER_TEXTURE, RING_TEXTURE};
        if self.ring_uploaded {
            return;
        }
        let art = self.ui.art();
        let mut ok = overlay.upload(RING_TEXTURE, &picture(art));
        if let Some(inner) = inner_picture(art) {
            ok &= overlay.upload(RING_INNER_TEXTURE, &inner);
        }
        self.ring_uploaded = ok;
    }

    /// Where the selection is on the screen, as the shell's last world frame drew it.
    fn locate_selection(&mut self, game: &Game<'_>, screen: (f32, f32)) {
        let id = self.state.target.as_ref().map(|t| t.id);
        if id.is_none() {
            self.recent = ui::marker::Recent::default();
        }
        self.selection_bounds = id.and_then(|id| self.recent.average(game.seen, id));
        self.state.selection_at = id
            .zip(self.selection_bounds)
            .and_then(|(id, b)| ui::marker::locate(game.seen, id, b, screen));
    }

    /// Where each object a damage text stands over is on the screen, as the shell's last world
    /// frame drew it: those of the texts up and of those the scripts sent this frame.
    fn locate_struck(&mut self, game: &Game<'_>, screen: (f32, f32)) {
        let objects: Vec<u64> = self
            .ui
            .damage
            .objects()
            .chain(self.state.damage.texts.iter().map(|(o, _)| *o))
            .collect();
        self.state.damage.at = objects
            .into_iter()
            .filter_map(|o| Some((o, Self::struck_box(game, o, screen)?)))
            .collect();
    }

    /// The box object `o` fills on a screen of `screen` pixels as the shell's last world frame
    /// drew it; none when none of it was drawn on the screen.
    fn struck_box(game: &Game<'_>, o: u64, screen: (f32, f32)) -> Option<Rect> {
        let bounds = game.seen.body(o)?.bounds?;
        match ui::marker::locate(game.seen, o, bounds, screen)? {
            ui::marker::Where::Box(b) => Some(b),
            ui::marker::Where::Off { .. } => None,
        }
    }

    /// The interface's actions the keys began this frame, carried out where Horizon has something
    /// for them.
    fn keyed(&mut self, game: &Game<'_>) {
        for &(action, began) in game.keyed {
            match keys::keyed(action).filter(|_| began) {
                Some(keys::Keyed::Chat { slash }) => self.ui.chat.begin(slash),
                Some(keys::Keyed::Reply) => {
                    if let Some(line) = self.reply_line() {
                        self.ui.chat.open(&line);
                    }
                }
                Some(keys::Keyed::Settings) => self.ui.settings.toggle(),
                Some(keys::Keyed::Screenshot) => self.pending.push(Request::Screenshot),
                Some(k) => {
                    let asked = self.ui.keyed(k, &self.state);
                    self.pending.extend(asked.into_iter().map(Request::Act));
                }
                None => {}
            }
        }
    }

    /// The device's key as the widget kit's key number.
    fn kit_key(key: Key) -> Option<usize> {
        let letters = [
            Key::A,
            Key::B,
            Key::C,
            Key::D,
            Key::E,
            Key::F,
            Key::G,
            Key::H,
            Key::I,
            Key::J,
            Key::K,
            Key::L,
            Key::M,
            Key::N,
            Key::O,
            Key::P,
            Key::Q,
            Key::R,
            Key::S,
            Key::T,
            Key::U,
            Key::V,
            Key::W,
            Key::X,
            Key::Y,
            Key::Z,
        ];
        if let Some(i) = letters.iter().position(|k| *k == key) {
            return Some(0x41 + i);
        }
        Some(match key {
            Key::Backspace => vk::BACK,
            Key::Tab => vk::TAB,
            Key::Enter | Key::NumpadEnter => vk::ENTER,
            Key::Escape => vk::ESCAPE,
            Key::End => vk::END,
            Key::Home => vk::HOME,
            Key::ArrowLeft => vk::LEFT,
            Key::ArrowUp => vk::UP,
            Key::ArrowRight => vk::RIGHT,
            Key::ArrowDown => vk::DOWN,
            Key::Delete => vk::DELETE,
            Key::Insert => INSERT,
            _ => return None,
        })
    }

    /// A device event before the world: every key and press is the screens'.
    fn event_before_world(&mut self, event: &Event) -> bool {
        match event {
            Event::Key { key, down, .. } => {
                let meta = match key {
                    Key::ShiftLeft | Key::ShiftRight => SHIFT,
                    Key::ControlLeft | Key::ControlRight => CTRL,
                    _ => 0,
                };
                if meta != 0 {
                    if *down {
                        self.held |= meta;
                    } else {
                        self.held &= !meta;
                    }
                    self.input.shift = self.held & SHIFT != 0;
                    self.input.ctrl = self.held & CTRL != 0;
                } else if *down {
                    if let Some(k) = Self::kit_key(*key) {
                        self.input.keys.push(k);
                    }
                }
                true
            }
            Event::Text(text) => {
                if self.ui.pregame.typing() {
                    self.input
                        .chars
                        .extend(dereth_primitives::text::key_text::typed_characters(text));
                }
                true
            }
            Event::Pointer { x, y } => {
                self.input.mouse = (*x, *y);
                false
            }
            Event::Button { button, down } => {
                let i = match button {
                    Button::Left => 0,
                    Button::Right => 1,
                    Button::Middle => 2,
                };
                self.input.down[i] = *down;
                if *down {
                    self.input.pressed[i] = true;
                } else {
                    self.input.released[i] = true;
                }
                true
            }
            Event::Wheel { notches } => {
                self.input.wheel += notches;
                true
            }
            Event::Focus(false) => {
                self.held = 0;
                self.input.shift = false;
                self.input.ctrl = false;
                false
            }
            _ => false,
        }
    }

    /// What the world's press asked for has come: the thing under the pointer selected, and used
    /// when it was pressed on just before.
    fn picked(&mut self, game: &Game<'_>, requests: &mut Vec<Request>) {
        if self.drop_picking {
            if let Some(found) = game.world_pick {
                self.drop_picking = false;
                let mut out = ui::Outcome::default();
                self.ui
                    .goods
                    .world_found(found.unwrap_or(0), &self.state, &mut out);
                requests.extend(out.acts.into_iter().map(Request::Act));
            }
            return;
        }
        if !self.picking {
            return;
        }
        let Some(found) = game.world_pick else {
            return;
        };
        self.picking = false;
        let Some(target) = found.filter(|t| *t != 0) else {
            return;
        };
        let again = self
            .last_press
            .is_some_and(|(t, at)| t == target && self.now - at <= DOUBLE_PRESS);
        requests.push(Request::Act(Action::Select { target }));
        if again {
            requests.push(Request::Act(Action::UseSelected));
            self.last_press = None;
        } else {
            self.last_press = Some((target, self.now));
        }
    }

    /// The orbit camera's preferences listed while the settings are open: asked for as the window
    /// opens and let go of as it closes, read from each new listing of the client's, and kept up
    /// with what the settings set and what the client changed itself (the client lists them as
    /// they are attached, not as each is set).
    fn follow_orbit(&mut self, game: &Game<'_>, requests: &mut Vec<Request>) {
        let open = self.ui.settings.is_open();
        if open != self.orbit_attached {
            self.orbit_attached = open;
            let sections = vec![section::ORBIT];
            requests.push(Request::Preferences(if open {
                PageRequest::Attach(sections)
            } else {
                PageRequest::Detach(sections)
            }));
        }
        let listing = game.preferences.listing();
        let serial = listing.map(|l| l.serial);
        let listed = listing
            .filter(|_| serial != self.orbit_for)
            .and_then(|l| l.sections.iter().find(|(s, _)| *s == section::ORBIT))
            .map(|(_, rows)| rows);
        self.orbit_for = serial;
        if let Some(rows) = listed {
            let find = |k: u32| rows.iter().find(|c| change_key(c) == k);
            let flag = |k: u32| match find(k) {
                Some(Change::Flag(f)) => Some(*f),
                _ => None,
            };
            let movement = match find(orbit_keys::MOVEMENT) {
                Some(Change::Choice(c)) => Some(c.clone()),
                _ => None,
            };
            if let (Some(movement), Some(reverse_x), Some(reverse_y)) = (
                movement,
                flag(orbit_keys::REVERSE_X),
                flag(orbit_keys::REVERSE_Y),
            ) {
                self.orbit_rows = Some(OrbitRows {
                    movement,
                    reverse_x,
                    reverse_y,
                });
            }
        }
        if let Some(rows) = &mut self.orbit_rows {
            for c in game.preferences_changed {
                rows.take(c);
            }
        }
        self.ui.orbit = self.orbit_rows.as_ref().map(|r| ui::settings::Orbit {
            movement: MovementMode::from_value(r.movement.get()),
            reverse_x: r.reverse_x.get(),
            reverse_y: r.reverse_y.get(),
        });
    }

    /// What the settings' pages read: the sections of the client's preferences the page shown
    /// lists (asked for as it is shown and let go of as it goes), the words of their labels, the
    /// player's options, and the keys Horizon binds now for the key bindings page.
    fn follow_pages(&mut self, game: &Game<'_>, requests: &mut Vec<Request>) {
        let want = self.ui.settings.sections().to_vec();
        if want != self.pages_attached {
            if !self.pages_attached.is_empty() {
                let gone = std::mem::take(&mut self.pages_attached);
                requests.push(Request::Preferences(PageRequest::Detach(gone)));
            }
            if !want.is_empty() {
                requests.push(Request::Preferences(PageRequest::Attach(want.clone())));
            }
            self.pages_attached = want;
        }
        let listing = game
            .preferences
            .listing()
            .filter(|_| !self.pages_attached.is_empty());
        if listing.map(|l| l.serial) == self.words_for {
            // The client's own changes to what the page shows, between its listings.
            for c in game.preferences_changed {
                self.ui.settings.data.fold(c);
            }
        } else {
            self.words_for = listing.map(|l| l.serial);
            let data = &mut self.ui.settings.data;
            data.listing = listing.cloned();
            data.words.clear();
            if let (Some(l), Some(r)) = (listing, &self.records) {
                let mut wanted = Vec::new();
                for (_, rows) in &l.sections {
                    for c in rows {
                        match c {
                            Change::Flag(f) => {
                                wanted.push((f.description.table, f.description.label))
                            }
                            Change::Number(n) => {
                                wanted.push((n.description.table, n.description.label));
                            }
                            Change::Choice(c) => {
                                wanted.push((c.description.table, c.description.label));
                                wanted.extend(c.list_choices().iter().map(|(_, l)| *l));
                            }
                        }
                    }
                }
                for (table, string) in wanted {
                    let id = r.data_id_by_enum(table, TABLE_MAPPING);
                    if let Some(w) = r.text(id, string) {
                        data.words.insert((table, string), w);
                    }
                }
            }
        }
        let open = self.ui.settings.is_open();
        self.ui.settings.data.options = game.view.and_then(|v| v.options.clone()).filter(|_| open);
        let b = &mut self.ui.settings.bindings;
        if open && self.ui.settings.tab == ui::settings::tab::KEYS && b.maps.is_none() {
            let layers = game.keys.stack.interface();
            let mut maps = ui::bindings::Maps::default();
            for l in layers {
                if let Some(i) = ui::bindings::METAS.iter().position(|m| *m == l.meta) {
                    maps[i] = l.map.bindings.iter().map(|k| (k.input, k.action)).collect();
                }
            }
            b.maps = Some(maps);
        }
    }

    /// What the settings' pages asked of the client: preferences set, key bindings applied.
    fn pages_asked(&mut self, requests: &mut Vec<Request>) {
        let s = &mut self.ui.settings;
        requests.extend(s.pages.asked.drain(..).map(Request::Preferences));
        if let Some(maps) = s.bindings.applied.take() {
            requests.push(Request::InterfaceKeyMaps(maps));
        }
    }

    /// One of the orbit camera's preferences set in the settings: tried, applied and kept, as an
    /// options page does, and shown as set from the next frame.
    fn set_orbit(&mut self, change: ui::settings::OrbitChange, requests: &mut Vec<Request>) {
        use ui::settings::OrbitChange as O;
        let Some(rows) = &mut self.orbit_rows else {
            return;
        };
        let change = match change {
            O::Movement(m) => {
                let mut c = rows.movement.clone();
                c.set_value(m.value());
                Change::Choice(c)
            }
            O::ReverseX(on) => {
                let mut f = rows.reverse_x;
                f.set_value(on);
                Change::Flag(f)
            }
            O::ReverseY(on) => {
                let mut f = rows.reverse_y;
                f.set_value(on);
                Change::Flag(f)
            }
        };
        rows.take(&change);
        for r in [
            PageRequest::Change(change),
            PageRequest::Apply(section::ORBIT),
            PageRequest::Save,
        ] {
            requests.push(Request::Preferences(r));
        }
    }

    /// What the windows of things asked besides the game's actions: a search under the pointer
    /// where a thing was put down on the world, lines told the player, a trade's or a purchase's
    /// answer noted.
    fn goods_asked(&mut self, requests: &mut Vec<Request>) {
        let asks = std::mem::take(&mut self.ui.goods.asks);
        if let Some((x, y)) = asks.pick_world {
            self.drop_picking = true;
            self.picking = false;
            requests.push(Request::PickWorld { x, y });
        }
        for line in asks.told {
            self.state.say(state::ERROR_TEXT, line);
        }
        for s in asks.trade {
            match s {
                ui::trade::Settle::Answering => self.state.goods.trade_answering(),
                ui::trade::Settle::Dismissed => self.state.goods.trade_dismissed(),
            }
        }
        if asks.purchase_answered {
            self.state.goods.purchase_answered();
        }
    }

    /// Horizon's own settings: their file asked for the first time Horizon is shown, and read
    /// once it comes.
    fn read_own(&mut self, game: &Game<'_>, requests: &mut Vec<Request>) {
        if !std::mem::replace(&mut self.own_asked, true) {
            requests.push(Request::ReadPlayerFile {
                name: own::FILE.to_owned(),
            });
        }
        for (name, bytes) in game.files_read {
            if name == own::FILE {
                self.own_read = true;
                if let Some(b) = bytes {
                    self.ui.settings.own = own::Own::read(&String::from_utf8_lossy(b));
                }
            }
        }
    }

    /// The HUD layout of the character in the world: its file asked for as the character comes
    /// into the world, and read once it comes.
    fn read_layout(&mut self, game: &Game<'_>, requests: &mut Vec<Request>) {
        let file = game
            .view
            .and_then(|v| v.player.as_ref())
            .filter(|_| game.phase == Phase::World)
            .and_then(|p| {
                p.name
                    .as_ref()
                    .map(|n| ui::layout::file_name(&p.world_name, n))
            });
        if let Some(f) = file.filter(|f| self.ui.layout.file.as_ref() != Some(f)) {
            self.ui.layout.start(f.clone());
            requests.push(Request::ReadPlayerFile { name: f });
        }
        for (name, bytes) in game.files_read {
            if self.ui.layout.file.as_ref() == Some(name) {
                let text = bytes
                    .as_ref()
                    .map(|b| String::from_utf8_lossy(b).into_owned());
                self.ui.layout.read(text.as_deref());
            }
        }
    }

    /// Horizon's own settings kept, when one changed; and the HUD layout,
    /// when its editor saved it.
    fn keep_own(&mut self, requests: &mut Vec<Request>) {
        if self.ui.settings.take_own_changed() {
            requests.push(Request::WritePlayerFile {
                name: own::FILE.to_owned(),
                bytes: self.ui.settings.own.write().into_bytes(),
            });
        }
        if let Some((name, text)) = self.ui.layout.take_write() {
            requests.push(Request::WritePlayerFile {
                name,
                bytes: text.into_bytes(),
            });
        }
    }

    /// The choice of interface carried out and kept in the client's preferences.
    fn choose(choice: Choice, requests: &mut Vec<Request>) {
        requests.push(Request::ChooseInterface(choice));
        requests.push(Request::Preferences(PageRequest::Save));
    }
}

impl Interface for Horizon {
    fn event(&mut self, event: &Event, game: &Game<'_>) -> bool {
        // A key pressed switches gamepad mode off.
        if matches!(event, Event::Key { down: true, .. }) && self.pad.on {
            let mut let_go = Vec::new();
            self.pad.off(&mut let_go);
            self.pending.extend(let_go.into_iter().map(Request::Act));
        }
        if game.phase != Phase::World {
            return self.event_before_world(event);
        }
        match event {
            Event::Key { key, down, repeat } => {
                let meta = match key {
                    Key::ShiftLeft | Key::ShiftRight => SHIFT,
                    Key::ControlLeft | Key::ControlRight => CTRL,
                    _ => 0,
                };
                if meta != 0 {
                    if *down {
                        self.held |= meta;
                    } else {
                        self.held &= !meta;
                    }
                    self.input.shift = self.held & SHIFT != 0;
                    self.input.ctrl = self.held & CTRL != 0;
                    return self.ui.typing();
                }
                // A key for the action the key bindings page waits on.
                if self.ui.settings.bindings.capturing.is_some() && *down && !*repeat {
                    if *key == Key::Escape {
                        self.ui.settings.bindings.cancel();
                    } else {
                        let meta = if self.held & CTRL != 0 {
                            auberean_shell::keys::META_CTRL
                        } else if self.held & SHIFT != 0 {
                            auberean_shell::keys::META_SHIFT
                        } else {
                            0
                        };
                        self.ui.settings.bindings.bind(key.input_word(), meta);
                    }
                    self.taken_keys.push(*key);
                    return true;
                }
                if self.ui.typing() {
                    if *down {
                        if let Some(k) = Self::kit_key(*key) {
                            self.input.keys.push(k);
                        }
                    }
                    return true;
                }
                if !*down {
                    let at = self.taken_keys.iter().position(|k| k == key);
                    return at.map(|i| self.taken_keys.swap_remove(i)).is_some();
                }
                if *repeat {
                    return self.taken_keys.contains(key);
                }
                // Control-C or Control-Insert copying what is selected in the chat log is the
                // interface's: the key maps' C does not see it.
                if self.held & CTRL != 0
                    && matches!(key, Key::C | Key::Insert)
                    && self.ui.chat.has_selection()
                {
                    if let Some(k) = Self::kit_key(*key) {
                        self.input.keys.push(k);
                    }
                    self.taken_keys.push(*key);
                    return true;
                }
                // Escape closes the window on top while one is open; every other key is the key
                // maps'.
                if *key == Key::Escape && self.held == 0 && self.ui.window_open() {
                    let mut asked = Vec::new();
                    self.ui.close_top(&mut asked);
                    self.pending.extend(asked.into_iter().map(Request::Act));
                    self.taken_keys.push(*key);
                    return true;
                }
                false
            }
            Event::Text(text) => {
                if self.ui.typing() {
                    self.input
                        .chars
                        .extend(dereth_primitives::text::key_text::typed_characters(text));
                    return true;
                }
                false
            }
            Event::Pointer { x, y } => {
                self.input.mouse = (*x, *y);
                false
            }
            Event::Button { button, down } => {
                let i = match button {
                    Button::Left => 0,
                    Button::Right => 1,
                    Button::Middle => 2,
                };
                let (x, y) = self.input.mouse;
                if *down {
                    if self.ui.covers(x, y) || self.ui.typing() && i == 0 {
                        self.input.down[i] = true;
                        self.input.pressed[i] = true;
                        self.taken_buttons[i] = true;
                        return true;
                    }
                    // A press over the world is the world's: held, its controls turn the camera.
                    if *button == Button::Left {
                        self.pressed_at = Some((x, y));
                    }
                    false
                } else {
                    self.input.down[i] = false;
                    if std::mem::take(&mut self.taken_buttons[i]) {
                        self.input.released[i] = true;
                        return true;
                    }
                    // A left click on the world that did not drag the camera picks what is under
                    // the pointer.
                    if let Some((px, py)) = self.pressed_at.take().filter(|_| i == 0) {
                        if (x - px).abs() <= CLICK_SLOP && (y - py).abs() <= CLICK_SLOP {
                            self.picking = true;
                            self.pending.push(Request::PickWorld { x, y });
                        }
                    }
                    false
                }
            }
            Event::Wheel { notches } => {
                let (x, y) = self.input.mouse;
                // The wheel over the world is the world's: its controls zoom the camera.
                if !self.ui.covers(x, y) {
                    return false;
                }
                self.input.wheel += notches;
                true
            }
            Event::Focus(false) => {
                self.held = 0;
                self.input.shift = false;
                self.input.ctrl = false;
                false
            }
            _ => false,
        }
    }

    fn update(&mut self, game: &Game<'_>, requests: &mut Vec<Request>) {
        let dt = self.last_now.map_or(0.0, |t| (game.now - t).max(0.0));
        self.last_now = Some(game.now);
        self.now = game.now;
        self.read_own(game, requests);
        self.read_layout(game, requests);
        // A session left the world since Horizon was last shown: what its chat held is of a
        // session that ended, and goes before this frame's lines are taken; the windows it was
        // left with open come back at the next world, as if it had been shown throughout.
        if self
            .left_seen
            .replace(game.left_world)
            .is_some_and(|n| n != game.left_world)
        {
            self.clear_chat();
            if let Some(r) = &mut self.reopen {
                r.relogged = true;
            }
        }
        // The lines for the chat windows and the client's lines are kept whichever screens are
        // shown, so the log has what came before the world too.
        let lines: Vec<(u32, String)> = game
            .shown
            .iter()
            .map(|s| (s.kind, s.text.clone()))
            .collect();
        let write = move |t: &TextRef| -> Option<String> { game.write_text(t) };
        let unknown = self.string(UNKNOWN_COMMAND);
        let records = self.records.clone();
        let message = move |name: &str| Self::message(records.as_ref(), name);
        self.pregame.follow(&pregame::PreFrame {
            phase: game.phase,
            view: game.view,
            ended: game.ended,
            stopped: game.stopped.is_some(),
            now: game.now,
            portal_space: game.status.portal_drawn,
            write: &write,
            message: &message,
        });
        // Until the login names the world, the server's address.
        if let Some(l) = self
            .start
            .login
            .as_ref()
            .filter(|_| self.pregame.world.is_empty())
        {
            self.pregame.world = l.server.to_string();
        }
        self.state.follow(&Frame {
            view: game.view,
            notices: game.notices,
            events: game.events,
            told: game.told,
            lines: &lines,
            write: &write,
            unknown_command: &unknown,
        });
        self.state.plugins_said(game.plugin_lines);
        self.state.follow_world(&atlas::WorldFrame {
            world: game.world,
            records: self.records.as_ref(),
            view: game.view,
            write: &write,
            arrivals: game.arrivals,
        });
        if game.phase != Phase::World {
            // Out of the world, the windows left open in it stay closed, but for those a Horizon
            // shown through a relog would still have open.
            if !self.reopen.is_some_and(|r| r.relogged) {
                self.reopen = None;
            }
            let mut let_go = Vec::new();
            self.pad.let_go(&mut let_go);
            requests.extend(let_go.into_iter().map(Request::Act));
            self.before_world(game, requests, dt);
            return;
        }
        if !self.was_in_world {
            self.was_in_world = true;
            self.ui.pregame.reset();
            self.credits = None;
            self.input = InputFrame::default();
        }
        if let Some(open) = self.reopen.take() {
            let mut asked = Vec::new();
            open.open(&mut self.ui, &mut asked);
            requests.extend(asked.into_iter().map(Request::Act));
        }
        // The map's picture, drawn from the world's land off the frame while the map is open.
        self.ui.map.keep_picture(game.world);
        // The orbit camera, asked again each frame in the world: a switch of interface ends it.
        requests.push(Request::Camera(CameraRequest::Scheme(
            Some(camera_scheme()),
        )));
        if self.logging_out {
            // Only the wait is shown, and nothing more is asked.
            self.pending.clear();
            #[allow(clippy::cast_precision_loss)]
            let screen = (game.size.0 as f32, game.size.1 as f32);
            ui::leaving::frame(&mut self.list, self.ui.art(), screen);
            self.pointer_over = None;
            self.input.next_frame();
            return;
        }
        self.keyed(game);
        self.picked(game, requests);
        requests.append(&mut self.pending);
        game.plugin_windows.clone_into(&mut self.ui.plugins.windows);
        let asked = self
            .pad
            .frame(game.pad, dt, &mut self.ui, &self.state, &mut self.input);
        requests.extend(asked.requests);
        requests.extend(asked.acts.into_iter().map(Request::Act));
        self.ui.crosses = self.pad.on.then_some(ui::cross::Crosses {
            held: self.pad.half,
        });
        self.follow_pages(game, requests);
        #[allow(clippy::cast_precision_loss)]
        let screen = (game.size.0 as f32, game.size.1 as f32);
        self.locate_selection(game, screen);
        self.locate_struck(game, screen);
        // The ring under the selection, laid by the world as it is drawn this frame.
        let art = self.ui.art();
        let inner = *self
            .ring_inner
            .get_or_insert_with(|| dereth_ui_kit::selection::inner_picture(art).is_some());
        requests.push(Request::GroundMarks(ui::marker::ground(
            &self.state,
            game.now,
            inner,
        )));
        // The crosshair in the middle of the screen while the pointer is held for mouse look.
        self.ui.crosshair = if game.mouse_look {
            self.crosshair_on(game.size)
        } else {
            None
        };
        let out = self.ui.frame(
            &mut self.list,
            screen,
            dt,
            &self.state,
            &mut self.input,
            game.interface,
        );
        self.pointer_over = self.input.pointer_over;
        self.input.next_frame();
        if let Some(text) = out.say {
            requests.push(Request::Act(Action::Say { text }));
        }
        if let Some(earlier) = out.recall {
            requests.push(Request::Act(Action::AskHistory {
                list: ChatHistory::Commands,
                earlier,
            }));
        }
        if let Some(c) = out.choose {
            Self::choose(c, requests);
        }
        if let Some(c) = out.orbit {
            self.set_orbit(c, requests);
        }
        self.follow_orbit(game, requests);
        self.pages_asked(requests);
        if out.log_out {
            self.ui.settings.close();
            requests.push(Request::Act(Action::LogOff));
        }
        if out.quit {
            // The game's options kept first, as the game's own Quit does.
            requests.push(Request::Act(Action::KeepOptions));
            requests.push(Request::Quit);
        }
        requests.extend(out.requests);
        requests.extend(out.acts.into_iter().map(Request::Act));
        self.goods_asked(requests);
        self.keep_own(requests);
        // However it was asked (a key, the settings, a menu), a log-off begins the wait, and the
        // portal space it ends in is kept in sight a moment once the world is left.
        if requests.contains(&Request::Act(Action::LogOff)) {
            self.logging_out = true;
            requests.push(Request::PortalSpaceLingers(ui::leaving::PORTAL_SEEN));
        }
    }

    /// The selection's marks and the damage texts moved to where the world, as it is drawn this
    /// frame, puts their objects.
    fn after_world(&mut self, game: &Game<'_>, _requests: &mut Vec<Request>) {
        if game.phase != Phase::World {
            return;
        }
        #[allow(clippy::cast_precision_loss)]
        let screen = (game.size.0 as f32, game.size.1 as f32);
        let now = self
            .state
            .target
            .as_ref()
            .zip(self.selection_bounds)
            .and_then(|(t, b)| ui::marker::locate(game.seen, t.id, b, screen));
        let struck: Vec<Option<Rect>> = self
            .ui
            .damage
            .anchored()
            .iter()
            .map(|o| Self::struck_box(game, *o, screen))
            .collect();
        ui::marker::follow(&mut self.list, now, &struck);
    }

    fn draw(&mut self, game: &Game<'_>, overlay: &mut Overlay) {
        overlay.behind = None;
        overlay.middle = None;
        overlay.front = None;
        let (x, y) = self.input.mouse;
        let aimed = self.ui.usage_pointer_at(x, y);
        let element = self.pointer_over.and_then(|o| self.cursors.over(o));
        overlay.cursor = if game.phase == Phase::World && self.state.goods.targeting && aimed {
            self.cursors.usage.clone()
        } else {
            element
        }
        .or_else(|| self.cursors.default.clone());
        overlay.screen = None;
        if game.phase != Phase::World {
            self.draw_before_world(game, overlay);
            return;
        }
        // The player's figure in the inventory window, over the window's quads drawn before it.
        let figure = self
            .ui
            .goods
            .inventory
            .figure()
            .filter(|_| game.phase == Phase::World && !self.logging_out)
            .and_then(|(r, mark)| {
                let scene = self.figure.scene(game.world, game.looks, r)?;
                Some((scene, r, mark))
            });
        let spaces: Vec<_> = figure
            .iter()
            .map(|(_, r, mark)| {
                (
                    auberean_shell::interface::PreviewSpace::PaperDoll,
                    *r,
                    *mark,
                )
            })
            .collect();
        self.overlay
            .compose(overlay, game.size, self.ui.art(), &self.list, &spaces);
        overlay.scenes.extend(figure.map(|(scene, _, _)| scene));
    }

    fn enter(&mut self, game: &Game<'_>) {
        self.held = 0;
        self.last_now = None;
        self.was_in_world = game.phase == Phase::World;
        // A session another interface began is the one Horizon goes on with.
        if game.phase != Phase::Disconnected {
            self.connect_asked = true;
            self.started = true;
        }
    }

    fn leave(&mut self, game: &Game<'_>, requests: &mut Vec<Request>) {
        self.reopen = (game.phase == Phase::World).then(|| Reopen::of(&self.ui));
        let mut asked = Vec::new();
        self.pad.let_go(&mut asked);
        self.ui.close_all(&mut asked);
        // The story's scenes stop where they are: the world shown again, the game told.
        self.ui.story.stop(requests, &mut asked);
        requests.extend(asked.into_iter().map(Request::Act));
        if std::mem::take(&mut self.orbit_attached) {
            requests.push(Request::Preferences(PageRequest::Detach(vec![
                section::ORBIT,
            ])));
        }
        let pages = std::mem::take(&mut self.pages_attached);
        if !pages.is_empty() {
            requests.push(Request::Preferences(PageRequest::Detach(pages)));
        }
        self.words_for = None;
        self.orbit_for = None;
        self.taken_keys.clear();
        self.taken_buttons = [false; 3];
        self.picking = false;
        self.drop_picking = false;
        self.pressed_at = None;
        self.input = InputFrame::default();
    }

    fn release_textures(&mut self, overlay: &mut Overlay) {
        self.overlay.release(overlay);
        if std::mem::take(&mut self.ring_uploaded) {
            overlay.release(dereth_ui_kit::selection::RING_TEXTURE);
            overlay.release(dereth_ui_kit::selection::RING_INNER_TEXTURE);
        }
    }

    fn wants_pointer(&self, game: &Game<'_>) -> bool {
        if game.phase != Phase::World {
            return true;
        }
        self.ui.window_open() || self.ui.typing() || self.ui.questions.is_up(&self.state)
    }

    fn text_mode(&self) -> bool {
        self.ui.typing() || self.ui.pregame.typing()
    }

    /// A click on the window in the background counts, as in most programs: a question's answer
    /// isn't lost to the click that brought the window forward.
    fn hears_the_pointer_in_background(&self) -> bool {
        true
    }

    fn set_random(&mut self, random: &auberean_primitives::random::Random) {
        self.random = Some(random.clone());
    }

    fn set_clipboard(&mut self, text: Option<String>) {
        self.input.paste = text;
    }

    fn take_copied(&mut self) -> Option<String> {
        self.input.copied.take()
    }
}
