//! The screens before the world, as the game's flow holds them: what lives across them (the
//! account's characters as the server last listed them, the row the list comes back on, the
//! avatar character creation is making), what they ask of the client, what the client tells them,
//! and the shape of a screen.
//!
//! Each screen is a main framework of its own made when the flow brings its mode up and gone when
//! the flow brings up another: the opening pictures ([`crate::intro`]), the connection and its
//! data ([`crate::datapatch`]), the reason a session ended ([`crate::disconnected`]), the
//! character list ([`crate::character_select`]), creation's hall, appearance and name
//! ([`crate::creation`]), the key map ([`crate::keyboard`]) and the credits ([`crate::credits`]).
//! A screen hears its elements through its framework's listener, as the play screen's panels do,
//! and says what it wants as [`Request`]s; the flow's next screen is one of them, asked for and
//! brought up at the next tick of the clock.

use std::any::Any;

use auberean_text::StringInfo;
use auberean_ui::framework::FrameId;
use auberean_ui::{Mail, Token, Ui};

use crate::{dialog, flow, gameplay};

/// One character of the account, as the server listed it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Character {
    pub id: u64,
    pub name: String,
    /// Seconds until a deleted character is purged (0: not deleted).
    pub greyed: u32,
}

/// The id the server gives no character.
pub const INVALID_ID: u64 = 0;

/// The account's characters as the server last listed them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CharacterSet {
    pub characters: Vec<Character>,
    /// How many characters the account may have (below 0: the server said nothing).
    pub slots: i32,
    /// The account's name.
    pub account: String,
}

impl CharacterSet {
    /// A row's character; none for a row that holds the invalid id.
    // D2-13097, D2-13098, D2-13099
    #[must_use]
    pub fn identity(&self, row: usize) -> Option<&Character> {
        self.characters.get(row).filter(|c| c.id != INVALID_ID)
    }

    /// The seconds until a row's character is purged: 0 when it is not deleted, for no such
    /// character, and for a row past the number the account may have (when it may have any).
    // D2-13101
    #[must_use]
    pub fn greyed_out_for(&self, row: usize) -> u32 {
        if self.slots >= 1 && i32::try_from(row).map_or(true, |r| r >= self.slots) {
            return 0;
        }
        self.identity(row).map_or(0, |c| c.greyed)
    }

    /// The row a character stands at.
    #[must_use]
    pub fn which_slot(&self, id: u64) -> Option<usize> {
        self.characters.iter().position(|c| c.id == id)
    }
}

/// A race and sex: an avatar of character creation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Avatar {
    pub race: u32,
    pub sex: u32,
}

/// What lives across the screens before the world.
#[derive(Clone, Debug, Default)]
pub struct Data {
    /// The account's characters, once the server listed them, and how many lists have come.
    pub characters: Option<CharacterSet>,
    pub lists: u64,
    /// The row the list selects when it next comes (a character just made); taken then.
    pub selected: usize,
    /// Character creation is under way (its avatars made), and the avatar chosen.
    pub in_creation: bool,
    pub avatar: Option<Avatar>,
    /// The race flags the account may make beyond the first three (the server's), and the
    /// avatars of the hall in the order its figures stand.
    pub may_make: u32,
    pub hall: Vec<Avatar>,
    /// The chosen avatar's kinds of appearance choice (the kinds its race has), and how many
    /// times they changed.
    pub choices: Vec<crate::creation::Choice>,
    pub choices_version: u64,
    /// The character to enter the world with as soon as the account's characters are listed
    /// (empty: none), as the command line or a developer's login names one.
    pub start_character: String,
    /// The character the command line named to make and play, read when it named none to play.
    pub create_character: String,
    /// The creation screen offers the administrator's character.
    pub admin_client: bool,
    /// The answer to the last selection in a screen's scene: the figure the pointer was over, or
    /// none; `None` while it is asked or when none was.
    pub selection: Option<Option<usize>>,
    /// The figure of the hall under the pointer.
    pub under: Option<usize>,
    /// The figures of the hall still playing a behavior they were asked to.
    pub behaving: Vec<usize>,
    /// The pointer, in the screen's pixels.
    pub pointer: (i32, i32),
    /// Whether the screens' sound is playing (`None`: the client plays no sound).
    pub sound_playing: Option<bool>,
    /// The client's key maps and the actions they bind, for the key map screen; the keys pressed
    /// since the last tick (each input word and the modifier held with it).
    pub keys: Option<crate::keyboard::KeyData>,
    pub key_hits: Vec<(u32, u32)>,
}

