//! The play screen: the frame every panel of the world hangs from.
//!
//! [`GamePlay`] is the play screen's own framework. Made, it sets the display to the player's
//! size and makes the camera's panel; once the player is in the world (the login finished) it makes
//! its panels, each a child framework ([`Child`]), and from then on:
//!
//! - it hears the interface keys: the toolbar's, the status bar's and the radar's keys show and
//!   hide them; one key hides the whole interface (not while a dialog waits on the player); the use
//!   key uses what is selected; the log-out key ends the session at the next tick;
//! - it hands each panel what its listener heard, the notices the player's scripts send (each to
//!   the panels registered for it, in the order they registered: [`crate::notices`]), and what
//!   the game shows this frame ([`View`]); a panel says what it asks of the client as
//!   [`Request`]s, and a notice it sends the others takes effect there and then;
//! - it keeps the dialogs it put up (please wait, death and resurrection, a confirmation, the
//!   game's messages) by the number each was given, and acts when one closes;
//! - it ends the session: the panels gone, the character logged off, the flow back to the
//!   character list.
//!
//! The panels, in the order the play screen makes them: the names over heads, the radar, the
//! stack splitter, the selection, the status bar, the toolbar (with its own panels: the shortcut
//! bar, the chat windows and the toolbar's windows), the fellowship's small panel, trade, an
//! external container, the book, the shop, its catalog, consignments and new sales, the pet and
//! the ingredients. A panel not written yet is a [`Pending`] framework with nothing in it, at its
//! place in [`GamePlay::setup_children`]. The story's scenes play over all of them
//! ([`crate::story`]).

use std::any::Any;

use auberean_ui::framework::FrameId;
use auberean_ui::manager::global;
use auberean_ui::{Handle, Mail, Token, Ui};

use crate::camera::{self, Camera};
use crate::flow;
use crate::{names, radar, selection, statusbar, toolbar};

/// The play layout's enumeration value (the radar, the selection, the dialogs, the story).
pub const PLAY_LAYOUT_ENUM: u32 = 0x4100_0009;

mod name {
    use auberean_ui::Token;
    /// The field that plays the incoming message sounds.
    pub const JUKEBOX_FIELD: Token = Token::of("JukeBoxField");
}

/// The interface actions the play screen hears.
pub mod action {
    /// Show and hide the toolbar, the status bar, the radar.
    pub const TOOLBAR_PANEL: u32 = 0x4100_0008;
    pub const STATUSBAR_PANEL: u32 = 0x4100_0009;
    pub const RADAR: u32 = 0x4100_000B;
    /// Show and hide the whole interface.
    pub const ALL_UI: u32 = 0x4100_000C;
    /// Use what is selected.
    pub const USE: u32 = 0x4100_000D;
    /// Log out.
    pub const LOGOUT: u32 = 0x4100_000E;
}

/// The player's scripts' notices to the panels, by the native that sends each (its package, then
/// its number in the package).
pub mod notice {
    /// The player died (1) or lives again (0).
    pub const UPDATE_DEATH_STATE: u32 = 0x0085 << 16 | 0x0008;
    /// The player's options changed: the panels' kept places among them.
    pub const RELOAD_OPTIONS: u32 = 0x0085 << 16 | 0x0009;
    /// The panels' places to be written to a file: its name, as a string reference.
    pub const SAVE_UI: u32 = 0x0085 << 16 | 0x001C;
    /// The panels' places to be read back from a file: its name, as a string reference.
    pub const LOAD_UI: u32 = 0x0085 << 16 | 0x001D;
    /// A message for a dialog of its own.
    pub const DISPLAY_MESSAGE: u32 = 0x0085 << 16 | 0x000C;
    /// Someone offers to bring the player back to life.
    pub const RESURRECTION_REQUESTED_BY_OTHER: u32 = 0x0085 << 16 | 0x0010;
    /// The server asks whether an effect may go on: its target, its source, the effect, the
    /// question.
    pub const SHOULD_APPLY_TOTEM: u32 = 0x0092 << 16 | 0x0003;
    /// The player mounted (1) or dismounted (0).
    pub const UPDATE_MOUNT_STATE: u32 = 0x0085 << 16 | 0x0011;
    /// The player is attacking (1) or not (0).
    pub const UPDATE_ATTACK_STATE: u32 = 0x0086 << 16 | 0x0001;
    /// A story's scenes to play (the record: the scenes).
    pub const PLAY_SEQUENCE: u32 = 0x008E << 16 | 0x0003;
    /// A scene on the way through a portal: whether one plays (a word), and the scenes (the
    /// record).
    pub const PORTAL_SCENE: u32 = 0x0091 << 16 | 0x0003;
    /// A sale of the player's waits to be collected (1) or none does (0).
    pub const SHOW_SALE_REMINDER: u32 = 0x1AD3 << 16 | 0x000B;
    /// A profile of what is examined (the record: the profile).
    pub const UPDATE_EXAMINATION_PROFILE: u32 = 0x0089 << 16 | 0x0001;
    /// What is examined changed (the record: the request).
    pub const EXAMINATION_REQUEST_CHANGED: u32 = 0x0089 << 16 | 0x0002;
    /// Open the examination window.
    pub const OPEN_EXAMINATION_PANEL: u32 = 0x0091 << 16 | 0x0002;
    /// The journal's quests changed: how (a word), and the quest (the record).
    pub const UPDATE_QUESTS: u32 = 0x008E << 16 | 0x0001;
    /// The story's acts changed.
    pub const UPDATE_STORY_QUESTS: u32 = 0x008E << 16 | 0x0002;
    /// A skill changed (a word: the skill).
    pub const UPDATE_SKILL: u32 = 0x0085 << 16 | 0x0006;
    /// The skill panel's points changed.
    pub const UPDATE_SKILL_PANEL: u32 = 0x0085 << 16 | 0x0005;
    pub const UPDATE_SKILL_PANEL_FULL: u32 = 0x0085 << 16 | 0x000F;
}

/// What the dialogs the play screen puts up are: their kind as the dialogs know it.
pub mod dialog {
    /// A question answered yes or no.
    pub const CONFIRMATION: u32 = 0x4000_0001;
    /// Please wait, with a cancel.
    pub const WAIT: u32 = 0x4000_0002;
    /// A message with one answer (death: be brought back).
    pub const ONE_ANSWER: u32 = 0x4000_0003;
    /// A message to read.
    pub const MESSAGE: u32 = 0x4000_0004;
}

/// A notice the player's scripts sent the panels: the native that sent it, the object it is
/// about, and its other arguments as the script machine holds them (a word each), with the record
/// each names that a panel reads (none for a plain value).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Notice {
    pub native: u32,
    pub about: Option<u64>,
    pub words: Vec<u32>,
    pub records: Vec<Option<Record>>,
}

