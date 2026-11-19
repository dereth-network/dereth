//! The game before the world as Horizon draws it: where the session is, how the login and the data
//! download go, the account's characters, the server's answers about a character made and its
//! refusals of an entry, and why the session ended. Followed once a frame from the game's view and
//! the shell's word on how the last session ended.

use auberean_client_sdk::contract::session::{LoginProgress, NetError, Phase, SessionEnd};
use auberean_client_sdk::contract::text::TextRef;
use auberean_client_sdk::contract::view::View;
use auberean_world::chargen;

/// The data download's events, by number.
pub mod download {
    /// Looking for data to download.
    pub const LOOKING: u32 = 0;
    /// Waiting in line (the value: the place).
    pub const WAITING: u32 = 1;
    /// Downloading (the value: how many bytes).
    pub const FETCHING: u32 = 2;
    /// Done.
    pub const DONE: u32 = 3;
    /// More bytes came (the value: how many).
    pub const PROGRESS: u32 = 4;
}

/// The game's messages before the world, by enumeration value: the reasons an entry is refused.
pub const MESSAGES: u32 = 0x4100_0018;

/// One of the account's characters.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Character {
    pub id: u64,
    pub name: String,
    /// Seconds until a deleted character is gone, as the list last gave them; 0 for one in play.
    pub deleting: u32,
}

/// How the data download stands.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Download {
    /// Nothing heard of it yet.
    #[default]
    None,
    /// Looking for data to download.
    Looking,
    /// Waiting in line, at this place.
    Waiting(u32),
    /// Downloading: the bytes to come and those come.
    Fetching { expected: u32, received: u32 },
    /// Done.
    Done,
}

impl Download {
    /// How far it has come, 0 to 1, while it downloads.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn share(self) -> Option<f32> {
        match self {
            Self::Fetching { expected, received } if expected > 0 => {
                Some((received as f32 / expected as f32).clamp(0.0, 1.0))
            }
            Self::Fetching { .. } | Self::Done => Some(1.0),
            Self::None | Self::Looking | Self::Waiting(_) => None,
        }
    }
}

/// The server refused an entry into the world, or ended the session in it: its reason, the line
/// that says why, and whether the player is back at the character list (else the session is
/// over).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refused {
    pub reason: u32,
    pub line: String,
    pub to_list: bool,
}

/// The server's answer to a character made: made (the character), or why not.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Made {
    Made(u64),
    Refused(String),
}

/// What a frame before the world brings.
#[derive(Clone, Copy)]
pub struct PreFrame<'a> {
    pub phase: Phase,
    pub view: Option<&'a View>,
    /// How the last session ended under the player, when the server ended it.
    pub ended: Option<SessionEnd>,
    /// The client stopped the last session: a record it was sent could not be kept.
    pub stopped: bool,
    /// Seconds since the program started.
    pub now: f64,
    /// Portal space is drawn.
    pub portal_space: bool,
    /// A line the server gave by reference, written out.
    pub write: &'a dyn Fn(&TextRef) -> Option<String>,
    /// One of the game's messages before the world, by its name.
    pub message: &'a dyn Fn(&str) -> Option<String>,
}

impl std::fmt::Debug for PreFrame<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreFrame")
            .field("phase", &self.phase)
            .finish_non_exhaustive()
    }
}

/// The game before the world.
#[derive(Clone, Debug, PartialEq)]
pub struct Pregame {
    pub phase: Phase,
    /// The account's characters, once the server listed them.
    pub characters: Option<Vec<Character>>,
    /// How many characters the account may have, and its name.
    pub slots: u32,
    pub account: String,
    /// The world's name, as the login named it (empty before it does).
    pub world: String,
    /// The races the account may make beyond the first three ([`chargen::EMPYREAN`],
    /// [`chargen::DRUDGE`]).
    pub may_make: u32,
    /// When the list last changed, for the deleted characters' countdowns.
    pub listed_at: f64,
    /// How the login stands, while connecting.
    pub login: Option<LoginProgress>,
    pub download: Download,
    /// The session was connected (it reached the server) since it last ended.
    pub connected: bool,
    /// Why the session ended, once it has: its line.
    pub ended: Option<String>,
    /// The server's answer to a character made, this frame.
    pub made: Option<Made>,
    /// The server's refusal of an entry, this frame.
    pub refused: Option<Refused>,
    /// Portal space is drawn: entering the world, it shows from the start.
    pub portal_space: bool,
    answers: u32,
    refusals: u32,
    downloads: usize,
}

impl Default for Pregame {
    fn default() -> Self {
        Self {
            phase: Phase::Disconnected,
            characters: None,
            slots: 0,
            account: String::new(),
            world: String::new(),
            may_make: 0,
            listed_at: 0.0,
            login: None,
            download: Download::None,
            connected: false,
            ended: None,
            made: None,
            refused: None,
            portal_space: false,
            answers: 0,
            refusals: 0,
            downloads: 0,
        }
    }
}

