//! The friends service's layer: friends with presence, the ignore list, the player's away state,
//! and tells to `Name@World`, as the client keeps them and hands them to its scripts.
//!
//! **The service.** A world may serve a friends service beside its login, reached over a
//! connection of its own ([`FriendsConnection`], the friends stream on the desktop). The layer
//! connects to it before the game login, presents the login's ticket, and only then lets the game
//! login go on ([`Client::frame`]). A service that cannot be reached leaves the player without
//! friends and the game goes on when the login was given its secret directly; a login without one
//! ends there, as it does when the service refuses the ticket ([`Refusal`]).
//!
//! **Requests and answers.** The scripts' requests (the friend and squelch commands, a tell to
//! `Name@World`, the session's start, the friends page) each go out with a mark of the layer's own;
//! the one answer to each comes back, its code turned into one of the client's own results, to the
//! player's scripts ([`Heard`]). A request made while the service is down is refused at once, and
//! the scripts show nothing for it.
//!
//! **Names.** The service names everyone `Name@World`. The layer adds `@` and the world's name (the
//! login's `WorldName`, the first one heard) to a name without `@`, and takes it off again when the
//! part from the `@` on is exactly the world's own ([`Friends::local_to_global`],
//! [`Friends::global_to_local`]).
//!
//! **Events** (a friend's state, the whole list as the character enters, a tell) reach the
//! scripts only once the ticket was taken, and only while a player is in the world.
//!
//! **Losing the service.** A stream that went down is dialed again on the connection's own
//! schedule, but only once one stood: an attempt that never got through is not repeated. On the
//! way back the ticket is presented again, and the scripts are asked to ask for their lists anew.

use std::collections::BTreeMap;

use core::time::Duration;

use auberean_client_net::friends::{
    Answer, Connect, Cx, Delivery, Event, Extra, FriendStatus, FriendsConnection, Hr, Request,
};
use auberean_script::native_object::{native_type, NativeObject, TextInfo};
use auberean_text::meta::{escape, retrieve_meta_letters};

pub use auberean_glue::friends::{UserStatus, USER_STATUS};

use crate::host::InterfaceArg;
use crate::Client;

/// The interface's functions the layer calls on the player's scripts.
pub mod function {
    /// A tell heard: the chat flags, the text, the teller, the teller's object (none).
    pub const HEAR_TELL: u32 = 0x00B4_003C;
    /// A tell's answer: the result, the text, the name told.
    pub const RESULT_SEND_TO_USER: u32 = 0x00B4_003D;
    /// A friend added or not: the result, the name.
    pub const RESULT_FRIEND_ADD: u32 = 0x00B4_0040;
    pub const RESULT_FRIEND_DELETE: u32 = 0x00B4_0041;
    /// The friends list: the result, the asker's word, the friends.
    pub const RESULT_GET_FRIENDS_LIST: u32 = 0x00B4_0042;
    /// Every friend with its state, as the character enters.
    pub const GUN_EVENT_FRIEND_START: u32 = 0x00B4_0043;
    /// One friend's state.
    pub const GUN_EVENT_FRIEND_STATUS: u32 = 0x00B4_0044;
    pub const RESULT_IGNORED_ADD: u32 = 0x00B4_0045;
    pub const RESULT_IGNORED_DELETE: u32 = 0x00B4_0046;
    /// The ignore list: the result, the asker's word, the names.
    pub const RESULT_GET_IGNORED_LIST: u32 = 0x00B4_0047;
    /// A help call's answer.
    pub const RESULT_CALL_CREATE: u32 = 0x00B4_0048;
    /// The service is back and the ticket taken again: ask for the lists again.
    pub const GUN_HANDLE_REAUTHENTICATION: u32 = 0x00B4_00F5;
}

/// The client's notices about the service, to the interface.
pub mod notice {
    /// The service took the ticket (word 1) or not (word 0) again, after coming back: shown as a
    /// line when no player is in the world, or when it refused.
    pub const AUTHENTICATED: u32 = 0x46C;
    /// The service closed the connection: the interface says so.
    pub const CONNECTION_LOST: u32 = 0x46A;
}

/// What a connect event means to the client's handlers.
pub mod status {
    pub const CONNECTED: u32 = 1;
    /// Connected again after a connection stood.
    pub const RECONNECTED: u32 = 2;
    /// The service closed the connection.
    pub const DISCONNECTED: u32 = 3;
    /// The attempt failed or the connection was lost.
    pub const LOST: u32 = 4;
}

/// What a ticket's answer means to the login.
pub mod ticket {
    pub const SUCCESS: u32 = 0;
    pub const NOT_TICKETED: u32 = 0xC112_080B;
    pub const ALREADY_TICKETED: u32 = 0xC112_080C;
    pub const BANNED: u32 = 0xC112_1005;
    /// Any other refusal.
    pub const FAILED: u32 = 0xC112_1006;
}

/// The flags word the session's start sets, and how.
const STATE_WORD: u32 = 1;
const SET: u32 = 1;

/// The service's code turned into one of the client's own results, as the scripts read them: 0 for
/// success, else the result whose message names the failure; a code the client does not know reads
/// as the unknown one.
// D2-12077
#[must_use]
pub fn weenie_result(hr: u32) -> u32 {
    match hr {
        0 => 0,
        0x8000_000A => 0xC73C,
        0x8007_0005 => 0xC74F,
        0xC112_0005 => 0xC748,
        0xC112_000C => 0xC8,
        0xC112_000F => 0xC743,
        0xC112_0010 => 0xC74E,
        0xC112_0011 => 0xC73B,
        0xC112_0012 => 0xC73D,
        0xC112_0014 | 0x4112_001B => 0xC744,
        0xC112_0018 => 0xC73F,
        0xC112_1003 => 0xC746,
        0xC112_1009 => 0xC740,
        0xC112_1401 => 0xC73E,
        0xC112_1800 => 0xC742,
        0xC112_1802 => 0xC747,
        0xC112_1803 => 0xC745,
        0xC112_1C04 => 0xC749,
        0xC112_2000 => 0xC74B,
        0xC112_2002 => 0xC74A,
        0xC112_2003 => 0xC74C,
        0x4112_001C => 0xC74D,
        _ => 0xC741,
    }
}