/// A record a notice's argument names, as the panels read it.
#[derive(Clone, Debug, PartialEq)]
pub enum Record {
    /// An examination profile.
    Profile(crate::examination::Profile),
    /// An examination request.
    Request(crate::examination::ExamRequest),
    /// The player's yes-or-no options (the options read again).
    OptionBits(u32),
    /// A quest.
    Quest(crate::quests::Quest),
    /// The player's quests (the whole list read).
    Quests(Vec<crate::quests::Quest>),
    /// Scenes of the story to play.
    Scenes(crate::story::Scenes),
    /// A string reference (a name, a product), as the scripts gave it.
    Text(auberean_text::StringInfo),
    /// A shop (its sales, or a player's consignments), as the scripts filled it.
    Store(crate::shop::Store),
    /// A catalog's shops.
    Stores(Vec<crate::shop::Store>),
    /// A fellowship, a fellow.
    Fellowship(Box<crate::social::Fellowship>),
    Fellow(Box<crate::social::Fellow>),
    /// An allegiance's profile.
    Allegiance(Box<crate::social::Profile>),
    /// The chat service's friends, or one friend's state.
    Friends(Vec<crate::social::UserStatus>),
    Friend(crate::social::UserStatus),
    /// A hit's damage text.
    DamageText(crate::names::DamageBlob),
}

impl Notice {
    /// The argument after the object, as a one-byte flag.
    #[must_use]
    pub fn flag(&self) -> bool {
        self.words.first().is_some_and(|w| w & 0xFF != 0)
    }

    /// The argument after the object, whole.
    #[must_use]
    pub fn word(&self) -> u32 {
        self.words.first().copied().unwrap_or(0)
    }

    /// The word at `at`.
    #[must_use]
    pub fn word_at(&self, at: usize) -> u32 {
        self.words.get(at).copied().unwrap_or(0)
    }

    /// A 64-bit argument (an instance id) whose high word is at `at`, its low word next, as the
    /// script machine holds one.
    #[must_use]
    pub fn long(&self, at: usize) -> u64 {
        u64::from(self.word_at(at)) << 32 | u64::from(self.word_at(at + 1))
    }
}

/// What the game shows on the play screen this frame, for the panels.
#[derive(Clone, Copy, Debug, Default)]
pub struct View<'a> {
    /// The player's health, its greatest, vigor, its greatest.
    pub vitals: Option<[u32; 4]>,
    /// The radar's place line.
    pub place: Option<&'a str>,
    /// Lines for the chat windows, each with the kind of text it is (the client's own errors are
    /// of the error kind).
    pub lines: &'a [(u32, String)],
    /// For each of `lines` (when given), the string reference it was written from: the chat
    /// windows add it to their logs as a reference, written once the string tables it reads have
    /// come.
    pub line_infos: &'a [Option<auberean_text::StringInfo>],
    /// How many of `lines` were written before the scripts said the options changed, if they did
    /// this frame: the chat windows take those by what they show now.
    pub lines_before_options: usize,
    /// The chat's part of the player's options, as last read, and a new player's (the options
    /// window's chat page's defaults).
    pub chat: Option<&'a crate::chat::ChatSettings>,
    pub chat_defaults: Option<&'a crate::chat::ChatSettings>,
    /// What the shortcut bar asks of the player's scripts.
    pub shortcuts: Option<&'a crate::shortcuts::Source>,
    /// The effects on the player, as the scripts list them.
    pub effects: Option<&'a crate::statusbar::EffectList>,
    /// The connection's health, as the client last measured it.
    pub link: Option<crate::statusbar::Link>,
    /// The world as the panels see it, once the player is in it.
    pub world: Option<&'a crate::world::World>,
    /// Every notice of the player's scripts this frame, about any object (the panels that hear
    /// other objects' notices read them here).
    pub everyone: &'a [Notice],
    /// The objects that went from the world since the last frame.
    pub gone: &'a [u64],
    /// How many pixels at the screen's top and foot the panels there cover (what the world's
    /// words keep clear).
    pub preclude: (i32, i32),
    /// The inventory as the player's scripts answer for it.
    pub inventory: Option<&'a crate::inventory::Facts>,
    /// The player's options as the client last read them (the options window's character page).
    pub options: Option<crate::options::Values>,
    /// The client's own preferences as last listed (the options window's client page).
    pub preferences: Option<&'a crate::options::client::Listing>,
    /// The client's own preferences it changed itself since the last frame.
    pub preferences_changed: &'a [crate::options::helper::Change],
    /// The shift key is held.
    pub shift: bool,
    /// The skill window's panels and skills, as the player's scripts last answered.
    pub skills: Option<&'a crate::skill_tree::Facts>,
    /// The story's acts, as the player's scripts list them.
    pub acts: Option<&'a [crate::story_quests::Act]>,
    /// What the map draws on, from the region.
    pub atlas: Option<&'a std::sync::Arc<crate::map::Atlas>>,
    /// The game's date and time, from the calendar.
    pub clock: Option<crate::map::Clock>,
    /// The crafting page's craft skills and recipes, as the player's scripts last answered.
    pub craft: Option<&'a crate::craft::Facts>,
    /// What the scripts answer for the shop panels.
    pub shop: Option<&'a crate::shop::Facts>,
}

/// What the examination panel is asked to show, and the selection it comes with: an effect, an
/// item, a skill, a recipe, the vitae (by `kind`), with the datum the kind uses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Examination {
    pub kind: u32,
    pub data_enum: u32,
    pub item: u64,
    pub data_id: u32,
    pub selection: u64,
}

/// A dialog a panel asks for: the number it knows it by (from [`crate::dialog::new_context`]),
/// its kind, its text.
#[derive(Clone, Debug, PartialEq)]
pub struct Dialog {
    pub id: u32,
    pub kind: u32,
    pub text: String,
    /// The string reference its text is written from, when it came as one: given to the dialog's
    /// text element, which writes it once the string tables it reads are at hand.
    pub info: Option<auberean_text::StringInfo>,
    /// Whether it goes up at once in place of the dialog up, which waits behind it (else it waits
    /// its turn).
    pub modal: bool,
}