/// The connection's progress as the data screen hears it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Connection {
    /// Asking the server, the step so far of how many.
    Connecting { so_far: u32, total: u32 },
    /// Waiting for the server's answer.
    Waiting,
    /// Connected.
    Connected,
    /// The connection to the server at this host and port failed, with an error.
    Failed {
        host: String,
        port: u16,
        error: ConnectError,
    },
}

/// Why a connection failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConnectError {
    /// The server's address could not be understood.
    BadAddress,
    /// Any other error, by its identifier (`ID_ConnectionError_Generic` when the server could not
    /// be reached; empty for one with no name).
    Named(String),
}

/// What the client tells the screen up.
#[derive(Clone, Debug, PartialEq)]
pub enum Notice {
    /// A question closed: its number, and whether the answer was yes.
    ConfirmationClosed { id: u32, yes: bool },
    /// The server's answer about a character made or restored: its response (1: done), the
    /// character, and the error the server's game gave (0: none).
    CharGenVerification {
        response: u32,
        character: Option<Character>,
        error: u32,
    },
    /// The connection's progress.
    Net(Connection),
    /// The connection's rate of data received (bytes a second) and whether any came.
    Heartbeat { rate: f64, received: bool },
    /// The data download's progress: an event and its value.
    Ddd { event: u32, value: i32 },
}

/// What a screen before the world asks of the client.
#[derive(Clone, Debug, PartialEq)]
pub enum Request {
    /// Bring up the screen of a mode at the next tick, with a text for it.
    Mode(Token, String),
    /// A dialog up under a screen's framework, or down.
    Dialog {
        dialog: gameplay::Dialog,
        parent: FrameId,
    },
    DialogDown(u32),
    /// Connect to the server with the account the client was given.
    Connect,
    /// Enter the world as a character.
    Play {
        character: u64,
    },
    /// Delete a character, or restore a deleted one.
    Delete {
        character: u64,
    },
    Restore {
        character: u64,
        name: String,
    },
    /// Make a character of the avatar chosen with this name, an administrator's when asked.
    Create {
        name: String,
        admin: bool,
    },
    /// Find the figure under the pointer in the screen's scene: the answer comes in
    /// [`Data::selection`].
    FindUnderPointer,
    /// The hall's avatar chosen.
    Choose(Avatar),
    /// A hall figure plays a behavior (it stays in [`Data::behaving`] until done).
    Behave {
        figure: usize,
        behavior: u32,
    },
    /// The avatar's appearance: every type of a mask at random, or a type at one of its stops.
    Randomize {
        mask: u32,
    },
    Choice {
        physique: u32,
        stop: usize,
    },
    /// The appearance's view: turned to a heading (degrees), framing the head or the body.
    Turn {
        heading: f32,
    },
    Zoom(bool),
    /// The key maps applied: the client keeps them.
    Keys(crate::keyboard::KeyMaps),
    /// A sound across the screens (a sound table's record), or the one playing stopped.
    Sound(u32),
    Silence,
    /// Leave the game.
    Quit,
}