/// The service the player was started with: its host and port, whether to present the ticket (and
/// hear the service's events), the ticket to present, and whether the options named the service.
/// One not named (tried at the login's own host) never ends the login: without a service named,
/// nothing of the service can.
#[derive(Clone, PartialEq, Eq)]
pub struct FriendsOptions {
    pub server: String,
    pub port: u16,
    pub use_ticket: bool,
    pub ticket: Option<String>,
    pub named: bool,
}

impl core::fmt::Debug for FriendsOptions {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("FriendsOptions")
            .field("server", &self.server)
            .field("port", &self.port)
            .field("use_ticket", &self.use_ticket)
            .field("ticket", &self.ticket.as_ref().map(|_| "***"))
            .field("named", &self.named)
            .finish()
    }
}

/// Why the login ended at the friends service.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The ticket could not be had, or was refused.
    Failed,
    /// The account is banned.
    Banned,
}

/// Where the friends service's part of the login is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Startup {
    /// Over, or never asked for: the game login goes on.
    Done,
    /// Waiting for the first attempt to connect to come back.
    Connecting,
    /// Waiting for the ticket's answer.
    Ticketing,
    /// The login ended.
    Refused(Refusal),
}

/// A request waiting for its answer: what the client keeps to hand the answer on, dropped (its
/// names and text with it) once the answer is handed on.
// D2-00362, D2-00391, D2-00399, D2-12152
#[derive(Clone, Debug, PartialEq, Eq)]
enum Waiting {
    FriendAdd(Vec<u16>),
    FriendDelete(Vec<u16>),
    IgnoredAdd(Vec<u16>),
    IgnoredDelete(Vec<u16>),
    /// The asker's word, handed back with the list.
    FriendsList(u32),
    IgnoredList(u32),
    Flags,
    Location,
    SendToUser {
        user: Vec<u16>,
        text: Vec<u16>,
    },
    CallCreate {
        kind: u32,
        target: Vec<u16>,
        description: Vec<u16>,
    },
    Ticket,
}

/// What the layer hands the client: the answers and events for the player's scripts, and the
/// connection's news.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Heard {
    FriendAdd {
        result: u32,
        user: Vec<u16>,
    },
    FriendDelete {
        result: u32,
        user: Vec<u16>,
    },
    IgnoredAdd {
        result: u32,
        user: Vec<u16>,
    },
    IgnoredDelete {
        result: u32,
        user: Vec<u16>,
    },
    FriendsList {
        result: u32,
        context: u32,
        friends: Vec<UserStatus>,
    },
    IgnoredList {
        result: u32,
        context: u32,
        names: Vec<Vec<u16>>,
    },
    SendToUser {
        user: Vec<u16>,
        text: Vec<u16>,
        result: u32,
    },
    CallCreate {
        result: u32,
        kind: u32,
        target: Vec<u16>,
        description: Vec<u16>,
    },
    /// Every friend with its state, the character entering.
    Entered(Vec<UserStatus>),
    /// One friend's state.
    FriendState(UserStatus),
    /// A tell: the teller as the service names it, the text, the teller's extra data.
    Tell {
        teller: Vec<u16>,
        text: Vec<u16>,
        extra: Option<Extra>,
    },
    /// A connect event's meaning ([`status`]).
    Connection(u32),
    /// The ticket's answer ([`ticket`]).
    Authenticated(u32),
}

/// What a tell carries besides its text: a version (1), the chat flags (an allegiance's tell and
/// the like) and the teller's meta letters, so that the hearer writes the teller's name as the
/// teller's own client would.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChatExtra {
    pub version: u16,
    pub flags: u32,
    pub meta_letters: Vec<u16>,
}

impl ChatExtra {
    /// The extra data of a tell from a player named `name` (its literal, meta letters kept): the
    /// meta letters are the name's.
    // D2-00661, D2-00662, D2-00663
    #[must_use]
    pub fn new(name: &[u16], flags: u32) -> Self {
        Self {
            version: 1,
            flags,
            meta_letters: retrieve_meta_letters(name).1,
        }
    }

    /// Its packed form: the version, a zero word, the flags, the meta letters (a version other
    /// than 1 packs the first two alone).
    // D2-00701
    #[must_use]
    pub fn pack(&self) -> Vec<u8> {
        let mut w = auberean_script::wire::Out::new();
        w.u16(self.version);
        w.u16(0);
        if self.version == 1 {
            w.u32(self.flags);
            w.wide(&self.meta_letters);
        }
        w.buf
    }

    /// One read from its packed form: `None` for fewer than eight bytes. A version other than 1
    /// leaves the flags and the letters as a new one has them; letters cut short read as none.
    // D2-00701
    #[must_use]
    pub fn unpack(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 8 {
            return None;
        }
        let mut r = auberean_script::wire::In::new(bytes);
        let version = r.u16().ok()?;
        r.u16().ok()?;
        let mut me = Self {
            version,
            ..Self::default()
        };
        if version == 1 {
            me.flags = r.u32().ok()?;
            me.meta_letters = r.wide().unwrap_or_default();
        }
        Some(me)
    }

    /// As the stream carries it: sixteen bytes, the packed form and zeros after it.
    // D2-00637
    #[must_use]
    pub fn to_stream(&self) -> Extra {
        let mut out = [0u8; 16];
        for (o, b) in out.iter_mut().zip(self.pack()) {
            *o = b;
        }
        Extra(out)
    }
}