/// What the play screen's panels ask of the client.
#[derive(Clone, Debug, PartialEq)]
pub enum Request {
    /// The camera's.
    Camera(camera::Request),
    /// The inventory's.
    Inventory(crate::inventory::Request),
    /// The options window's.
    Options(crate::options::Request),
    /// A notice for the panels from one of them (the client's own notices between its panels,
    /// numbered below `0x10000`): the play screen hands it to every panel at once.
    Notice(Notice),
    /// Start attacking what is selected, or stop.
    Attack {
        start: bool,
    },
    /// Get off the mount.
    Dismount,
    /// Show the sales waiting to be collected.
    ShowSales,
    /// Use what is selected.
    UseSelected,
    /// Select an object (0: nothing): the player's scripts take it and the player looks at it.
    Select(u64),
    /// Select an object (0: nothing) from a panel: the player's scripts take it and the player's
    /// head stays as it is.
    SetSelection(u64),
    /// Find the object under the pointer in the world as last drawn (the answer comes with a
    /// later frame's world).
    Pick {
        x: i32,
        y: i32,
    },
    /// Stop attacking an object.
    CeaseAttack(u64),
    /// The radar's map coordinates shown or hidden, and names over heads, kept in the player's
    /// options.
    ShowCoordinates(bool),
    ShowNames(bool),
    /// The music's groove level: what the objects around add up to.
    GrooveLevel(i32),
    /// Open the examination panel on the selection.
    OpenExaminationPanel,
    /// The help panel, or the plug-in manager, opened or closed.
    Help {
        open: bool,
    },
    PluginManager {
        open: bool,
    },
    /// How many pixels at the screen's foot the toolbar covers (0 when it is hidden).
    PrecludeFromBottom(i32),
    /// The display at the player's own size (the world), or the screens' before it.
    DisplayInWorld(bool),
    /// The sound playing across the screens before the world, stopped.
    StopScreenSound,
    /// A sound played (a sound table's record).
    Sound(u32),
    /// The player's commands made known to the chat window's command line.
    KnowCommands,
    /// The player's options sent to the server to keep.
    KeepOptions,
    /// Examine with these tabs (the examination window shown, or its tabs changed), or stop
    /// examining (hidden).
    Examination {
        tabs: Option<u32>,
    },
    /// Give up a quest.
    CancelQuest {
        quest: u32,
    },
    /// The panels' places kept with the player's options and sent to the server to keep.
    SaveLocations,
    /// The panels' places put back where the player's options keep them: read from the options
    /// that came again (`reloaded`), else asked of the player's interface.
    PutBackLocations {
        reloaded: bool,
    },
    /// The panels' places written to the file of this name (empty: the file of this character,
    /// world and screen).
    SaveLayout {
        name: String,
    },
    /// The panels' places read back from the file of this name (empty as for saving).
    LoadLayout {
        name: String,
    },
    /// A line for the chat windows, as one of the client's own errors.
    ErrorLine(String),
    /// The chat panel's.
    Chat(crate::chat::Ask),
    /// A dialog up, or down.
    Dialog(Dialog),
    DialogDown(u32),
    /// Bring the player back to life, or turn down someone's offer to.
    Resurrect,
    DeclineResurrection,
    /// Yes to the server's question whether an effect may go on: its target and source, and the
    /// effect.
    Confirm {
        objects: [u64; 2],
        value: u32,
    },
    /// The story's scenes are done (or none was played): the scenes, as the scripts handed them.
    PlayScenesDone(Vec<u8>),
    /// The world shown again, or hidden (while the story plays).
    ShowWorld(bool),
    /// A scene on the way through a door plays in place of portal space, or no longer does.
    PortalScenePlaying(bool),
    /// The story's music or voice played or stopped.
    StorySound(crate::story::Sound),
    /// The trade panel's.
    Trade(crate::trade::TradeRequest),
    /// The shop panels'.
    Shop(crate::shop::ShopRequest),
    /// The pet panel's.
    Pet(crate::pet::PetRequest),
    /// Log the character off: the session ends.
    LogOff,
    /// Back to the character list.
    CharacterSelection,
    /// The program ends.
    Quit,
    /// Use what a toolbar slot holds.
    UseSlot(u32),
    /// Put a shortcut in a toolbar slot (the player's scripts keep it).
    PutInSlot {
        slot: u32,
        shortcut: crate::shortcuts::Shortcut,
    },
    /// The shortcut bar shows this set (from 1): the player's scripts keep it.
    ShortcutSet(u32),
    /// The shortcut bar was resized by hand to this many rows.
    ShortcutRows(i32),
    /// Select (the selection given) and open the examination panel on something.
    Examine(Examination),
    /// Train, untrain or raise a skill, or stop an untraining.
    Skill(crate::skill_tree::SkillRequest),
    /// Open a recipe or raise a craft skill.
    Craft(crate::craft::CraftRequest),
    /// The ingredient panel's: a recipe at a spinner value read, an item put in a slot, the
    /// recipe made.
    Ingredients(crate::ingredients::IngredientRequest),
    /// Play a scene of the story again.
    PlayScene {
        act: u32,
        scene: u32,
    },
    /// How many pixels at the screen's top the status bar covers (0 when it is hidden).
    PrecludeFromTop(i32),
    /// The social panels' (the fellowship, the allegiance, the friends).
    Social(crate::social::Request),
}

