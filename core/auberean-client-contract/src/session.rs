//! Where the session stands: its phase, how the login goes, the connection's measure, the account's
//! characters and how the session ended. Plain data the runtime fills and every front end reads.

/// Where the session is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// No server: before connecting, or after the session ended.
    Disconnected,
    /// Connecting: the login sent, the character list not here yet.
    Connecting,
    /// The account's characters are listed: choosing, making or deleting one.
    Characters,
    /// Entering the world as a character: asked, the player not here yet.
    Entering,
    /// In the world.
    World,
}

/// How the login stands, for the connecting screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoginProgress {
    /// A login went out: the attempt, of the most the client makes before it gives up.
    Asking { attempt: u32, of: u32 },
    /// The login server answered and was acknowledged: the client waits for its first packet.
    Waiting,
}

/// The error a connection was refused or let go with, by the id of the string that says it (the
/// connection error table's).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NetError(pub &'static str);

impl NetError {
    /// The server's address could not be used.
    pub const BAD_SERVER_ADDRESS: Self = Self("ID_NetError_BadServerAddress");

    /// The id of the string that says it.
    #[must_use]
    pub const fn id_string(self) -> &'static str {
        self.0
    }
}

/// How the session ended under the player, as far as it has: the connection failed or was let go
/// (`connect_failed`, with its error, or `None` inside: one this client has no name for), the login
/// failed (`login_failed`), and whether the server is gone (the client has logged off).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SessionEnd {
    pub connect_failed: Option<Option<NetError>>,
    pub login_failed: bool,
    pub server_died: bool,
}

impl SessionEnd {
    /// Whether the session can go no further: the login failed, or the server is gone.
    #[must_use]
    pub const fn over(&self) -> bool {
        self.server_died || self.login_failed
    }
}

/// The connection as the client measures it every two seconds or more: the average round trip
/// (seconds), the share of packets lost, the bytes received and sent over the last span (seconds),
/// and the seconds since the server was last heard; the packets received over the last span, and
/// the bytes received a second over the last two measures; `count` numbers the measures.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Heartbeat {
    pub count: u64,
    pub latency: f64,
    pub loss: f64,
    pub bytes_in: u32,
    pub bytes_out: u32,
    pub span: f32,
    pub since_heard: f32,
    pub packets_in: u16,
    pub rate_in: f64,
}

/// One of the account's characters, as the character list shows it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Character {
    pub id: u64,
    pub name: String,
    /// Seconds until a deleted character is gone; 0 for one in play.
    pub seconds_greyed_out: u32,
}

/// The account's characters, as the server listed them, and how many it may have; the account's
/// name, whether it has the expansion, and the list's third flag (1 or 0 in the recorded lists; the
/// creation screens read it as whether drudges may be made).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CharacterList {
    pub characters: Vec<Character>,
    pub slots: u32,
    pub account: String,
    pub expansion: bool,
    pub third_flag: bool,
}

impl CharacterList {
    /// The character of this name, matched without regard to case.
    #[must_use]
    pub fn named(&self, name: &str) -> Option<&Character> {
        self.characters
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(name))
    }
}