/// The friends service's layer.
#[derive(Debug)]
pub struct Friends {
    connection: Box<dyn FriendsConnection>,
    /// The service's host and port, kept to connect again.
    server: String,
    port: u16,
    connected: bool,
    connecting: bool,
    /// A connection stood once in this login: one lost is dialed again.
    persistent: bool,
    /// The world's name, set once, and `@` with it.
    world_name: Vec<u16>,
    world_suffix: Vec<u16>,
    /// The ticket handed over to present.
    ticket: Option<Vec<u8>>,
    /// The connect events the login's handler has had, once it is registered.
    connect_handler: Option<u32>,
    chat_handler: bool,
    friends_handler: bool,
    next_cx: u64,
    waiting: BTreeMap<Cx, Waiting>,
    /// The ticket's handler: whether its answer came, and what it was.
    ticket_answer: Option<u32>,
    /// A ticket was presented and is not finished with.
    ticket_held: bool,
    /// The ticket is being presented again after the service came back.
    reauthenticating: bool,
    use_ticket: bool,
    /// The options named the service.
    named: bool,
    startup: Startup,
    now: Duration,
}

impl Friends {
    /// The layer over a connection, with the options the player was started with: the ticket
    /// handed over now, the connection made at the login ([`Client::start_friends`]).
    ///
    /// [`Client::start_friends`]: crate::Client::start_friends
    // D2-12117, D2-12113, D2-12133, D2-00346
    #[must_use]
    pub fn new(connection: Box<dyn FriendsConnection>, options: &FriendsOptions) -> Self {
        let mut me = Self {
            connection,
            server: options.server.clone(),
            port: options.port,
            connected: false,
            connecting: false,
            persistent: false,
            world_name: Vec::new(),
            world_suffix: Vec::new(),
            ticket: None,
            connect_handler: None,
            chat_handler: false,
            friends_handler: false,
            next_cx: 1,
            waiting: BTreeMap::new(),
            ticket_answer: None,
            ticket_held: false,
            reauthenticating: false,
            use_ticket: options.use_ticket,
            named: options.named,
            startup: Startup::Done,
            now: Duration::ZERO,
        };
        if let Some(t) = options.ticket.as_ref().filter(|t| !t.is_empty()) {
            me.set_ticket(t.as_bytes());
        }
        me
    }

    /// The connection, for its host.
    pub fn connection(&mut self) -> &mut dyn FriendsConnection {
        &mut *self.connection
    }

    /// Where the service's part of the login is.
    #[must_use]
    pub fn startup(&self) -> Startup {
        self.startup
    }

    /// Whether the service is connected.
    #[must_use]
    pub fn connected(&self) -> bool {
        self.connected
    }

    /// The world's name as the login named it (empty before), and `@` with it.
    #[must_use]
    pub fn world_name(&self) -> &[u16] {
        &self.world_name
    }

    #[must_use]
    pub fn world_suffix(&self) -> &[u16] {
        &self.world_suffix
    }

    /// The world's name, from the login: set the first time only; `@` and the name are added to
    /// names from then on.
    // D2-01686
    pub fn set_world_name(&mut self, name: &[u16]) -> bool {
        if !self.world_name.is_empty() {
            return false;
        }
        name.clone_into(&mut self.world_name);
        self.world_suffix = std::iter::once(u16::from(b'@'))
            .chain(name.iter().copied())
            .collect();
        true
    }

    /// A name as the service names it: `@` and the world's name added to one without `@`.
    // D2-12119
    #[must_use]
    pub fn local_to_global(&self, name: &[u16]) -> Vec<u16> {
        let mut out = name.to_vec();
        if !name.contains(&u16::from(b'@')) {
            out.extend_from_slice(&self.world_suffix);
        }
        out
    }

    /// A name as the client shows it: the part from the first `@` on taken off when it is exactly
    /// `@` and the world's name.
    // D2-12120, D2-12138
    #[must_use]
    pub fn global_to_local(&self, name: &[u16]) -> Vec<u16> {
        match name.iter().position(|&u| u == u16::from(b'@')) {
            Some(at) if !self.world_suffix.is_empty() && name[at..] == self.world_suffix[..] => {
                name[..at].to_vec()
            }
            _ => name.to_vec(),
        }
    }

    /// Hand over the ticket to present.
    // D2-12108
    pub fn set_ticket(&mut self, ticket: &[u8]) -> bool {
        self.ticket = Some(ticket.to_vec());
        true
    }

    /// The ticket to present: only while connected, and only when one was handed over.
    // D2-12085
    #[must_use]
    pub fn ticket(&self) -> Option<Vec<u8>> {
        if !self.connected {
            return None;
        }
        self.ticket.clone()
    }

    /// Connect to the service at `server` and `port`; the attempt's outcome comes back as a
    /// connect event.
    // D2-12107
    pub fn connect(&mut self, server: &str, port: u16) -> bool {
        self.persistent = false;
        self.connection.connect(self.now, server, port);
        server.clone_into(&mut self.server);
        self.port = port;
        self.connecting = true;
        true
    }

    /// Listen for connect events (the login's own handler), for the service's chat events (tells),
    /// and for its friends events.
    // D2-12110, D2-01689, D2-01670
    pub fn register_for_connect_events(&mut self) -> bool {
        self.connect_handler = Some(0);
        true
    }

    // D2-12111, D2-00353
    pub fn register_for_chat_events(&mut self) -> bool {
        self.chat_handler = true;
        true
    }

    // D2-12112
    pub fn register_for_friends_events(&mut self) -> bool {
        self.friends_handler = true;
        true
    }

    /// Stop listening for every event.
    // D2-12106, D2-12114, D2-12115, D2-12116
    pub fn unregister_events(&mut self) {
        self.connect_handler = None;
        self.chat_handler = false;
        self.friends_handler = false;
    }

    /// How many connect events the login's handler has had.
    // D2-17597
    #[must_use]
    pub fn connect_events_handled(&self) -> u32 {
        self.connect_handler.unwrap_or(0)
    }

    /// Send a request with a new mark, keeping what its answer is to be handed on with; refused
    /// at once while not connected. Whether it went out.
    // D2-12137
    fn request(&mut self, request: Request, waiting: Waiting) -> bool {
        if !self.connected {
            return false;
        }
        let cx = Cx(self.next_cx);
        self.next_cx += 1;
        self.waiting.insert(cx, waiting);
        self.connection.request(self.now, cx, request);
        true
    }

    fn string(units: &[u16]) -> String {
        String::from_utf16_lossy(units)
    }