/// One panel of the play screen: a child framework.
pub trait Child: Any + std::fmt::Debug {
    /// Its framework.
    fn frame(&self) -> FrameId;
    /// Show or hide it (its own flag; it shows only while the play screen does).
    fn show(&mut self, ui: &mut Ui, on: bool, _out: &mut Vec<Request>) {
        ui.frame_show(self.frame(), on);
    }
    /// Whether it shows.
    fn shown(&self, ui: &Ui) -> bool {
        ui.frame_shown(self.frame())
    }
    /// What its listener heard (besides being told to show).
    fn listen(&mut self, _ui: &mut Ui, _mail: &Mail, _out: &mut Vec<Request>) {}
    /// A notice of the player's scripts; one a panel has no handling of its own for does nothing
    /// (its arguments let go).
    // D2-00033, D2-00034, D2-00070, D2-00406, D2-00407, D2-00408, D2-00409, D2-00706, D2-00707
    // D2-00708, D2-00734, D2-00735, D2-00740, D2-16962, D2-17634
    fn notice(&mut self, _ui: &mut Ui, _n: &Notice, _out: &mut Vec<Request>) {}
    /// A notice, with what the game shows this frame (for a panel that needs the world to act on
    /// one); by default as [`Child::notice`].
    fn notice_seen(&mut self, ui: &mut Ui, _view: &View<'_>, n: &Notice, out: &mut Vec<Request>) {
        self.notice(ui, n, out);
    }
    /// The notices it registers for as it is made, in the order it registers them, its own
    /// panels' with its own ([`crate::notices`]): it hears no other.
    fn notices(&self) -> Vec<u32> {
        Vec::new()
    }
    /// What the game shows this frame.
    fn update(&mut self, _ui: &mut Ui, _view: &View<'_>, _out: &mut Vec<Request>) {}
    /// It goes: its elements and its listening.
    fn close(self: Box<Self>, ui: &mut Ui);
    /// Itself, to be reached by what holds it.
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

/// What a panel's listener heard, acted on: told to show, it shows by its own flag (as a
/// framework shown or hidden tells each of its panels at once).
pub fn hear(child: &mut dyn Child, ui: &mut Ui, out: &mut Vec<Request>) {
    for m in camera::take(ui, child.frame()) {
        match m {
            Mail::Show { shown } => child.show(ui, shown, out),
            m => child.listen(ui, &m, out),
        }
    }
}

/// Run a panel for a frame: what its listener heard (told to show: shown by its own flag), the
/// notices, then what the game shows.
pub fn drive(
    child: &mut dyn Child,
    ui: &mut Ui,
    view: &View<'_>,
    notices: &[Notice],
    out: &mut Vec<Request>,
) {
    hear(child, ui, out);
    for n in notices {
        child.notice(ui, n, out);
    }
    child.update(ui, view, out);
}

/// A notice handed to a panel of a framework's own, when the panel registered for it.
pub fn hand(child: &mut dyn Child, ui: &mut Ui, n: &Notice, out: &mut Vec<Request>) {
    if child.notices().contains(&n.native) {
        child.notice(ui, n, out);
    }
}

/// A panel not written yet: its framework, with nothing in it, at its place among the play
/// screen's panels.
#[derive(Debug)]
pub struct Pending {
    frame: FrameId,
    /// Which panel it stands for.
    pub what: &'static str,
}

impl Pending {
    /// The framework of the panel `what` under `parent`.
    pub fn new(ui: &mut Ui, parent: FrameId, what: &'static str) -> Self {
        Self {
            frame: ui.new_child_frame(parent),
            what,
        }
    }
}

impl Child for Pending {
    fn frame(&self) -> FrameId {
        self.frame
    }
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

/// A panel of the play screen and the key that shows and hides it (0 for none).
#[derive(Debug)]
pub struct ChildInfo {
    pub child: Box<dyn Child>,
    pub keypress: u32,
    /// Its number among the play screen's panels, by which it is registered for its notices.
    pub id: u32,
}

/// Who hears a notice on the play screen: the play screen itself (with its camera), or a panel by
/// its number.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handler {
    Screen,
    Panel(u32),
}

/// The notices the play screen registers for as it is made (its camera's with them), and those
/// it registers for once its panels are made, after them; by their own numbers.
const MADE_NOTICES: [u32; 14] = [
    0x455, 0x1_0419, 0x1_0413, 0x1_0401, 0x442, 0x444, 0x445, 0x449, 0x498, 0x44E, 0x44F, 0x450,
    0x1_0418, 0x467,
];
const LOGIN_NOTICES: [u32; 10] = [
    0x41C, 0x411, 0x417, 0x1_040C, 0x451, 0x478, 0x4A4, 0x4A5, 0x4A6, 0x436,
];

/// A question the player's scripts asked through a confirmation: the two objects and two tokens
/// it carried and its value, answered with them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Confirmation {
    id: u32,
    objects: [u64; 2],
    value: u32,
}

/// The play screen.
#[derive(Debug)]
pub struct GamePlay {
    frame: FrameId,
    camera: Option<Camera>,
    jukebox: Option<Handle>,
    /// The panels, in the play screen's list (the last made first).
    children: Vec<ChildInfo>,
    /// Who hears which notice, in the order they registered; the next panel's number.
    registry: crate::notices::Registry<Handler>,
    next_panel: u32,
    /// The session ended; asked to end at the next tick; going back to the character list.
    ended: bool,
    end_asked: bool,
    back: bool,
    /// The dialogs up: please wait, death or resurrection, a confirmation, messages (0: none).
    wait: u32,
    resurrect: u32,
    confirmation: Option<Confirmation>,
    messages: Vec<u32>,
    /// A story playing over the screen.
    story: Option<crate::story::Story>,
    /// A scene plays on the way through a portal (the story it makes waits for the player to
    /// arrive).
    playing_portal_scene: bool,
}

impl GamePlay {
    /// The play screen made: its framework; the sound of the screens before it stopped; the
    /// display at the player's own size; the camera's panel; the clock and the display's being set
    /// up again heard; its first notices registered for. Its panels come with
    /// [`Self::login_finished`].
    // D2-01934, D2-01935, D2-01943, D2-11906
    pub fn new(ui: &mut Ui, out: &mut Vec<Request>) -> Self {
        let frame = ui.new_main_frame();
        out.push(Request::StopScreenSound);
        out.push(Request::DisplayInWorld(true));
        let mut cam = Vec::new();
        let camera = Camera::new(ui, frame, &mut cam);
        out.extend(cam.into_iter().map(Request::Camera));
        if let Some(l) = ui.frame_listener(frame) {
            ui.listen_for_global_message(l, global::TIME);
            ui.listen_for_global_message(l, global::REFRESH_START);
        }
        let mut registry = crate::notices::Registry::new();
        registry.register_all(&crate::notices::keys(&MADE_NOTICES), Handler::Screen);
        Self {
            frame,
            camera: Some(camera),
            jukebox: None,
            children: Vec::new(),
            registry,
            next_panel: 0,
            ended: false,
            end_asked: false,
            back: false,
            wait: 0,
            resurrect: 0,
            confirmation: None,
            messages: Vec::new(),
            story: None,
            playing_portal_scene: false,
        }
    }

    /// The play screen's framework.
    #[must_use]
    pub const fn frame(&self) -> FrameId {
        self.frame
    }

    /// The panels.
    #[must_use]
    pub fn children(&self) -> &[ChildInfo] {
        &self.children
    }

    /// The first panel of a kind.
    #[must_use]
    pub fn child<T: Child>(&self) -> Option<&T> {
        self.children
            .iter()
            .find_map(|c| c.child.as_any().downcast_ref::<T>())
    }

    /// The first panel of a kind, to change.
    pub fn child_mut<T: Child>(&mut self) -> Option<&mut T> {
        self.children
            .iter_mut()
            .find_map(|c| c.child.as_any_mut().downcast_mut::<T>())
    }

    /// The camera's panel.
    #[must_use]
    pub const fn camera(&self) -> Option<&Camera> {
        self.camera.as_ref()
    }

    /// Whether the session has ended.
    #[must_use]
    pub const fn ended(&self) -> bool {
        self.ended
    }

    /// Show or hide the whole play screen; not while a dialog waits on the player (please wait,
    /// death, a confirmation) or a story plays.
    // D2-01900
    pub fn show(&mut self, ui: &mut Ui, on: bool, out: &mut Vec<Request>) {
        if self.wait == 0
            && self.resurrect == 0
            && self.story.is_none()
            && self.confirmation.is_none()
        {
            self.framework_show(ui, on, out);
        }
    }

    /// The framework shown or hidden, each panel told at once.
    fn framework_show(&mut self, ui: &mut Ui, on: bool, out: &mut Vec<Request>) {
        ui.frame_show(self.frame, on);
        for c in &mut self.children {
            hear(c.child.as_mut(), ui, out);
        }
    }

    /// Whether the play screen shows.
    #[must_use]
    pub fn shown(&self, ui: &Ui) -> bool {
        ui.frame_shown(self.frame)
    }

