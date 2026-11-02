//! The game's flow between its screens: which screen each mode is, the screen the game starts
//! on, the default cursor, the screenshot key, and the screens the client's troubles lead to
//! (a character refused or logged off, the server gone, the chat service lost).
//!
//! It is the element framework's [`Flow`] with the game's nine screens registered: a screen
//! asked for comes up at the next tick of the clock. What makes each screen is the caller's (the
//! client's own screens, with what they need from the game); the modes and their order are the
//! game's.

use auberean_ui::framework::flow::{Factory, Flow};
use auberean_ui::manager::global;
use auberean_ui::{Mail, Token, Ui};

/// The modes the screens are known by.
pub mod mode {
    use auberean_ui::Token;
    /// The key map's screen.
    pub const KEYBOARD: Token = Token::of("keyboard");
    pub const CREDITS: Token = Token::of("credits");
    /// The opening movies; the game starts on it.
    pub const INTRO: Token = Token::of("intro");
    /// The screen that says why the session ended.
    pub const DISCONNECTED: Token = Token::of("disconnected");
    /// Downloading data.
    pub const DATA_PATCH: Token = Token::of("datapatch");
    pub const CHARACTER_SELECTION: Token = Token::of("characterselection");
    /// Character creation's avatar hall, and its appearance.
    pub const AVATAR_SELECTION: Token = Token::of("avatarselection");
    pub const AVATAR_MODIFICATION: Token = Token::of("avatarmodification");
    /// The play screen.
    pub const GAMEPLAY: Token = Token::of("play");
}

/// The modes, in the order the game registers them.
pub const MODES: [Token; 9] = [
    mode::KEYBOARD,
    mode::CREDITS,
    mode::INTRO,
    mode::DISCONNECTED,
    mode::DATA_PATCH,
    mode::CHARACTER_SELECTION,
    mode::AVATAR_SELECTION,
    mode::AVATAR_MODIFICATION,
    mode::GAMEPLAY,
];

/// The interface action that takes a screenshot (when it is let go).
pub const SCREENSHOT_KEY: u32 = 0x4100_0017;

/// The enumeration value of the string table the flow's messages come from, and the chat
/// service's.
pub const MESSAGES: u32 = 0x4100_0018;
pub const CHAT_SERVICE_MESSAGES: u32 = 0x4100_000E;

/// The mapping string tables are named in.
pub const TABLE_MAPPING: u32 = 0x4000_0004;

/// The default cursor: the enumeration value and the mapping it is named in.
pub const DEFAULT_CURSOR: (u32, u32) = (0x4100_0002, 0x4000_0003);

/// What the flow heard that its owner carries out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// Save a screenshot.
    Screenshot,
}

/// Why the server ended a character's session or refused one (the reason it sends), and the
/// screen and message each leads to: the screens before the world for a character that could not
/// enter, the disconnected screen for the account's troubles.
#[must_use]
pub const fn character_error(reason: u32) -> Option<(Token, &'static str)> {
    let back = mode::CHARACTER_SELECTION;
    let gone = mode::DISCONNECTED;
    Some(match reason {
        2 => (gone, "ID_CHAR_ERROR_ACCOUNT_LOGON"),
        3 => (gone, "ID_CHAR_ERROR_SERVER_CRASH"),
        4 => (gone, "ID_CHAR_ERROR_LOGOFF"),
        5 => (back, "ID_CHAR_ERROR_DELETE"),
        6 => (gone, "ID_CHAR_ERROR_ACCOUNT_IN_USE"),
        7 => (gone, "ID_CHAR_ERROR_ACCOUNT_INVALID"),
        8 => (gone, "ID_CHAR_ERROR_ACCOUNT_DOESNT_EXIST"),
        9 => (back, "ID_CHAR_ERROR_ENTER_GAME_GENERIC"),
        10 => (back, "ID_CHAR_ERROR_ENTER_GAME_STRESS_ACCOUNT"),
        11 => (back, "ID_CHAR_ERROR_ENTER_GAME_CHARACTER_IN_WORLD"),
        12 => (back, "ID_CHAR_ERROR_ENTER_GAME_PLAYER_ACCOUNT_MISSING"),
        13 => (back, "ID_CHAR_ERROR_ENTER_GAME_CHARACTER_NOT_OWNED"),
        14 => (back, "ID_CHAR_ERROR_ENTER_GAME_CHARACTER_IN_WORLD_SERVER"),
        15 => (back, "ID_CHAR_ERROR_ENTER_GAME_OLD_CHARACTER"),
        16 => (back, "ID_CHAR_ERROR_ENTER_GAME_CORRUPT_CHARACTER"),
        17 => (back, "ID_CHAR_ERROR_ENTER_GAME_START_SERVER_DOWN"),
        18 => (back, "ID_CHAR_ERROR_ENTER_GAME_COULDNT_PLACE_CHARACTER"),
        19 => (gone, "ID_CHAR_ERROR_LOGON_SERVER_FULL"),
        20 => (gone, "ID_CHAR_ERROR_CHARACTER_IS_BOOTED"),
        21 => (back, "ID_CHAR_ERROR_ENTER_GAME_LOCKED"),
        _ => return None,
    })
}

/// A message of a string table named by enumeration value, written as the client writes it.
#[must_use]
pub fn message(ui: &Ui, table_enum: u32, name: &str) -> String {
    let table = ui.assets().data_id_by_enum(table_enum, TABLE_MAPPING);
    ui.assets()
        .text(table, Token::of(name).raw())
        .unwrap_or_default()
}