    /// Present the ticket: its answer decides the login, or the session's going on after the
    /// service came back.
    // D2-12098, D2-12151, D2-12153, D2-12154, D2-00402
    pub fn present_ticket(&mut self) -> bool {
        let Some(t) = self.ticket() else {
            return false;
        };
        let ticket = String::from_utf8_lossy(&t)
            .trim_end_matches('\0')
            .to_owned();
        self.ticket_answer = None;
        self.ticket_held = true;
        self.request(
            Request::PresentTicket {
                email: String::new(),
                ticket,
            },
            Waiting::Ticket,
        )
    }

    /// The client is done with its ticket, when one was presented and the service is up: whether
    /// it was. The stream's own end gives the ticket back; no element says so.
    // D2-12099
    pub fn finish_with_ticket(&mut self) -> bool {
        if !self.connected || !self.ticket_held {
            return false;
        }
        self.ticket_held = false;
        true
    }

    /// The layer ends: the connection closes, if it stands, and no event is listened for.
    // D2-12118
    pub fn end(&mut self) {
        if self.connected {
            self.connection.disconnect(self.now);
            self.connected = false;
        }
        self.unregister_events();
    }

    /// Ask for the friends list; `context` comes back with it.
    // D2-12101, D2-00365
    pub fn get_friends_list(&mut self, context: u32) -> bool {
        self.request(Request::GetFriendsList, Waiting::FriendsList(context))
    }

    /// Ask for the ignore list; `context` comes back with it.
    // D2-12102, D2-00368
    pub fn get_ignored_list(&mut self, context: u32) -> bool {
        self.request(Request::GetIgnoredList, Waiting::IgnoredList(context))
    }

    /// Set the player's state flags for the session (the session's start does).
    // D2-12103, D2-00666
    pub fn set_state_flags(&mut self, flags: u32) -> bool {
        self.request(
            Request::Flags {
                op: SET,
                word: STATE_WORD,
                flags,
            },
            Waiting::Flags,
        )
    }

    /// Set the player's state (available, away, not to be disturbed) and where it is.
    // D2-12104, D2-00667
    pub fn set_location(&mut self, flags: u32, location: &[u16]) -> bool {
        self.request(
            Request::SetLoc {
                flags,
                location: Self::string(location),
            },
            Waiting::Location,
        )
    }

    /// Add a friend, take one off, ignore a name, stop ignoring one: the name as the service names
    /// it.
    // D2-12125, D2-00361
    pub fn friend_add(&mut self, user: &[u16]) -> bool {
        let user_global = Self::string(&self.local_to_global(user));
        self.request(
            Request::FriendAdd { user: user_global },
            Waiting::FriendAdd(user.to_vec()),
        )
    }

    // D2-12126, D2-00364
    pub fn friend_delete(&mut self, user: &[u16]) -> bool {
        let user_global = Self::string(&self.local_to_global(user));
        self.request(
            Request::FriendDelete { user: user_global },
            Waiting::FriendDelete(user.to_vec()),
        )
    }

    // D2-12127, D2-00388
    pub fn ignored_add(&mut self, user: &[u16]) -> bool {
        let user_global = Self::string(&self.local_to_global(user));
        self.request(
            Request::IgnoredAdd { user: user_global },
            Waiting::IgnoredAdd(user.to_vec()),
        )
    }

    // D2-12128, D2-00390
    pub fn ignored_delete(&mut self, user: &[u16]) -> bool {
        let user_global = Self::string(&self.local_to_global(user));
        self.request(
            Request::IgnoredDelete { user: user_global },
            Waiting::IgnoredDelete(user.to_vec()),
        )
    }

    /// A tell: both the name and the text must be there. The name goes as the service names it,
    /// with the teller's extra data.
    // D2-12124, D2-00397
    pub fn send_to_user(&mut self, user: &[u16], text: &[u16], extra: &ChatExtra) -> bool {
        if user.is_empty() || text.is_empty() {
            return false;
        }
        let user_global = Self::string(&self.local_to_global(user));
        self.request(
            Request::SendToUserEx {
                user: user_global,
                text: Self::string(text),
                extra: Some(extra.to_stream()),
            },
            Waiting::SendToUser {
                user: user.to_vec(),
                text: text.to_vec(),
            },
        )
    }

    /// A call for help of a kind, about a player, with a description.
    // D2-12129, D2-00358, D2-00357
    pub fn call_create(&mut self, kind: u32, target: &[u16], description: &[u16]) -> bool {
        self.request(
            Request::CallCreate,
            Waiting::CallCreate {
                kind,
                target: target.to_vec(),
                description: description.to_vec(),
            },
        )
    }

    /// Take everything the connection brought, in order: each answer handed on with what its
    /// request kept, each event to the handlers listening for its kind. An answer with a mark no
    /// request is waiting for is dropped.
    // D2-12084, D2-12086, D2-12087, D2-12105, D2-12078, D2-12079, D2-12080
    pub fn dispatch(&mut self, now: Duration) -> Vec<Heard> {
        self.now = now;
        let mut heard = Vec::new();
        for d in self.connection.dispatch(now) {
            match d {
                Delivery::Connect(c) => self.connect_event(c, &mut heard),
                Delivery::Answer { cx, hr, answer } => {
                    let Some(waiting) = self.waiting.remove(&cx) else {
                        continue;
                    };
                    self.answer(waiting, hr, answer, &mut heard);
                }
                Delivery::Event(e) => self.event(e, &mut heard),
            }
        }
        heard
    }

    /// A connect event: connected (again, when one stood before), or down, which dials again when
    /// a connection stood in this login.
    // D2-12109, D2-00350, D2-00351
    fn connect_event(&mut self, c: Connect, heard: &mut Vec<Heard>) {
        let status = match c {
            Connect::Connected => {
                let s = if self.persistent {
                    status::RECONNECTED
                } else {
                    status::CONNECTED
                };
                self.connected = true;
                self.persistent = true;
                self.connecting = false;
                s
            }
            Connect::Closed => {
                self.connected = false;
                status::DISCONNECTED
            }
            Connect::Failed => {
                self.connected = false;
                status::LOST
            }
        };
        if let Some(n) = &mut self.connect_handler {
            *n += 1;
            heard.push(Heard::Connection(status));
        }
        if c != Connect::Connected && self.persistent {
            let (server, port) = (self.server.clone(), self.port);
            self.connection.connect(self.now, &server, port);
            self.persistent = true;
        }
    }