    /// The player is in the world: the please-wait dialog down, the panels made, the interface
    /// keys heard, the player's commands made known; portal space left behind.
    // D2-01923, D2-01926
    pub fn login_finished(&mut self, ui: &mut Ui, out: &mut Vec<Request>) {
        self.close_dialog(self.wait, out);
        self.wait = 0;
        self.setup_children(ui, out);
        if let Some(l) = ui.frame_listener(self.frame) {
            ui.listen_for_global_message(l, global::KEY_DOWN);
            ui.listen_for_global_message(l, global::KEY_UP);
        }
        out.push(Request::KnowCommands);
        self.registry
            .register_all(&crate::notices::keys(&LOGIN_NOTICES), Handler::Screen);
    }

    /// The panels, made in the play screen's order, each with the key that shows and hides it,
    /// kept in its list (the last made first); the field that plays message sounds.
    // D2-01920
    pub fn setup_children(&mut self, ui: &mut Ui, out: &mut Vec<Request>) {
        let f = self.frame;
        self.jukebox = ui.frame_create_root(f, PLAY_LAYOUT_ENUM, name::JUKEBOX_FIELD);
        // Made in this order, each registering for its notices as it is made.
        let names = self.made(Box::new(names::Names::new(ui, f)));
        let radar = self.made(Box::new(radar::Radar::new(ui, f)));
        let stack = self.made(Box::new(crate::inventory::stack::StackSplitter::new(ui, f)));
        let selection = self.made(Box::new(selection::Selection::new(ui, f)));
        let status = self.made(Box::new(statusbar::StatusBar::new(ui, f, out)));
        let toolbar = self.made(Box::new(toolbar::Toolbar::new(ui, f, out)));
        let fellows = self.made(Box::new(crate::social::mini::MiniFellowship::new(ui, f)));
        let trade = self.made(Box::new(crate::trade::Trade::new(ui, f)));
        let container = self.made(Box::new(
            crate::inventory::external::ExternalContainer::new(ui, f),
        ));
        let book = self.made(Box::new(crate::book::Book::new(ui, f)));
        let shop = self.made(Box::new(crate::shop::Shop::new(ui, f)));
        let catalog = self.made(Box::new(crate::shop::catalog::Catalog::new(ui, f)));
        let consignments = self.made(Box::new(crate::shop::consignment::Consignments::new(ui, f)));
        let new_sale = self.made(Box::new(crate::shop::new_sale::NewSale::new(ui, f)));
        let pet = self.made(Box::new(crate::pet::Pets::new(ui, f, out)));
        let ingredients = self.made(Box::new(crate::ingredients::Ingredients::new(ui, f)));
        // Put in the list in this order, each at its front: the list runs from the last put.
        let put = [
            (names, 0),
            (radar, action::RADAR),
            (stack, 0),
            (toolbar, action::TOOLBAR_PANEL),
            (status, action::STATUSBAR_PANEL),
            (selection, 0),
            (fellows, 0),
            (trade, 0),
            (container, 0),
            (shop, 0),
            (catalog, 0),
            (consignments, 0),
            (new_sale, 0),
            (book, 0),
            (ingredients, 0),
            (pet, 0),
        ];
        for ((child, id), keypress) in put {
            self.children.insert(
                0,
                ChildInfo {
                    child,
                    keypress,
                    id,
                },
            );
        }
    }

    /// A panel just made: numbered, and registered for its notices.
    fn made(&mut self, child: Box<dyn Child>) -> (Box<dyn Child>, u32) {
        let id = self.next_panel;
        self.next_panel += 1;
        self.registry
            .register_all(&child.notices(), Handler::Panel(id));
        (child, id)
    }

    /// Who hears notice `native`, in the order they registered.
    #[must_use]
    pub fn handlers(&self, native: u32) -> &[Handler] {
        self.registry.handlers(native)
    }

    /// The panel numbered `id`.
    #[must_use]
    pub fn panel(&self, id: u32) -> Option<&dyn Child> {
        self.children
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.child.as_ref())
    }

    /// A notice handed to whoever registered for it, in the order they registered: the play
    /// screen's own (leaving the world and quitting among them), the panels'.
    fn deliver(&mut self, ui: &mut Ui, view: &View<'_>, n: &Notice, out: &mut Vec<Request>) {
        for h in self.registry.handlers(n.native).to_vec() {
            match h {
                Handler::Screen => match n.native {
                    crate::options::notice::LEAVE_WORLD => self.end_character_session(),
                    crate::options::notice::QUIT => Self::logoff(out),
                    _ => self.notice(ui, n, out),
                },
                Handler::Panel(id) => {
                    if let Some(c) = self.children.iter_mut().find(|c| c.id == id) {
                        c.child.notice_seen(ui, view, n, out);
                    }
                }
            }
        }
    }

    /// Show or hide the panel a key is for: the first in the list with that key.
    // D2-01917
    pub fn child_key_press(&mut self, ui: &mut Ui, key: u32, out: &mut Vec<Request>) {
        if let Some(c) = self.children.iter_mut().find(|c| c.keypress == key) {
            let shown = c.child.shown(ui);
            c.child.show(ui, !shown, out);
        }
    }

    /// The play screen's frame: what its listener heard, the camera's, each panel's, the
    /// notices of the player's scripts, what the game shows.
    pub fn tick(
        &mut self,
        ui: &mut Ui,
        view: &View<'_>,
        notices: &[Notice],
        out: &mut Vec<Request>,
    ) {
        for m in camera::take(ui, self.frame) {
            self.listen(ui, &m, out);
        }
        if let Some(c) = &mut self.camera {
            let mut cam = Vec::new();
            c.tick(ui, &mut cam);
            out.extend(cam.into_iter().map(Request::Camera));
        }
        for n in notices {
            self.deliver(ui, view, n, out);
        }
        for c in &mut self.children {
            drive(c.child.as_mut(), ui, view, &[], out);
        }
        if let Some(s) = &mut self.story {
            s.tick(ui, out);
        }
        self.pass_notices(ui, view, out);
    }

    /// The story playing over the screen, if one is.
    #[must_use]
    pub const fn story(&self) -> Option<&crate::story::Story> {
        self.story.as_ref()
    }

    /// The panels' notices to each other, handed to whoever registered for each as it is sent:
    /// what they ask in turn takes its place, before what was asked after the notice. The
    /// shortcut bar resized by hand tells the options window its rows.
    fn pass_notices(&mut self, ui: &mut Ui, view: &View<'_>, out: &mut Vec<Request>) {
        let asked = std::mem::take(out);
        *out = self.passed(ui, view, asked, 0);
    }