/// Whether an entry refused for `reason` leaves the player at the character list (else the
/// session is over), and the name of the game's message that says why.
#[must_use]
pub const fn refusal(reason: u32) -> Option<(bool, &'static str)> {
    Some(match reason {
        2 => (false, "ID_CHAR_ERROR_ACCOUNT_LOGON"),
        3 => (false, "ID_CHAR_ERROR_SERVER_CRASH"),
        4 => (false, "ID_CHAR_ERROR_LOGOFF"),
        5 => (true, "ID_CHAR_ERROR_DELETE"),
        6 => (false, "ID_CHAR_ERROR_ACCOUNT_IN_USE"),
        7 => (false, "ID_CHAR_ERROR_ACCOUNT_INVALID"),
        8 => (false, "ID_CHAR_ERROR_ACCOUNT_DOESNT_EXIST"),
        9 => (true, "ID_CHAR_ERROR_ENTER_GAME_GENERIC"),
        10 => (true, "ID_CHAR_ERROR_ENTER_GAME_STRESS_ACCOUNT"),
        11 => (true, "ID_CHAR_ERROR_ENTER_GAME_CHARACTER_IN_WORLD"),
        12 => (true, "ID_CHAR_ERROR_ENTER_GAME_PLAYER_ACCOUNT_MISSING"),
        13 => (true, "ID_CHAR_ERROR_ENTER_GAME_CHARACTER_NOT_OWNED"),
        14 => (true, "ID_CHAR_ERROR_ENTER_GAME_CHARACTER_IN_WORLD_SERVER"),
        15 => (true, "ID_CHAR_ERROR_ENTER_GAME_OLD_CHARACTER"),
        16 => (true, "ID_CHAR_ERROR_ENTER_GAME_CORRUPT_CHARACTER"),
        17 => (true, "ID_CHAR_ERROR_ENTER_GAME_START_SERVER_DOWN"),
        18 => (true, "ID_CHAR_ERROR_ENTER_GAME_COULDNT_PLACE_CHARACTER"),
        19 => (false, "ID_CHAR_ERROR_LOGON_SERVER_FULL"),
        20 => (false, "ID_CHAR_ERROR_CHARACTER_IS_BOOTED"),
        21 => (true, "ID_CHAR_ERROR_ENTER_GAME_LOCKED"),
        _ => return None,
    })
}

/// Why a character could not be made, by the server's response and the game's error.
#[must_use]
pub const fn not_made(response: u32, error: u32) -> &'static str {
    match response {
        2 => "A character is already being created.",
        3 => "That name is taken.",
        4 => "That name cannot be used.",
        5 => match error {
            0xC74A => "That name is taken.",
            200 | 0xC74B => "That name cannot be used.",
            _ => "The character could not be created.",
        },
        6 => "The server cannot create characters now.",
        7 => "Only an administrator may create that character.",
        8 => "All character slots are full.",
        _ => "The character could not be created.",
    }
}

/// Why the session ended, as `end` says it, when it says: by the connection's error where it
/// names one (a login that failed for want of an answer couldn't reach the server), else a login
/// failed as refused.
#[must_use]
pub fn end_line(end: SessionEnd) -> Option<&'static str> {
    if end.server_died {
        return Some("The server went down.");
    }
    match end.connect_failed {
        Some(Some(error)) => Some(error_line(error)),
        _ if end.login_failed => Some("The server refused the login."),
        Some(None) => Some("The server can't be reached."),
        None => None,
    }
}

/// What the connection's error says, in Horizon's words.
#[must_use]
pub fn error_line(error: NetError) -> &'static str {
    if error == NetError::BAD_SERVER_ADDRESS {
        return "The server's address can't be used.";
    }
    match error.id_string() {
        "ID_NetError_CantCrypto" | "ID_ConnectionError_BadCryptoKey" => {
            "The connection to the server couldn't be secured."
        }
        "ID_ConnectionError_NetVersionMismatch" => {
            "This client's version doesn't match the server's."
        }
        "ID_ConnectionError_ServerFull" => "The server is full.",
        // The privilege error, by the start of its id, whatever the rest's spelling.
        id if id.starts_with("ID_ConnectionError_Insufficient") => {
            "This account may not log in to this server."
        }
        "ID_ConnectionError_SecondLogon" | "ID_ConnectionError_PlayerAlreadyLoggedOn" => {
            "This account is already logged in."
        }
        "ID_ConnectionError_ServerClosedConnection" => "The server closed the connection.",
        "ID_ConnectionError_ServerTimedOutClient" => "The server timed out the connection.",
        "ID_ConnectionError_RunningSpeedhack" => {
            "The server closed the connection: the game ran too fast."
        }
        "ID_ConnectionError_ClientLogOnFailed"
        | "ID_ConnectionError_AccountAuthenticationFailed" => "The server refused the login.",
        "ID_ConnectionError_LogonServerMigrated" => "The login server has moved.",
        // No socket, no answer, no login server, a handshake abandoned, or an error unnamed.
        _ => "The server can't be reached.",
    }
}