    /// An answer, handed on with what its request kept, its code turned into the client's result.
    // D2-12095, D2-12094, D2-12097, D2-12092, D2-00360, D2-00363, D2-00367, D2-00386, D2-00387
    // D2-00389, D2-00396, D2-00355, D2-00401, D2-00400
    fn answer(&mut self, waiting: Waiting, hr: Hr, answer: Answer, heard: &mut Vec<Heard>) {
        let result = weenie_result(hr.0);
        match waiting {
            Waiting::FriendAdd(user) => heard.push(Heard::FriendAdd { result, user }),
            Waiting::FriendDelete(user) => heard.push(Heard::FriendDelete { result, user }),
            Waiting::IgnoredAdd(user) => heard.push(Heard::IgnoredAdd { result, user }),
            Waiting::IgnoredDelete(user) => heard.push(Heard::IgnoredDelete { result, user }),
            Waiting::FriendsList(context) => {
                let friends = match answer {
                    Answer::GetFriendsList { friends } if hr.is_ok() => {
                        friends.iter().map(|f| self.status_of(f, true)).collect()
                    }
                    _ => Vec::new(),
                };
                heard.push(Heard::FriendsList {
                    result,
                    context,
                    friends,
                });
            }
            Waiting::IgnoredList(context) => {
                let names = match answer {
                    Answer::GetIgnoredList { ignored } if hr.is_ok() => ignored
                        .iter()
                        .map(|n| self.global_to_local(&n.encode_utf16().collect::<Vec<_>>()))
                        .collect(),
                    _ => Vec::new(),
                };
                heard.push(Heard::IgnoredList {
                    result,
                    context,
                    names,
                });
            }
            // The flags' and the state's answers are taken and nothing more.
            Waiting::Flags | Waiting::Location => {}
            Waiting::SendToUser { user, text } => {
                heard.push(Heard::SendToUser { user, text, result });
            }
            Waiting::CallCreate {
                kind,
                target,
                description,
            } => heard.push(Heard::CallCreate {
                result,
                kind,
                target,
                description,
            }),
            Waiting::Ticket => {
                let t = match hr.0 {
                    0 => ticket::SUCCESS,
                    ticket::ALREADY_TICKETED => ticket::ALREADY_TICKETED,
                    ticket::BANNED => ticket::BANNED,
                    _ => ticket::FAILED,
                };
                self.ticket_answer = Some(t);
                heard.push(Heard::Authenticated(t));
            }
        }
    }

    /// A friend's state as the client keeps it: its name as the client shows it (when `local`),
    /// its mode and where it is.
    // D2-00354, D2-00352
    fn status_of(&self, f: &FriendStatus, local: bool) -> UserStatus {
        let name: Vec<u16> = f.user.encode_utf16().collect();
        UserStatus {
            name: if local {
                self.global_to_local(&name)
            } else {
                name
            },
            location: f.location.encode_utf16().collect(),
            mode: f.mode.0,
        }
    }

    /// An event, to the handlers listening for its kind (none before the ticket was taken): the
    /// whole list (an empty one is not handed on), one friend's state, a tell (its teller's name
    /// as the service gives it).
    // D2-12122, D2-12121, D2-00349
    fn event(&mut self, e: Event, heard: &mut Vec<Heard>) {
        match e {
            Event::FStart { friends } => {
                if self.friends_handler && !friends.is_empty() {
                    let list = friends.iter().map(|f| self.status_of(f, true)).collect();
                    heard.push(Heard::Entered(list));
                }
            }
            Event::FriendStat { user, mode } => {
                if self.friends_handler {
                    let f = FriendStatus {
                        user,
                        mode,
                        location: String::new(),
                    };
                    heard.push(Heard::FriendState(self.status_of(&f, true)));
                }
            }
            Event::UTellEv { user, text, extra } => {
                if self.chat_handler {
                    heard.push(Heard::Tell {
                        teller: user.encode_utf16().collect(),
                        text: text.encode_utf16().collect(),
                        extra,
                    });
                }
            }
        }
    }
}

impl Client {
    /// Give the client a friends service's layer before its first frame: the service's part of
    /// the login starts, and the game login waits for it.
    pub fn use_friends(&mut self, layer: Friends) {
        self.friends = Some(layer);
        self.start_friends();
    }

    /// A request of the friends service, when there is one: whether it went out.
    pub(crate) fn with_friends(&mut self, f: impl FnOnce(&mut Friends) -> bool) -> bool {
        self.friends.as_mut().is_some_and(f)
    }

    /// The client ends: it finishes with its ticket and the friends service's layer ends.
    pub fn end_friends(&mut self) {
        if let Some(f) = &mut self.friends {
            f.finish_with_ticket();
            f.end();
        }
    }

    /// The friends service's layer, when the player was started with one.
    #[must_use]
    pub fn friends(&self) -> Option<&Friends> {
        self.friends.as_ref()
    }

    pub fn friends_mut(&mut self) -> Option<&mut Friends> {
        self.friends.as_mut()
    }

    /// Why the login ended at the friends service, if it did.
    #[must_use]
    pub fn friends_refusal(&self) -> Option<Refusal> {
        match self.friends.as_ref()?.startup {
            Startup::Refused(r) => Some(r),
            _ => None,
        }
    }

    /// Whether the game login waits for the friends service.
    pub(crate) fn friends_hold_login(&self) -> bool {
        self.friends
            .as_ref()
            .is_some_and(|f| f.startup != Startup::Done)
    }

    /// The friends service's part of the login, as it starts: listen for connect events, then
    /// connect; the game login waits for the outcome.
    // D2-01699, D2-01701
    pub(crate) fn start_friends(&mut self) {
        let Some(f) = self.friends.as_mut() else {
            return;
        };
        if f.server.is_empty() {
            f.startup = Startup::Done;
            return;
        }
        f.register_for_connect_events();
        let (server, port) = (f.server.clone(), f.port);
        f.connect(&server, port);
        f.startup = Startup::Connecting;
    }