    fn passed(
        &mut self,
        ui: &mut Ui,
        view: &View<'_>,
        asked: Vec<Request>,
        depth: u32,
    ) -> Vec<Request> {
        let mut done = Vec::with_capacity(asked.len());
        for r in asked {
            let r = match r {
                Request::ShortcutRows(rows) => Request::Notice(Notice {
                    native: crate::options::notice::SET_SHORTCUT_HEIGHT,
                    words: vec![rows.cast_unsigned()],
                    ..Notice::default()
                }),
                r => r,
            };
            match r {
                Request::Notice(n) if depth < 8 => {
                    let mut more = Vec::new();
                    self.deliver(ui, view, &n, &mut more);
                    done.extend(self.passed(ui, view, more, depth + 1));
                }
                r => done.push(r),
            }
        }
        done
    }

    /// What the play screen's listener heard: the panels' keys (while it shows), the key that
    /// hides the whole interface (not while a dialog waits or the session ends), the use key, the
    /// log-out key (the session ends at the next tick); the display set up again (the panels'
    /// places put back from the player's options, as the client reads them).
    // D2-01963
    pub fn listen(&mut self, ui: &mut Ui, mail: &Mail, out: &mut Vec<Request>) {
        match *mail {
            Mail::Global {
                what: global::KEY_DOWN,
                param,
            } => match param {
                action::TOOLBAR_PANEL | action::STATUSBAR_PANEL | action::RADAR => {
                    if self.shown(ui) {
                        self.child_key_press(ui, param, out);
                    }
                }
                action::ALL_UI => {
                    if self.resurrect == 0 && !self.ended && self.confirmation.is_none() {
                        let shown = ui.frame_shown(self.frame);
                        self.show(ui, !shown, out);
                    }
                }
                action::USE => out.push(Request::UseSelected),
                action::LOGOUT => self.end_asked = true,
                _ => {}
            },
            Mail::Global {
                what: global::TIME, ..
            } if !self.ended && self.end_asked => {
                self.end_session(ui, true, out);
                self.end_asked = false;
            }
            Mail::Global {
                what: global::REFRESH_START,
                ..
            } => out.push(Request::PutBackLocations { reloaded: false }),
            _ => {}
        }
    }

    /// A notice of the player's scripts for the play screen itself; the dialogs' answers; a
    /// dialog asked for or gone (mouse look waits on dialogs).
    pub fn notice(&mut self, ui: &mut Ui, n: &Notice, out: &mut Vec<Request>) {
        use crate::dialog::notice as answer;
        match n.native {
            notice::UPDATE_DEATH_STATE => self.update_death_state(ui, n.word() != 0, out),
            notice::RELOAD_OPTIONS => self.reload_options(out),
            notice::SAVE_UI | notice::LOAD_UI => Self::layout_notice(ui, n, out),
            notice::PLAY_SEQUENCE => self.play_sequence(ui, &scenes_of(n), out),
            // The message's string reference goes to the dialog's text element as it is; its
            // second word says whether it goes up at once, in place of the dialog up.
            notice::DISPLAY_MESSAGE => {
                let info = n.records.iter().flatten().find_map(|r| match r {
                    Record::Text(t) => Some(t.clone()),
                    _ => None,
                });
                self.display_message(String::new(), info, n.word_at(1) != 0, out);
            }
            notice::SHOULD_APPLY_TOTEM => {
                let info = n.records.iter().flatten().find_map(|r| match r {
                    Record::Text(t) => Some(t.clone()),
                    _ => None,
                });
                let text = info.and_then(|i| ui.assets().write(&i)).unwrap_or_default();
                let objects = [n.long(0), n.long(2)];
                self.make_confirmation_dialog(text, objects, n.word_at(4), out);
            }
            notice::PORTAL_SCENE => self.portal_scene(ui, n.word() != 0, &scenes_of(n), out),
            // An offer with no name reads as no offer.
            notice::RESURRECTION_REQUESTED_BY_OTHER => {
                let by = n
                    .records
                    .iter()
                    .flatten()
                    .find_map(|r| match r {
                        Record::Text(t) => ui.assets().write(t),
                        _ => None,
                    })
                    .filter(|s| !s.is_empty());
                match by {
                    Some(by) => self.resurrection_requested_by_other(ui, &by, out),
                    None => self.make_resurrect_dialog(ui, None, out),
                }
            }
            crate::story::notice::FINISH_PLAY_SEQUENCE => {
                self.finish_play_sequence(ui, &scenes_of(n), out);
            }
            answer::CLOSE_CONFIRMATION_DIALOG => {
                if let Some((id, yes)) = crate::dialog::confirmation_closed(n) {
                    self.close_confirmation_dialog(ui, id, yes, out);
                }
            }
            answer::CLOSE_RESURRECT_DIALOG => self.close_resurrect_dialog(n.word(), out),
            answer::CLOSE_MESSAGE_DIALOG => self.close_message_dialog(n.word()),
            answer::UPDATE_CAN_MOUSE_LOOK => {
                if let Some(c) = &mut self.camera {
                    let mut cam = Vec::new();
                    c.update_can_mouse_look(ui, n.word() != 0, &mut cam);
                    out.extend(cam.into_iter().map(Request::Camera));
                }
            }
            _ => {}
        }
    }

    /// The character's session ends (the server said so): at the next tick the play screen ends
    /// it as the log-out key does.
    // D2-01902
    pub fn end_character_session(&mut self) {
        self.end_asked = true;
    }

    /// The player logged off: the options sent to keep, and the program ends.
    // D2-01903
    pub fn logoff(out: &mut Vec<Request>) {
        out.push(Request::KeepOptions);
        out.push(Request::Quit);
    }

    /// Back to the character list; the play screen notes it is going.
    // D2-01918
    pub fn update(&mut self, out: &mut Vec<Request>) {
        out.push(Request::CharacterSelection);
        self.back = true;
    }

    /// A line of the client's own errors for the chat windows.
    // D2-01901, D2-03977
    pub fn set_error_msg(text: &str, out: &mut Vec<Request>) {
        out.push(Request::ErrorLine(text.to_owned()));
    }

    fn new_dialog(&mut self, kind: u32, text: String, modal: bool, out: &mut Vec<Request>) -> u32 {
        Self::dialog_of(kind, text, None, modal, out)
    }

    fn dialog_of(
        kind: u32,
        text: String,
        info: Option<auberean_text::StringInfo>,
        modal: bool,
        out: &mut Vec<Request>,
    ) -> u32 {
        let id = crate::dialog::new_context();
        out.push(Request::Dialog(Dialog {
            id,
            kind,
            text,
            info,
            modal,
        }));
        id
    }

    fn close_dialog(&self, id: u32, out: &mut Vec<Request>) {
        if id != 0 {
            out.push(Request::DialogDown(id));
        }
    }

    /// Please wait, unless it is up.
    // D2-01906
    pub fn make_please_wait_dialog(&mut self, ui: &Ui, out: &mut Vec<Request>) {
        if self.wait == 0 {
            let text = flow::message(ui, flow::MESSAGES, "ID_Wait_PleaseWait");
            self.wait = self.new_dialog(dialog::WAIT, text, false, out);
        }
    }