/// One screen before the world, as the flow holds it.
pub trait Screen: Any + std::fmt::Debug {
    /// Its framework.
    fn frame(&self) -> FrameId;
    /// Show or hide it.
    fn show(&mut self, ui: &mut Ui, on: bool) {
        ui.frame_show(self.frame(), on);
    }
    /// The flow's tick while it is up: what its listener heard.
    fn tick(&mut self, ui: &mut Ui, data: &mut Data, out: &mut Vec<Request>);
    /// What lives across screens changed: the account's characters came again.
    fn update(&mut self, _ui: &mut Ui, _data: &mut Data, _out: &mut Vec<Request>) {}
    /// What the client tells it.
    fn notice(&mut self, _ui: &mut Ui, _data: &mut Data, _n: &Notice, _out: &mut Vec<Request>) {}
    /// The text the flow asked for it with: an error to show.
    fn set_error_msg(&mut self, _ui: &mut Ui, _text: &str) {}
    /// It goes: its elements, its listening, its dialogs.
    fn close(self: Box<Self>, ui: &mut Ui, data: &mut Data, out: &mut Vec<Request>);
    /// Itself, to be reached by what holds it.
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

/// What a screen's listener heard since it last looked.
#[must_use]
pub fn take(ui: &mut Ui, frame: FrameId) -> Vec<Mail> {
    ui.frame_listener(frame)
        .map(|l| ui.take_mail(l))
        .unwrap_or_default()
}

/// Have a screen's listener hear what elements named `element` say (`what`).
pub fn listen(ui: &mut Ui, frame: FrameId, element: Token, what: Token) {
    if let Some(l) = ui.frame_listener(frame) {
        ui.listen_for_message(l, element, what);
    }
}

/// Have a screen's listener hear one of the manager's global messages.
pub fn listen_global(ui: &mut Ui, frame: FrameId, what: u32) {
    if let Some(l) = ui.frame_listener(frame) {
        ui.listen_for_global_message(l, what);
    }
}

/// A string of the client's own messages (by its name), written with its variables.
#[must_use]
pub fn message(ui: &Ui, name: &str, vars: &[(&str, auberean_text::Value)]) -> String {
    message_by_token(ui, Token::of(name).raw(), vars)
}

/// A string of the client's own messages by its token, written with its variables.
#[must_use]
pub fn message_by_token(ui: &Ui, token: u32, vars: &[(&str, auberean_text::Value)]) -> String {
    let table = ui
        .assets()
        .data_id_by_enum(flow::MESSAGES, flow::TABLE_MAPPING);
    let mut info = StringInfo::entry(table, token);
    for (n, v) in vars {
        info.add_var(Token::of(n).raw(), v.clone());
    }
    ui.assets().write(&info).unwrap_or_default()
}

/// Please wait, under a screen's framework: its number.
#[must_use]
pub fn please_wait(ui: &Ui, frame: FrameId, out: &mut Vec<Request>) -> u32 {
    let id = dialog::new_context();
    out.push(Request::Dialog {
        dialog: gameplay::Dialog {
            id,
            kind: dialog::kind::WAIT,
            text: message(ui, "ID_Wait_PleaseWait", &[]),
            modal: false,
            info: None,
        },
        parent: frame,
    });
    id
}

/// A question under a screen's framework: its number.
#[must_use]
pub fn question(text: String, frame: FrameId, out: &mut Vec<Request>) -> u32 {
    let id = dialog::new_context();
    out.push(Request::Dialog {
        dialog: gameplay::Dialog {
            id,
            kind: dialog::kind::CONFIRMATION,
            text,
            modal: false,
            info: None,
        },
        parent: frame,
    });
    id
}

/// A dialog down (none for 0).
pub fn dialog_down(id: u32, out: &mut Vec<Request>) {
    if id != 0 {
        out.push(Request::DialogDown(id));
    }
}

/// Ask the flow for the screen of a mode, with no text.
pub fn queue(mode: Token, out: &mut Vec<Request>) {
    out.push(Request::Mode(mode, String::new()));
}