    /// The friends service's news, for a host that hands the client its messages itself rather
    /// than running its frames: the world's name from the login, then what the service sent.
    pub fn pump_friends(&mut self) {
        self.friends_world_name();
        self.friends_frame();
    }

    /// The service's news this frame: what came back, handed on; then the login's wait, when it
    /// is waiting. A service that could not be reached lets the game login go on without it.
    // D2-01699, D2-01693
    pub(crate) fn friends_frame(&mut self) {
        let now = Duration::from_secs_f64(self.now.max(0.0));
        let Some(f) = self.friends.as_mut() else {
            return;
        };
        let heard = f.dispatch(now);
        for h in heard {
            self.hear_friends(h);
        }
        let Some(f) = self.friends.as_mut() else {
            return;
        };
        match f.startup {
            Startup::Connecting if f.connect_events_handled() > 0 => {
                // Not reached: the game goes on without it when the login was given its secret
                // directly (or the service was not named); a login without its secret ends.
                // Reached with no ticket to present: requests answered, events never heard.
                if !f.connected && f.named && !self.login_secret_given {
                    f.startup = Startup::Refused(Refusal::Failed);
                } else if !f.connected || !f.use_ticket {
                    f.startup = Startup::Done;
                } else if self.authenticate_with_friends(false) {
                    if let Some(f) = self.friends.as_mut() {
                        f.startup = Startup::Ticketing;
                    }
                } else if let Some(f) = self.friends.as_mut() {
                    f.startup = Startup::Refused(Refusal::Failed);
                }
            }
            Startup::Ticketing => {
                if let Some(answer) = f.ticket_answer {
                    if answer == ticket::SUCCESS {
                        f.register_for_chat_events();
                        f.register_for_friends_events();
                        f.startup = Startup::Done;
                    } else {
                        let why = if answer == ticket::BANNED {
                            Refusal::Banned
                        } else {
                            Refusal::Failed
                        };
                        // The login is over: the service is left for good.
                        f.unregister_events();
                        f.connection.disconnect(now);
                        f.startup = Startup::Refused(why);
                    }
                }
            }
            _ => {}
        }
    }

    /// Present the ticket: whether it went out (none to present, or not connected: no).
    // D2-01673
    fn authenticate_with_friends(&mut self, again: bool) -> bool {
        let Some(f) = self.friends.as_mut() else {
            return false;
        };
        f.reauthenticating = again;
        f.ticket().is_some() && f.present_ticket()
    }

    /// The world's name, from the login, to the layer.
    // D2-01695
    pub(crate) fn friends_world_name(&mut self) {
        if self
            .friends
            .as_ref()
            .is_none_or(|f| !f.world_name.is_empty())
        {
            return;
        }
        let Some(units) = self.world_name_units() else {
            return;
        };
        if let Some(f) = self.friends.as_mut() {
            f.set_world_name(&units);
        }
    }

    /// The world's name as the login last named it, whether or not the friends service runs.
    fn world_name_units(&mut self) -> Option<Vec<u16>> {
        let mut info = crate::text::from_wire(self.model.world_name.as_ref()?);
        Some(match self.scripts.as_mut() {
            Some(s) => info.text(&mut s.host.text).encode_utf16().collect(),
            None => info.literal_value(false),
        })
    }

    /// The world's name as text; empty before the login names it.
    pub fn world_name(&mut self) -> String {
        self.world_name_units()
            .map_or_else(String::new, |u| String::from_utf16_lossy(&u))
    }

    /// One thing the layer heard, handed on: to the player's scripts (through the player's own
    /// interface, so nothing reaches them while no player stands), to the login, or to the
    /// interface.
    fn hear_friends(&mut self, h: Heard) {
        use function as f;
        use InterfaceArg::{Long, Object, Objects, Text, Texts, Word};
        let status = |s: UserStatus| s.native();
        match h {
            // D2-00513, D2-01231
            Heard::FriendAdd { result, user } => {
                self.ask(f::RESULT_FRIEND_ADD, &[Text(user), Word(result)]);
            }
            // D2-00514, D2-01232
            Heard::FriendDelete { result, user } => {
                self.ask(f::RESULT_FRIEND_DELETE, &[Text(user), Word(result)]);
            }
            // D2-00516, D2-01235
            Heard::IgnoredAdd { result, user } => {
                self.ask(f::RESULT_IGNORED_ADD, &[Text(user), Word(result)]);
            }
            // D2-00517, D2-01236
            Heard::IgnoredDelete { result, user } => {
                self.ask(f::RESULT_IGNORED_DELETE, &[Text(user), Word(result)]);
            }
            // D2-00710, D2-01233, D2-01056
            Heard::FriendsList {
                result,
                context,
                friends,
            } => {
                let list = Objects {
                    ty: USER_STATUS,
                    objects: friends.into_iter().map(status).collect(),
                };
                self.ask(
                    f::RESULT_GET_FRIENDS_LIST,
                    &[list, Word(context), Word(result)],
                );
            }
            // D2-00712, D2-01234
            Heard::IgnoredList {
                result,
                context,
                names,
            } => {
                self.ask(
                    f::RESULT_GET_IGNORED_LIST,
                    &[Texts(names), Word(context), Word(result)],
                );
            }
            // D2-00511, D2-01238
            Heard::SendToUser { user, text, result } => {
                self.ask(
                    f::RESULT_SEND_TO_USER,
                    &[Word(result), Text(text), Text(user)],
                );
            }
            // D2-00518, D2-01230
            Heard::CallCreate {
                result,
                kind,
                target,
                description,
            } => {
                self.ask(
                    f::RESULT_CALL_CREATE,
                    &[
                        Word(0),
                        Word(0),
                        Text(description),
                        Text(target),
                        Word(kind),
                        Word(result),
                    ],
                );
            }
            // D2-00711, D2-01102
            Heard::Entered(list) => {
                let list = Objects {
                    ty: USER_STATUS,
                    objects: list.into_iter().map(status).collect(),
                };
                self.ask(f::GUN_EVENT_FRIEND_START, &[list]);
            }
            // D2-00515, D2-01103
            Heard::FriendState(s) => {
                let s = Object {
                    ty: USER_STATUS,
                    object: status(s),
                };
                self.ask(f::GUN_EVENT_FRIEND_STATUS, &[s]);
            }
            Heard::Tell {
                teller,
                text,
                extra,
            } => {
                let (name, text, flags) = heard_tell(teller, &text, extra);
                let info = |literal| Object {
                    ty: native_type::STRING_INFO,
                    object: NativeObject::TextInfo(TextInfo {
                        has_literal: 1,
                        literal: Some(literal),
                        ..TextInfo::default()
                    }),
                };
                self.ask(
                    f::HEAR_TELL,
                    &[Word(flags), info(text), info(name), Long(0)],
                );
            }
            Heard::Connection(s) => self.friends_connection(s),
            Heard::Authenticated(t) => self.friends_authenticated(t),
        }
    }