impl Pregame {
    /// The character of this id, while listed.
    #[must_use]
    pub fn character(&self, id: u64) -> Option<&Character> {
        self.characters.as_ref()?.iter().find(|c| c.id == id)
    }

    /// Whether the account has room for another character.
    #[must_use]
    pub fn has_room(&self) -> bool {
        self.characters.as_ref().is_some_and(|c| {
            u32::try_from(c.len()).unwrap_or(u32::MAX) < self.slots || self.slots == 0
        })
    }

    /// Seconds until a deleted character is gone at `now`.
    #[must_use]
    pub fn seconds_left(&self, c: &Character, now: f64) -> f64 {
        (f64::from(c.deleting) - (now - self.listed_at)).max(0.0)
    }

    /// Follow one frame.
    pub fn follow(&mut self, f: &PreFrame<'_>) {
        let was = self.phase;
        self.phase = f.phase;
        self.portal_space = f.portal_space;
        self.made = None;
        self.refused = None;
        let pre = f.view.map(|v| &v.pregame);
        match f.phase {
            Phase::Connecting => {
                if was == Phase::Disconnected {
                    self.ended = None;
                    self.download = Download::None;
                }
                self.connected = true;
            }
            Phase::Characters | Phase::Entering | Phase::World => self.connected = true,
            Phase::Disconnected => {}
        }
        self.login = pre
            .and_then(|p| p.login)
            .filter(|_| f.phase == Phase::Connecting);
        if let Some(p) = pre.filter(|p| !p.world_name.is_empty()) {
            p.world_name.clone_into(&mut self.world);
        }
        if let Some(list) = pre.and_then(|p| p.characters.as_ref()) {
            let now: Vec<Character> = list
                .characters
                .iter()
                .map(|c| Character {
                    id: c.id,
                    name: c.name.clone(),
                    deleting: c.seconds_greyed_out,
                })
                .collect();
            if self.characters.as_ref() != Some(&now) {
                self.characters = Some(now);
                self.listed_at = f.now;
            }
            self.slots = list.slots;
            list.account.clone_into(&mut self.account);
            self.may_make = (if list.expansion { chargen::EMPYREAN } else { 0 })
                | (if list.third_flag { chargen::DRUDGE } else { 0 });
        }
        if let Some(a) = pre.and_then(|p| p.creation) {
            if a.count != self.answers {
                self.answers = a.count;
                self.made = Some(if a.response == 1 {
                    Made::Made(a.character)
                } else {
                    Made::Refused(not_made(a.response, a.error).to_owned())
                });
            }
        }
        if let Some(r) = pre.and_then(|p| p.refused.as_ref()) {
            if r.count != self.refusals {
                self.refusals = r.count;
                let (to_list, name) = refusal(r.reason).unwrap_or((true, ""));
                let line = r
                    .text
                    .as_ref()
                    .and_then(f.write)
                    .filter(|l| !l.is_empty())
                    .or_else(|| (f.message)(name).filter(|l| !l.is_empty()))
                    .unwrap_or_else(|| format!("The server refused the entry ({}).", r.reason));
                if !to_list {
                    self.ended = Some(line.clone());
                }
                self.refused = Some(Refused {
                    reason: r.reason,
                    line,
                    to_list,
                });
            }
        }
        // The data download's events, each once (a new session's from its first).
        let events = pre.map_or(&[][..], |p| p.download.as_slice());
        if events.len() < self.downloads {
            self.downloads = 0;
        }
        for &(event, value) in &events[self.downloads..] {
            self.download = match (event, self.download) {
                (download::LOOKING, Download::None | Download::Looking) => Download::Looking,
                (download::WAITING, Download::None | Download::Looking | Download::Waiting(_)) => {
                    Download::Waiting(value)
                }
                (download::FETCHING, _) => Download::Fetching {
                    expected: value,
                    received: 0,
                },
                (download::DONE, _) => Download::Done,
                (download::PROGRESS, Download::Fetching { expected, received }) => {
                    Download::Fetching {
                        expected,
                        received: received.wrapping_add(value),
                    }
                }
                (_, d) => d,
            };
        }
        self.downloads = events.len();
        // The session's end: the server's word on it, else the client's own stop, else a lost
        // connection once it was connected.
        if f.phase == Phase::Disconnected && was != Phase::Disconnected {
            let line = f
                .ended
                .and_then(end_line)
                .map(str::to_owned)
                .or_else(|| {
                    f.stopped.then(|| {
                        "The client stopped: it couldn't save the data the server sent.".to_owned()
                    })
                })
                .unwrap_or_else(|| "The connection to the server was lost.".to_owned());
            if self.ended.is_none() {
                self.ended = Some(line);
            }
            self.characters = None;
            self.login = None;
        } else if f.phase == Phase::Disconnected && self.ended.is_none() {
            // A connection that never got going says why as soon as the shell knows.
            if let Some(line) = f.ended.and_then(end_line) {
                self.ended = Some(line.to_owned());
            }
        }
    }
}