    /// Death's dialog (the one up replaced): with no one offering, "You have died." with one
    /// answer; with someone offering (`by`, their name), the question whether to accept.
    // D2-01907
    pub fn make_resurrect_dialog(&mut self, ui: &Ui, by: Option<&str>, out: &mut Vec<Request>) {
        self.close_dialog(self.resurrect, out);
        self.resurrect = 0;
        let (kind, text) = match by {
            None => (
                dialog::ONE_ANSWER,
                flow::message(ui, flow::MESSAGES, "ID_Death_YouDied"),
            ),
            Some(who) => {
                let table = ui
                    .assets()
                    .data_id_by_enum(RESURRECTION_MESSAGES, flow::TABLE_MAPPING);
                let mut info = auberean_text::StringInfo::entry(
                    table,
                    Token::of("ID_ResurrectionPrompt").raw(),
                );
                info.variables.push(auberean_text::Variable {
                    id: Token::of("PLAYER").raw(),
                    value: auberean_text::Value::Text(Box::new(
                        auberean_text::StringInfo::literal(who),
                    )),
                });
                (
                    dialog::CONFIRMATION,
                    ui.assets().write(&info).unwrap_or_default(),
                )
            }
        };
        self.resurrect = self.new_dialog(kind, text, true, out);
    }

    /// A question of the player's scripts put up (the one up replaced), with what to answer it
    /// with.
    // D2-01908
    pub fn make_confirmation_dialog(
        &mut self,
        text: String,
        objects: [u64; 2],
        value: u32,
        out: &mut Vec<Request>,
    ) {
        if let Some(c) = self.confirmation.take() {
            self.close_dialog(c.id, out);
        }
        let id = self.new_dialog(dialog::CONFIRMATION, text, true, out);
        self.confirmation = Some(Confirmation { id, objects, value });
    }

    /// The player died (the play screen hidden, death's dialog up) or lives again (the dialog
    /// down, the play screen back).
    // D2-01911
    pub fn update_death_state(&mut self, ui: &mut Ui, dead: bool, out: &mut Vec<Request>) {
        if dead {
            self.show(ui, false, out);
            self.make_resurrect_dialog(ui, None, out);
            return;
        }
        self.close_dialog(self.resurrect, out);
        self.resurrect = 0;
        self.show(ui, true, out);
    }

    /// Someone offers to bring the player back.
    // D2-01912
    pub fn resurrection_requested_by_other(&mut self, ui: &Ui, by: &str, out: &mut Vec<Request>) {
        self.make_resurrect_dialog(ui, Some(by), out);
    }

    /// A question closed: death's (no: the offer turned down and death's own dialog back; yes:
    /// brought back), or the scripts' (yes: answered with what it carried).
    // D2-01913
    pub fn close_confirmation_dialog(
        &mut self,
        ui: &Ui,
        id: u32,
        yes: bool,
        out: &mut Vec<Request>,
    ) {
        if id == self.resurrect {
            if yes {
                self.resurrect = 0;
                out.push(Request::Resurrect);
            } else {
                out.push(Request::DeclineResurrection);
                self.make_resurrect_dialog(ui, None, out);
            }
        } else if let Some(c) = self.confirmation.filter(|c| c.id == id) {
            if yes {
                out.push(Request::Confirm {
                    objects: c.objects,
                    value: c.value,
                });
            }
            self.confirmation = None;
        }
    }

    /// Death's dialog answered: brought back.
    // D2-01914
    pub fn close_resurrect_dialog(&mut self, id: u32, out: &mut Vec<Request>) {
        if id == self.resurrect {
            self.resurrect = 0;
            out.push(Request::Resurrect);
        }
    }

    /// A message of the scripts in a dialog of its own (written, or a string reference its text
    /// element writes), kept until it closes; `modal`, it goes up at once in place of the dialog
    /// up, which waits behind it.
    // D2-01919
    pub fn display_message(
        &mut self,
        text: String,
        info: Option<auberean_text::StringInfo>,
        modal: bool,
        out: &mut Vec<Request>,
    ) {
        let id = Self::dialog_of(dialog::MESSAGE, text, info, modal, out);
        self.messages.push(id);
    }

    /// A message's dialog closed: forgotten (the last kept with that number).
    // D2-01927
    pub fn close_message_dialog(&mut self, id: u32) {
        if let Some(i) = self.messages.iter().rposition(|m| *m == id) {
            self.messages.remove(i);
        }
    }

    /// A story's scenes to play, unless one plays: the story made (Escape may cut it short).
    // D2-01938
    pub fn play_sequence(
        &mut self,
        ui: &mut Ui,
        scenes: &crate::story::Scenes,
        out: &mut Vec<Request>,
    ) {
        if self.story.is_none() {
            self.story = Some(crate::story::Story::new(ui, self.frame, scenes, true, out));
        }
    }

    /// The story's scenes are done: unless a scene plays on the way through a portal, the story
    /// goes and the scripts hear which scenes were played.
    // D2-01937
    pub fn finish_play_sequence(
        &mut self,
        ui: &mut Ui,
        scenes: &crate::story::Scenes,
        out: &mut Vec<Request>,
    ) {
        if !self.playing_portal_scene {
            self.close_story(ui, out);
            out.push(Request::PlayScenesDone(scenes.packed.clone()));
        }
    }

    /// A scene on the way through a portal: one plays (from now until the player arrives) or not;
    /// then the scenes checked.
    // D2-01945
    pub fn portal_scene(
        &mut self,
        ui: &mut Ui,
        playing: bool,
        scenes: &crate::story::Scenes,
        out: &mut Vec<Request>,
    ) {
        if playing {
            self.playing_portal_scene = true;
            out.push(Request::PortalScenePlaying(true));
        }
        self.check_portal_scene(ui, scenes, out);
    }

    /// The portal's scenes: while none plays, the story goes and the scripts hear which were
    /// played; while one plays, a story is made for it unless one plays (Escape does not cut it
    /// short).
    // D2-01939
    pub fn check_portal_scene(
        &mut self,
        ui: &mut Ui,
        scenes: &crate::story::Scenes,
        out: &mut Vec<Request>,
    ) {
        if !self.playing_portal_scene {
            self.close_story(ui, out);
            out.push(Request::PlayScenesDone(scenes.packed.clone()));
        } else if self.story.is_none() {
            self.story = Some(crate::story::Story::new(ui, self.frame, scenes, false, out));
        }
    }

    /// The player arrived in the world: a scene that played on the way through a portal is over,
    /// and the scenes checked with none.
    // D2-01944
    pub fn player_ready_to_enter_world(&mut self, ui: &mut Ui, out: &mut Vec<Request>) {
        if self.playing_portal_scene {
            self.playing_portal_scene = false;
            out.push(Request::PortalScenePlaying(false));
            self.check_portal_scene(ui, &crate::story::Scenes::default(), out);
        }
    }