    /// A connect event's meaning: back after a connection stood, the ticket is presented again;
    /// closed by the service, the interface says so; connected, or lost, nothing hears it.
    // D2-00350, D2-01696, D2-00313, D2-00314, D2-00315
    fn friends_connection(&mut self, s: u32) {
        match s {
            status::RECONNECTED => {
                if !self.authenticate_with_friends(true) {
                    self.friends_line(false);
                }
            }
            status::CONNECTED | status::LOST => {}
            _ => self.notices.push(crate::host::PanelNotice {
                native: notice::CONNECTION_LOST,
                about: None,
                words: Vec::new(),
                objects: Vec::new(),
                requests: Vec::new(),
                effect_lines: Vec::new(),
                listed: Vec::new(),
            }),
        }
    }

    /// The ticket's answer, after the service came back: with a player in the world, its scripts
    /// ask for their lists again; else a line says how it went.
    // D2-01672, D2-00510, D2-01105
    fn friends_authenticated(&mut self, t: u32) {
        let Some(f) = self.friends.as_mut() else {
            return;
        };
        if !f.reauthenticating {
            return;
        }
        f.reauthenticating = false;
        let ok = t == ticket::SUCCESS;
        if ok && self.model.player_object().is_some() {
            self.ask(function::GUN_HANDLE_REAUTHENTICATION, &[]);
        } else {
            self.friends_line(ok);
        }
    }

    /// The line about the ticket presented again, to the interface.
    // D2-00312
    fn friends_line(&mut self, ok: bool) {
        self.notices.push(crate::host::PanelNotice {
            native: notice::AUTHENTICATED,
            about: None,
            words: vec![u32::from(ok)],
            objects: Vec::new(),
            requests: Vec::new(),
            effect_lines: Vec::new(),
            listed: Vec::new(),
        });
    }
}

/// A heard tell as the scripts take it: the teller's name with its meta letters (from the extra
/// data) in brackets after it, the text escaped so that nothing in it reads as markup, and the
/// chat flags (0 without extra data).
// D2-00696, D2-01156
#[must_use]
pub fn heard_tell(
    teller: Vec<u16>,
    text: &[u16],
    extra: Option<Extra>,
) -> (Vec<u16>, Vec<u16>, u32) {
    let extra = extra.and_then(|e| ChatExtra::unpack(&e.0));
    let mut name = teller;
    let flags = extra.as_ref().map_or(0, |e| e.flags);
    if let Some(e) = extra.filter(|e| !e.meta_letters.is_empty()) {
        name.push(u16::from(b'['));
        name.extend_from_slice(&e.meta_letters);
        name.push(u16::from(b']'));
    }
    (name, escape(text), flags)
}

// ---------------------------------------------------------------------------------------------
// The scripts' requests
// ---------------------------------------------------------------------------------------------

use auberean_script::native::{Call, NativeFn, Status, Stop};
use auberean_script::value::{tag, NativeId, NULL};

use crate::host::host;

/// The interface's package, and the friends utilities'.
const GM_INTERFACE: u32 = 0xB4;
const FRIENDS_UTILS: u32 = 0x1AD5;

/// The string a reference at `at` names (empty for the null reference).
pub(crate) fn string_at(c: &mut Call<'_>, at: i64) -> Result<Vec<u16>, Stop> {
    let r = c.cx.at(at)?;
    if r == NULL {
        return Ok(Vec::new());
    }
    match c.vm.heap.native(r)? {
        NativeObject::Text(t) => Ok(t.clone()),
        _ => Err(c.fail("a string argument is not one")),
    }
}

/// A request made of the layer, its answer (1 when it went out, 0 when there is no service or it
/// is down) below its `ARGS` arguments.
fn ask_layer(
    c: &mut Call<'_>,
    args: i64,
    f: impl FnOnce(&mut Friends, &crate::host::ClientHost) -> bool,
) -> Result<Status, Stop> {
    let h = host(c)?;
    let ok = match h.friends.take() {
        Some(mut layer) => {
            let ok = f(&mut layer, h);
            h.friends = Some(layer);
            ok
        }
        None => false,
    };
    c.write(-2 - args, u32::from(ok), tag::MIXED)?;
    Ok(Status::Success)
}

/// The null receiver stops the call as the client's own crash would.
pub(crate) fn no_receiver(c: &mut Call<'_>) -> Result<bool, Stop> {
    Ok(c.receiver()? == NULL)
}

/// Add a friend (`fp[-2]`).
// D2-00990, D2-00640
fn friend_add(c: &mut Call<'_>) -> Result<Status, Stop> {
    if no_receiver(c)? {
        return Ok(Status::Crash);
    }
    let user = string_at(c, -2)?;
    ask_layer(c, 1, |f, _| f.friend_add(&user))
}

/// Take a friend (`fp[-2]`) off the list.
// D2-00991, D2-00641
fn friend_delete(c: &mut Call<'_>) -> Result<Status, Stop> {
    if no_receiver(c)? {
        return Ok(Status::Crash);
    }
    let user = string_at(c, -2)?;
    ask_layer(c, 1, |f, _| f.friend_delete(&user))
}