/// The game's flow: the element framework's flow over what lives across screens (`P`).
pub struct GameFlow<P> {
    pub flow: Flow<P>,
}

impl<P> std::fmt::Debug for GameFlow<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.flow.fmt(f)
    }
}

impl<P> GameFlow<P> {
    /// The game's screens registered, each mode by what `make` gives for it (a mode it gives
    /// nothing for has no screen), the opening screen asked for, the default cursor set, and the
    /// flow listening for keys let go (the screenshot key).
    // D2-01713
    pub fn new(ui: &mut Ui, data: P, make: impl Fn(Token) -> Option<Factory<P>>) -> Self {
        let mut flow = Flow::new(ui, data);
        for m in MODES {
            if let Some(f) = make(m) {
                flow.register(m, f);
            }
        }
        flow.queue_mode(mode::INTRO, "");
        set_default_cursor(ui);
        ui.listen_for_global_message(flow.listener(), global::KEY_UP);
        Self { flow }
    }

    /// The flow's tick (a screen asked for comes up when the clock ticks), and what else it heard:
    /// the screenshot key let go.
    // D2-01706
    pub fn tick(&mut self, ui: &mut Ui) -> Vec<Event> {
        let mut out = Vec::new();
        for m in self.flow.tick(ui) {
            if let Mail::Global {
                what: global::KEY_UP,
                param: SCREENSHOT_KEY,
            } = m
            {
                out.push(Event::Screenshot);
            }
        }
        out
    }

    /// Ask for a screen, with a text for it.
    pub fn queue_mode(&mut self, mode: Token, argument: &str) {
        self.flow.queue_mode(mode, argument);
    }

    /// The server refused a character or ended its session for `reason`: the screen the reason
    /// leads to, asked for with the server's own words when it gave some that are valid (a boot
    /// gives its line), else the reason's message. A reason the game does not know changes
    /// nothing.
    // D2-01708, D2-01711
    pub fn character_error(
        &mut self,
        ui: &Ui,
        reason: u32,
        given: Option<&auberean_text::StringInfo>,
    ) {
        let Some((mode, name)) = character_error(reason) else {
            return;
        };
        let text = given
            .and_then(|g| ui.assets().write_valid(g))
            .unwrap_or_else(|| message(ui, MESSAGES, name));
        self.flow.queue_mode(mode, &text);
    }

    /// The connection to the server was lost: the disconnected screen, saying so, unless it is up
    /// or asked for already.
    // D2-01709, D2-01712
    pub fn server_died(&mut self, ui: &Ui) {
        let d = mode::DISCONNECTED;
        if self.flow.mode() == d || self.flow.queued_mode() == d {
            return;
        }
        let text = message(ui, MESSAGES, "ID_NetErr_ConnectionLost");
        self.flow.queue_mode(d, &text);
    }

    /// The client stopped, saying `line` (the line it shows when it cannot go on): the
    /// disconnected screen, saying it, whatever screen was up.
    // D2-09653
    pub fn stopped(&mut self, line: &str) {
        self.flow.queue_mode(mode::DISCONNECTED, line);
    }

    /// The chat service's connection was lost: the screen up is told why (it was never up while
    /// data downloads), as the error it shows.
    // D2-01710
    pub fn chat_service_lost(&mut self, ui: &mut Ui) {
        let name = if self.flow.mode() == mode::DATA_PATCH {
            "ID_CHAR_ERROR_GUN_SERVER_DOWN"
        } else {
            "ID_CHAR_ERROR_GUN_SERVER_LOST"
        };
        let text = message(ui, CHAT_SERVICE_MESSAGES, name);
        let (current, data) = self.flow.current_mut();
        if let Some(s) = current {
            s.argument(ui, data, &text);
        }
    }

    /// The chat service took the login's ticket again after its connection came back (or did not):
    /// the screen up is told so, as for a connection lost.
    pub fn chat_service_reauthenticated(&mut self, ui: &mut Ui, taken: bool) {
        let name = if taken {
            "ID_GunReAuthenticationSuccess"
        } else {
            "ID_GunReAuthenticationFailed"
        };
        let text = message(ui, CHAT_SERVICE_MESSAGES, name);
        let (current, data) = self.flow.current_mut();
        if let Some(s) = current {
            s.argument(ui, data, &text);
        }
    }

    /// The chat service refused the login's ticket, so the login ends: the disconnected screen,
    /// saying why (the account banned, or the credentials not had).
    pub fn chat_service_refused(&mut self, ui: &Ui, banned: bool) {
        let name = if banned {
            "ID_DataPatch_GUNAccountBanned"
        } else {
            "ID_DataPatch_GUNFailed"
        };
        let text = message(ui, MESSAGES, name);
        self.flow.queue_mode(mode::DISCONNECTED, &text);
    }

    /// The flow goes: the screen up goes, and the flow stops listening.
    // D2-01704, D2-01707
    pub fn close(self, ui: &mut Ui) {
        self.flow.close(ui);
    }
}

/// The interface's default cursor, from the data.
// D2-01713
pub fn set_default_cursor(ui: &mut Ui) {
    let picture = ui
        .assets()
        .data_id_by_enum(DEFAULT_CURSOR.0, DEFAULT_CURSOR.1);
    if picture != 0 {
        ui.set_cursor(
            auberean_ui::Cursor {
                picture,
                hot_x: 0,
                hot_y: 0,
            },
            true,
        );
    }
}