    /// The story goes, if one plays.
    fn close_story(&mut self, ui: &mut Ui, out: &mut Vec<Request>) {
        if let Some(s) = self.story.take() {
            s.close(ui, out);
        }
    }

    /// The panels go: each one's elements and listening.
    // D2-01928
    pub fn cleanup_children(&mut self, ui: &mut Ui) {
        for c in self.children.drain(..) {
            self.registry.unregister_all(Handler::Panel(c.id));
            c.child.close(ui);
        }
    }

    /// The session ends (once): the story and death's and the scripts' dialogs down, the options
    /// sent to keep; with `wait`, please wait and the panels gone; the character logged off.
    // D2-01936
    pub fn end_session(&mut self, ui: &mut Ui, wait: bool, out: &mut Vec<Request>) {
        if self.ended {
            return;
        }
        self.close_story(ui, out);
        if self.resurrect != 0 {
            self.close_dialog(self.resurrect, out);
            self.resurrect = 0;
            self.show(ui, true, out);
        }
        if let Some(c) = self.confirmation.take() {
            self.close_dialog(c.id, out);
            self.show(ui, true, out);
        }
        out.push(Request::KeepOptions);
        if wait {
            self.make_please_wait_dialog(ui, out);
            self.cleanup_children(ui);
        }
        out.push(Request::LogOff);
        self.ended = true;
    }

    /// The panels' places changed: kept with the player's options and sent to the server to
    /// keep.
    // D2-01962, D2-00323
    pub fn save_locations_changed(out: &mut Vec<Request>) {
        out.push(Request::SaveLocations);
    }

    /// The scripts' command to write or read the panels' places: its file's name goes with it.
    // D2-01946, D2-01947
    fn layout_notice(ui: &Ui, n: &Notice, out: &mut Vec<Request>) {
        let name = n
            .records
            .iter()
            .flatten()
            .find_map(|r| match r {
                Record::Text(t) => ui.assets().write(t),
                _ => None,
            })
            .unwrap_or_default();
        out.push(if n.native == notice::SAVE_UI {
            Request::SaveLayout { name }
        } else {
            Request::LoadLayout { name }
        });
    }

    /// The player's options came again: the panels' places put back from them.
    // D2-01964
    pub fn reload_options(&mut self, out: &mut Vec<Request>) {
        out.push(Request::PutBackLocations { reloaded: true });
    }

    /// The play screen goes: the session ended if it had not, the interface keys no longer heard,
    /// the message field, the dialogs, the panels and the camera gone; the display back to the
    /// screens' before the world (unless the program is ending).
    // D2-01942, D2-11907, D2-11908
    pub fn close(mut self, ui: &mut Ui, shutting_down: bool, out: &mut Vec<Request>) {
        if !self.ended {
            self.end_session(ui, false, out);
        }
        if let Some(j) = self.jukebox.take() {
            ui.frame_remove_root(self.frame, j);
            ui.remove(j);
        }
        self.close_dialog(self.wait, out);
        for m in std::mem::take(&mut self.messages) {
            self.close_dialog(m, out);
        }
        self.close_story(ui, out);
        self.cleanup_children(ui);
        if let Some(c) = self.camera.take() {
            let mut cam = Vec::new();
            c.close(ui, &mut cam);
            out.extend(cam.into_iter().map(Request::Camera));
        }
        if !shutting_down {
            out.push(Request::DisplayInWorld(false));
        }
        ui.drop_frame(self.frame);
    }
}

/// The scenes a notice names (none when it names no record of them).
fn scenes_of(n: &Notice) -> crate::story::Scenes {
    n.records
        .iter()
        .flatten()
        .find_map(|r| match r {
            Record::Scenes(s) => Some(s.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

/// The string table of the resurrection's question, by enumeration value.
pub const RESURRECTION_MESSAGES: u32 = 0x4100_000A;

/// The panels' places as a file the player writes and reads back: a line per panel, its tag, its
/// place and size on the screen.
pub mod layout_file {
    use auberean_ui::{Token, Ui};

    /// The panels kept, by tag and element name, in the file's order.
    pub const PANELS: [(&str, &str); 9] = [
        ("<SOCIA>", "SocialField"),
        ("<QUEST>", "QuestField"),
        ("<SKILL>", "SkillField"),
        ("<NAVIG>", "NavigationField"),
        ("<EXAMI>", "ExaminationField"),
        ("<CHAT1>", "ChatWindow1"),
        ("<CHAT2>", "ChatWindow2"),
        ("<CHAT3>", "ChatWindow3"),
        ("<CHAT4>", "ChatWindow4"),
    ];

    /// The file's name: the name the player gave, or with none the character's, the world's and
    /// the screen's width and height.
    // D2-01929
    #[must_use]
    pub fn file_name(name: &str, character: &str, world: &str, width: i32, height: i32) -> String {
        if name.is_empty() {
            format!("UI-{character}-{world}-{width}-{height}.txt")
        } else {
            format!("{name}.txt")
        }
    }

    /// The file written: each panel's line, its place on the screen and its size.
    // D2-01941
    #[must_use]
    pub fn write(ui: &Ui) -> String {
        let mut s = String::new();
        for (tag, el) in PANELS {
            let r = ui
                .element(Token::of(el))
                .and_then(|h| ui.screen_rect(h))
                .unwrap_or_default();
            s.push_str(&format!(
                "{tag} X:{} Y: {} W: {} H: {}\n",
                r.x0,
                r.y0,
                r.width(),
                r.height()
            ));
        }
        s
    }

    /// The file read back: each line naming a kept panel gives it its size, then its place.
    // D2-01940
    pub fn read(ui: &mut Ui, text: &str) {
        for line in text.lines() {
            let Some((tag, x, y, w, h)) = parse(line) else {
                continue;
            };
            let Some((_, el)) = PANELS.iter().find(|(t, _)| *t == tag) else {
                continue;
            };
            if let Some(e) = ui.element(Token::of(el)) {
                ui.resize_to(e, w, h);
                ui.move_to(e, x, y);
            }
        }
    }

    /// `<TAG> X:%d Y: %d W: %d H: %d`, read as the client reads it (five fields or none).
    fn parse(line: &str) -> Option<(&str, i32, i32, i32, i32)> {
        let mut it = line.split_whitespace();
        let tag = it.next()?;
        let mut num = |label: &str| -> Option<i32> {
            let t = it.next()?;
            match t.strip_prefix(label) {
                Some("") => it.next()?.parse().ok(),
                Some(rest) => rest.parse().ok(),
                None => None,
            }
        };
        let x = num("X:")?;
        let y = num("Y:")?;
        let w = num("W:")?;
        let h = num("H:")?;
        Some((tag, x, y, w, h))
    }
}