/// Ignore a name (`fp[-2]`).
// D2-00994, D2-00643
fn ignored_add(c: &mut Call<'_>) -> Result<Status, Stop> {
    if no_receiver(c)? {
        return Ok(Status::Crash);
    }
    let user = string_at(c, -2)?;
    ask_layer(c, 1, |f, _| f.ignored_add(&user))
}

/// Stop ignoring a name (`fp[-2]`).
// D2-00995, D2-00644
fn ignored_delete(c: &mut Call<'_>) -> Result<Status, Stop> {
    if no_receiver(c)? {
        return Ok(Status::Crash);
    }
    let user = string_at(c, -2)?;
    ask_layer(c, 1, |f, _| f.ignored_delete(&user))
}

/// Ask for the friends list, the asker's word (`fp[-2]`) to come back with it.
// D2-00992, D2-00642
fn get_friends_list(c: &mut Call<'_>) -> Result<Status, Stop> {
    if no_receiver(c)? {
        return Ok(Status::Crash);
    }
    let context = c.cx.at(-2)?;
    ask_layer(c, 1, |f, _| f.get_friends_list(context))
}

/// Ask for the ignore list, the asker's word (`fp[-2]`) to come back with it.
// D2-00993, D2-00645
fn get_ignored_list(c: &mut Call<'_>) -> Result<Status, Stop> {
    if no_receiver(c)? {
        return Ok(Status::Crash);
    }
    let context = c.cx.at(-2)?;
    ask_layer(c, 1, |f, _| f.get_ignored_list(context))
}

/// Set the player's state flags (`fp[-2]`) for the session.
// D2-00999, D2-00686
fn set_state_flags(c: &mut Call<'_>) -> Result<Status, Stop> {
    if no_receiver(c)? {
        return Ok(Status::Crash);
    }
    let flags = c.cx.at(-2)?;
    ask_layer(c, 1, |f, _| f.set_state_flags(flags))
}

/// Set the player's state (`fp[-2]`) and where it is (`fp[-3]`).
// D2-00998, D2-00687
fn set_location(c: &mut Call<'_>) -> Result<Status, Stop> {
    if no_receiver(c)? {
        return Ok(Status::Crash);
    }
    let flags = c.cx.at(-2)?;
    let location = string_at(c, -3)?;
    ask_layer(c, 2, |f, _| f.set_location(flags, &location))
}

/// A tell: to a name (`fp[-2]`), its text (`fp[-3]`), its chat flags (`fp[-4]`), with the extra
/// data the teller's name gives.
// D2-00997, D2-00664, D2-00661
fn send_to_user(c: &mut Call<'_>) -> Result<Status, Stop> {
    if no_receiver(c)? {
        return Ok(Status::Crash);
    }
    let user = string_at(c, -2)?;
    let text = string_at(c, -3)?;
    let flags = c.cx.at(-4)?;
    ask_layer(c, 3, |f, h| {
        let extra = ChatExtra::new(&player_name(h), flags);
        f.send_to_user(&user, &text, &extra)
    })
}

/// A call for help: its kind (`fp[-2]`), about a player (`fp[-4]`), with a description
/// (`fp[-5]`); the string at `fp[-3]` is not used.
// D2-00989, D2-00646
fn call_create(c: &mut Call<'_>) -> Result<Status, Stop> {
    let kind = c.cx.at(-2)?;
    let target = string_at(c, -4)?;
    let description = string_at(c, -5)?;
    ask_layer(c, 4, |f, _| f.call_create(kind, &target, &description))
}

/// The player's name as its literal holds it, meta letters kept (empty without a player).
pub(crate) fn player_name(h: &crate::host::ClientHost) -> Vec<u16> {
    h.model
        .player_object()
        .and_then(|o| o.weenie.name.literal.as_ref())
        .map(|l| l.0.clone())
        .unwrap_or_default()
}

/// The world's name (`fp[-2]`), a new string: empty without the service, or before the login
/// named it.
// D2-01551, D2-12134
fn get_world_name(c: &mut Call<'_>) -> Result<Status, Stop> {
    let units = host(c)?
        .friends
        .as_ref()
        .map(|f| f.world_name().to_vec())
        .unwrap_or_default();
    let r =
        c.vm.heap
            .add_native(native_type::WP_STRING, NativeObject::Text(units));
    c.write(-2, r, tag::REFERENCE)?;
    Ok(Status::Success)
}

/// `@` and the world's name (`fp[-2]`), a new string.
// D2-01552, D2-12135
fn get_world_suffix(c: &mut Call<'_>) -> Result<Status, Stop> {
    let units = host(c)?
        .friends
        .as_ref()
        .map(|f| f.world_suffix().to_vec())
        .unwrap_or_default();
    let r =
        c.vm.heap
            .add_native(native_type::WP_STRING, NativeObject::Text(units));
    c.write(-2, r, tag::REFERENCE)?;
    Ok(Status::Success)
}

const NATIVES: &[(u32, u32, NativeFn)] = &[
    (GM_INTERFACE, 0x32, send_to_user),
    (GM_INTERFACE, 0x34, set_state_flags),
    (GM_INTERFACE, 0x35, friend_add),
    (GM_INTERFACE, 0x36, friend_delete),
    (GM_INTERFACE, 0x37, get_friends_list),
    (GM_INTERFACE, 0x38, ignored_add),
    (GM_INTERFACE, 0x39, ignored_delete),
    (GM_INTERFACE, 0x3A, get_ignored_list),
    (GM_INTERFACE, 0x3B, call_create),
    (GM_INTERFACE, 0xF3, set_location),
    (FRIENDS_UTILS, 0x01, get_world_name),
    (FRIENDS_UTILS, 0x02, get_world_suffix),
];

/// The layer's natives: the scripts' requests of the service and the world's name.
pub(crate) fn natives() -> impl Iterator<Item = (NativeId, NativeFn)> {
    NATIVES.iter().map(|(p, n, f)| (NativeId::new(*p, *n), *f))
}
