//! The elements as typed values: the stream's own (the greeting, keep-alive and farewell) and the
//! friends service's requests, answers and events, read from and written as XML.
//!
//! Element and attribute names are the friends service's own where its traffic shows them, and
//! this protocol's for the rest. A reader skips an element it does not know ([`Element::Unknown`])
//! and ignores attributes and children it does not know; a known element missing what it needs, or
//! holding a value out of its bounds, is malformed.

use core::fmt;

use crate::frame::{self, FrameError};
use crate::xml::{self, Node, XmlError};

/// The version of the protocol this crate speaks.
pub const PROTO: u32 = 1;

/// The longest `Software` a greeting names, in characters.
pub const MAX_SOFTWARE: usize = 64;

/// The longest ticket, in characters.
pub const MAX_TICKET: usize = 512;

/// The longest tell, in characters.
pub const MAX_TELL: usize = 1_024;

/// The client's mark for a call, echoed in its answer: `0x` and 1 to 16 hex digits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Cx(pub u64);

impl Cx {
    /// Read `0x` and 1 to 16 hex digits, in either case.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        hex_after_0x(text, 16).map(Self)
    }
}

impl fmt::Display for Cx {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{:x}", self.0)
    }
}

/// A result code, written `0x` and hex digits: [`Hr::OK`] for success, else one of the friends
/// service's own codes, so that the client shows its own message for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Hr(pub u32);

impl Hr {
    /// Done.
    pub const OK: Self = Self(0);
    /// The ticket is refused: the client ends its login, as for a refused password.
    pub const TICKET_REFUSED: Self = Self(0xC112_0806);
    /// The account is banned.
    pub const BANNED: Self = Self(0xC112_1005);
    /// A request before the ticket.
    pub const NOT_TICKETED: Self = Self(0xC112_080B);
    /// A second ticket on the stream.
    pub const ALREADY_TICKETED: Self = Self(0xC112_080C);
    /// Not connected: from the world, the account has no character in the world now; from a
    /// client session, the stream was down when the request was made, or went down before its
    /// answer came.
    pub const NOT_CONNECTED: Self = Self(0xC112_0405);
    /// No such user: the name is no address, or names no character of the world. For a tell, the
    /// client says the name does not exist.
    pub const NO_SUCH_USER: Self = Self(0xC112_0012);
    /// The user is not online now. For a tell, the client says the name is not logged in; it is the
    /// one failure, besides no such user, that the client names the user in.
    pub const NOT_ONLINE: Self = Self(0xC112_0011);
    /// No answer in time, or the user's world cannot be reached. For a tell, the client says the
    /// message could not be sent right now, as for every failure but the two that name the user.
    pub const TIMED_OUT: Self = Self(0xC112_0010);
    /// Denied: a tell held back, a friend who will not be added. For a tell, the client says the
    /// message could not be sent right now.
    pub const DENIED: Self = Self(0xC112_0018);
    /// Not implemented in this version.
    pub const NOT_IMPLEMENTED: Self = Self(0x8000_4001);
    /// A request a client session would not send, because a value is out of its bounds.
    pub const INVALID_ARGUMENT: Self = Self(0x8007_0057);

    /// Read `0x` and 1 to 8 hex digits, in either case.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        hex_after_0x(text, 8)
            .and_then(|v| u32::try_from(v).ok())
            .map(Self)
    }

    /// Whether this is success.
    #[must_use]
    pub const fn is_ok(self) -> bool {
        self.0 == 0
    }
}

impl fmt::Display for Hr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{:x}", self.0)
    }
}

/// `0x` and 1 to `max` hex digits.
fn hex_after_0x(text: &str, max: usize) -> Option<u64> {
    let digits = text
        .strip_prefix("0x")
        .or_else(|| text.strip_prefix("0X"))?;
    if digits.is_empty() || digits.len() > max || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    u64::from_str_radix(digits, 16).ok()
}

/// A decimal number: digits only, no sign.
fn decimal(text: &str) -> Option<u32> {
    if text.is_empty() || text.len() > 10 || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

/// A friend's state: a bit set. Nothing set is offline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Mode(pub u32);

impl Mode {
    /// Logged out.
    pub const OFFLINE: Self = Self(0);
    /// Logged in.
    pub const ONLINE: Self = Self(1);
    /// Away.
    pub const AWAY: Self = Self(2);
    /// Not to be disturbed.
    pub const NOT_DISTURBED: Self = Self(4);

    /// Whether every bit of `other` is set here.
    #[must_use]
    pub const fn has(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Whether the friend is logged in.
    #[must_use]
    pub const fn is_online(self) -> bool {
        self.has(Self::ONLINE)
    }

    /// Both sets of bits.
    #[must_use]
    pub const fn with(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

/// The 16 bytes a client attaches to a tell, written as 32 hex digits; the world carries them to a
/// recipient on the same world and drops them across worlds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Extra(pub [u8; 16]);

impl Extra {
    /// Read 32 hex digits, in either case.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let b = text.as_bytes();
        if b.len() != 32 || !b.iter().all(u8::is_ascii_hexdigit) {
            return None;
        }
        let mut out = [0u8; 16];
        for (byte, i) in out.iter_mut().zip((0..32).step_by(2)) {
            *byte = u8::from_str_radix(text.get(i..i + 2)?, 16).ok()?;
        }
        Some(Self(out))
    }
}

impl fmt::Display for Extra {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for b in self.0 {
            write!(f, "{b:02x}")?;
        }
        Ok(())
    }
}

/// One friend and its state, in a friends list.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FriendStatus {
    /// The friend's address.
    pub user: String,
    pub mode: Mode,
    /// Always empty in this version.
    pub location: String,
}

impl FriendStatus {
    /// A friend in a state, with no location.
    #[must_use]
    pub fn new(user: &str, mode: Mode) -> Self {
        Self {
            user: user.to_owned(),
            mode,
            location: String::new(),
        }
    }
}

/// Why a side ends the stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ByeReason {
    /// The other side broke the protocol (also how an unknown reason reads).
    Protocol,
    /// A newer stream for the same account took this one's place.
    Replaced,
    /// The world will not serve this stream.
    Refused,
    /// Past the bound on requests.
    Flood,
    /// A deadline passed, or nothing was heard for too long.
    Idle,
    /// The world serves as many streams as it will.
    Full,
    /// The sender is stopping.
    Shutdown,
}

impl ByeReason {
    /// Every reason.
    pub const ALL: [Self; 7] = [
        Self::Protocol,
        Self::Replaced,
        Self::Refused,
        Self::Flood,
        Self::Idle,
        Self::Full,
        Self::Shutdown,
    ];

    /// The reason as written.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Protocol => "protocol",
            Self::Replaced => "replaced",
            Self::Refused => "refused",
            Self::Flood => "flood",
            Self::Idle => "idle",
            Self::Full => "full",
            Self::Shutdown => "shutdown",
        }
    }

    /// The reason a text names; one this version does not know reads as [`ByeReason::Protocol`].
    #[must_use]
    pub fn read(text: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|r| r.as_str() == text)
            .unwrap_or(Self::Protocol)
    }
}

/// The client's greeting, the stream's first frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hello {
    /// The version spoken: [`PROTO`].
    pub proto: u32,
    /// The client's name and version: 1 to 64 printable ASCII characters.
    pub software: String,
    /// The client's Windows locale number, when it says.
    pub locale: Option<u32>,
}

/// The world's answer to the greeting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Welcome {
    pub proto: u32,
    /// The world's network slug; empty for a world on no network.
    pub world: String,
    /// The world's name as its login shows it.
    pub name: String,
    /// The keep-alive interval, in seconds.
    pub ping: u32,
}

/// The kinds of request, each answered by one kind of answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Kind {
    PresentTicket,
    GetFriendsList,
    FriendAdd,
    FriendDelete,
    GetIgnoredList,
    IgnoredAdd,
    IgnoredDelete,
    Flags,
    SetLoc,
    SendToUserEx,
    CallCreate,
    SendToRoomEx,
}

impl Kind {
    /// Every kind.
    pub const ALL: [Self; 12] = [
        Self::PresentTicket,
        Self::GetFriendsList,
        Self::FriendAdd,
        Self::FriendDelete,
        Self::GetIgnoredList,
        Self::IgnoredAdd,
        Self::IgnoredDelete,
        Self::Flags,
        Self::SetLoc,
        Self::SendToUserEx,
        Self::CallCreate,
        Self::SendToRoomEx,
    ];

    /// The request's element name.
    #[must_use]
    pub const fn request_name(self) -> &'static str {
        match self {
            Self::PresentTicket => "PresentTicketCmd",
            Self::GetFriendsList => "GetFriendsList",
            Self::FriendAdd => "FriendAdd",
            Self::FriendDelete => "FriendDelete",
            Self::GetIgnoredList => "GetIgnoredList",
            Self::IgnoredAdd => "IgnoredAdd",
            Self::IgnoredDelete => "IgnoredDelete",
            Self::Flags => "Flags",
            Self::SetLoc => "SetLoc",
            Self::SendToUserEx => "SendToUserEx",
            Self::CallCreate => "CallCreate",
            Self::SendToRoomEx => "SendToRoomEx",
        }
    }

    /// The answer's element name.
    #[must_use]
    pub const fn answer_name(self) -> &'static str {
        match self {
            Self::PresentTicket => "PresentTicketRes",
            Self::GetFriendsList => "GetFriendsListRes",
            Self::FriendAdd => "FriendAddRes",
            Self::FriendDelete => "FriendDeleteRes",
            Self::GetIgnoredList => "GetIgnoredListRes",
            Self::IgnoredAdd => "IgnoredAddRes",
            Self::IgnoredDelete => "IgnoredDeleteRes",
            Self::Flags => "FlagsRes",
            Self::SetLoc => "SetLocRes",
            Self::SendToUserEx => "SendToUserExRes",
            Self::CallCreate => "CallCreateRes",
            Self::SendToRoomEx => "SendToRoomExRes",
        }
    }
}

/// A request, client to world. Each carries a [`Cx`] beside it in [`Element::Request`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// Present the ticket. `email` is ignored.
    PresentTicket {
        email: String,
        ticket: String,
    },
    GetFriendsList,
    FriendAdd {
        user: String,
    },
    FriendDelete {
        user: String,
    },
    GetIgnoredList,
    IgnoredAdd {
        user: String,
    },
    IgnoredDelete {
        user: String,
    },
    /// The client's start-of-session state flags; the world echoes `word` and `flags`.
    Flags {
        op: u32,
        word: u32,
        flags: u32,
    },
    /// The character's state: `flags` 1 available, 2 away, 4 not to be disturbed. `location` is
    /// ignored.
    SetLoc {
        flags: u32,
        location: String,
    },
    /// A tell: 1 to 1,024 characters.
    SendToUserEx {
        user: String,
        text: String,
        extra: Option<Extra>,
    },
    /// Reserved: not served in this version.
    CallCreate,
    /// Reserved: rooms are the game's chat rooms, never this service's.
    SendToRoomEx,
}

impl Request {
    /// What kind of request this is.
    #[must_use]
    pub const fn kind(&self) -> Kind {
        match self {
            Self::PresentTicket { .. } => Kind::PresentTicket,
            Self::GetFriendsList => Kind::GetFriendsList,
            Self::FriendAdd { .. } => Kind::FriendAdd,
            Self::FriendDelete { .. } => Kind::FriendDelete,
            Self::GetIgnoredList => Kind::GetIgnoredList,
            Self::IgnoredAdd { .. } => Kind::IgnoredAdd,
            Self::IgnoredDelete { .. } => Kind::IgnoredDelete,
            Self::Flags { .. } => Kind::Flags,
            Self::SetLoc { .. } => Kind::SetLoc,
            Self::SendToUserEx { .. } => Kind::SendToUserEx,
            Self::CallCreate => Kind::CallCreate,
            Self::SendToRoomEx => Kind::SendToRoomEx,
        }
    }
}

/// An answer, world to client. Each carries the request's [`Cx`] and an [`Hr`] beside it in
/// [`Element::Answer`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    PresentTicket {
        status: u32,
    },
    GetFriendsList {
        friends: Vec<FriendStatus>,
    },
    FriendAdd,
    FriendDelete,
    /// The ignored addresses.
    GetIgnoredList {
        ignored: Vec<String>,
    },
    IgnoredAdd,
    IgnoredDelete,
    Flags {
        which_flag: u32,
        flags: u32,
    },
    SetLoc,
    SendToUserEx,
    CallCreate,
    SendToRoomEx,
}

impl Answer {
    /// What kind of request this answers.
    #[must_use]
    pub const fn kind(&self) -> Kind {
        match self {
            Self::PresentTicket { .. } => Kind::PresentTicket,
            Self::GetFriendsList { .. } => Kind::GetFriendsList,
            Self::FriendAdd => Kind::FriendAdd,
            Self::FriendDelete => Kind::FriendDelete,
            Self::GetIgnoredList { .. } => Kind::GetIgnoredList,
            Self::IgnoredAdd => Kind::IgnoredAdd,
            Self::IgnoredDelete => Kind::IgnoredDelete,
            Self::Flags { .. } => Kind::Flags,
            Self::SetLoc => Kind::SetLoc,
            Self::SendToUserEx => Kind::SendToUserEx,
            Self::CallCreate => Kind::CallCreate,
            Self::SendToRoomEx => Kind::SendToRoomEx,
        }
    }

    /// The answer of a kind that carries nothing: what goes with a failure code.
    #[must_use]
    pub const fn empty(kind: Kind) -> Self {
        match kind {
            Kind::PresentTicket => Self::PresentTicket { status: 0 },
            Kind::GetFriendsList => Self::GetFriendsList {
                friends: Vec::new(),
            },
            Kind::FriendAdd => Self::FriendAdd,
            Kind::FriendDelete => Self::FriendDelete,
            Kind::GetIgnoredList => Self::GetIgnoredList {
                ignored: Vec::new(),
            },
            Kind::IgnoredAdd => Self::IgnoredAdd,
            Kind::IgnoredDelete => Self::IgnoredDelete,
            Kind::Flags => Self::Flags {
                which_flag: 0,
                flags: 0,
            },
            Kind::SetLoc => Self::SetLoc,
            Kind::SendToUserEx => Self::SendToUserEx,
            Kind::CallCreate => Self::CallCreate,
            Kind::SendToRoomEx => Self::SendToRoomEx,
        }
    }
}

/// An event, world to client, unasked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// The character entered the world, or the stream came back while it is in: every friend with
    /// its state.
    FStart { friends: Vec<FriendStatus> },
    /// One friend's state changed, or a friend was just added.
    FriendStat { user: String, mode: Mode },
    /// A tell to the character; `user` is the teller.
    UTellEv {
        user: String,
        text: String,
        extra: Option<Extra>,
    },
}

/// One element of the stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Element {
    Hello(Hello),
    Welcome(Welcome),
    /// Sent after the keep-alive interval of not sending; answered with [`Element::Pong`].
    Ping {
        seq: u32,
    },
    Pong {
        seq: u32,
    },
    /// The sender is closing the stream, and says why.
    Bye {
        reason: ByeReason,
    },
    Request {
        cx: Cx,
        request: Request,
    },
    Answer {
        cx: Cx,
        hr: Hr,
        answer: Answer,
    },
    Event(Event),
    /// An element this version does not know: a reader skips it.
    Unknown {
        name: String,
    },
}

/// Why a frame's text is not an element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElementError {
    /// The text is not one element of the XML the stream carries.
    Xml(XmlError),
    /// A known element lacks what it needs or holds a value out of bounds.
    Malformed { element: String, what: &'static str },
}

impl fmt::Display for ElementError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Xml(e) => write!(f, "not an element: {e}"),
            Self::Malformed { element, what } => write!(f, "a malformed {element}: {what}"),
        }
    }
}

impl From<XmlError> for ElementError {
    fn from(e: XmlError) -> Self {
        Self::Xml(e)
    }
}

/// Reads the attributes of one node, naming the node in what it refuses.
struct Reader<'a> {
    node: &'a Node,
}

impl<'a> Reader<'a> {
    fn bad(&self, what: &'static str) -> ElementError {
        ElementError::Malformed {
            element: self.node.name.clone(),
            what,
        }
    }

    fn text(&self, name: &'static str, missing: &'static str) -> Result<&'a str, ElementError> {
        self.node.get(name).ok_or_else(|| self.bad(missing))
    }

    fn text_or_empty(&self, name: &str) -> String {
        self.node.get(name).unwrap_or_default().to_owned()
    }

    fn number(&self, name: &'static str, bad: &'static str) -> Result<u32, ElementError> {
        self.node
            .get(name)
            .and_then(decimal)
            .ok_or_else(|| self.bad(bad))
    }

    fn cx(&self) -> Result<Cx, ElementError> {
        self.node
            .get("Cx")
            .and_then(Cx::parse)
            .ok_or_else(|| self.bad("no Cx of 0x and 1 to 16 hex digits"))
    }

    fn hr(&self) -> Result<Hr, ElementError> {
        self.node
            .get("HR")
            .and_then(Hr::parse)
            .ok_or_else(|| self.bad("no HR of 0x and 1 to 8 hex digits"))
    }

    fn user(&self) -> Result<String, ElementError> {
        self.text("User", "no User").map(str::to_owned)
    }

    fn tell(&self) -> Result<String, ElementError> {
        let text = self.text("Text", "no Text")?;
        let n = text.chars().count();
        if n == 0 || n > MAX_TELL {
            return Err(self.bad("a Text not of 1 to 1,024 characters"));
        }
        Ok(text.to_owned())
    }

    fn extra(&self) -> Result<Option<Extra>, ElementError> {
        match self.node.children.iter().find(|c| c.name == "Extra") {
            None => Ok(None),
            Some(c) => c
                .get("Val")
                .and_then(Extra::parse)
                .map(Some)
                .ok_or_else(|| self.bad("an Extra not of 32 hex digits")),
        }
    }

    fn friends(&self) -> Result<Vec<FriendStatus>, ElementError> {
        self.node
            .children
            .iter()
            .filter(|c| c.name == "FriendStatus")
            .map(|c| {
                let r = Reader { node: c };
                Ok(FriendStatus {
                    user: r.user()?,
                    mode: Mode(r.number("Mode", "no Mode of digits")?),
                    location: r.text_or_empty("Location"),
                })
            })
            .collect()
    }
}

/// Whether `text` is 1 to `max` printable ASCII characters.
fn printable(text: &str, max: usize) -> bool {
    !text.is_empty() && text.len() <= max && text.bytes().all(|b| (0x20..=0x7E).contains(&b))
}

impl Element {
    /// Read one frame's text.
    ///
    /// # Errors
    /// [`ElementError`] for text that is not one element of the subset, or a known element that
    /// is malformed.
    pub fn parse(text: &str) -> Result<Self, ElementError> {
        Self::from_node(&xml::parse(text)?)
    }

    /// Read one element.
    ///
    /// # Errors
    /// [`ElementError::Malformed`] for a known element that lacks what it needs or holds a value
    /// out of bounds.
    pub fn from_node(node: &Node) -> Result<Self, ElementError> {
        let r = Reader { node };
        let name = node.name.as_str();
        if let Some(kind) = Kind::ALL.into_iter().find(|k| k.request_name() == name) {
            let cx = r.cx()?;
            let request = Self::request(&r, kind)?;
            return Ok(Self::Request { cx, request });
        }
        if let Some(kind) = Kind::ALL.into_iter().find(|k| k.answer_name() == name) {
            let (cx, hr) = (r.cx()?, r.hr()?);
            let answer = Self::answer(&r, kind)?;
            return Ok(Self::Answer { cx, hr, answer });
        }
        Ok(match name {
            "Hello" => {
                let software = r.text("Software", "no Software")?;
                if !printable(software, MAX_SOFTWARE) {
                    return Err(r.bad("a Software not of 1 to 64 printable ASCII characters"));
                }
                let locale = match node.get("Locale") {
                    None => None,
                    Some(_) => Some(r.number("Locale", "a Locale not of digits")?),
                };
                Self::Hello(Hello {
                    proto: r.number("Proto", "no Proto of digits")?,
                    software: software.to_owned(),
                    locale,
                })
            }
            "Welcome" => {
                let ping = r.number("Ping", "no Ping of digits")?;
                if ping == 0 {
                    return Err(r.bad("a Ping of no seconds"));
                }
                Self::Welcome(Welcome {
                    proto: r.number("Proto", "no Proto of digits")?,
                    world: r.text("World", "no World")?.to_owned(),
                    name: r.text("Name", "no Name")?.to_owned(),
                    ping,
                })
            }
            "Ping" => Self::Ping {
                seq: r.number("Seq", "no Seq of digits")?,
            },
            "Pong" => Self::Pong {
                seq: r.number("Seq", "no Seq of digits")?,
            },
            "Bye" => Self::Bye {
                reason: ByeReason::read(node.get("Reason").unwrap_or_default()),
            },
            "FStart" => Self::Event(Event::FStart {
                friends: r.friends()?,
            }),
            "FriendStat" => Self::Event(Event::FriendStat {
                user: r.user()?,
                mode: Mode(r.number("Mode", "no Mode of digits")?),
            }),
            "UTellEv" => Self::Event(Event::UTellEv {
                user: r.user()?,
                text: r.tell()?,
                extra: r.extra()?,
            }),
            _ => Self::Unknown {
                name: name.to_owned(),
            },
        })
    }

    fn request(r: &Reader<'_>, kind: Kind) -> Result<Request, ElementError> {
        Ok(match kind {
            Kind::PresentTicket => {
                let ticket = r
                    .node
                    .children
                    .iter()
                    .find(|c| c.name == "Ticket")
                    .and_then(|c| c.get("Val"))
                    .ok_or_else(|| r.bad("no Ticket"))?;
                if !printable(ticket, MAX_TICKET) {
                    return Err(r.bad("a Ticket not of 1 to 512 printable ASCII characters"));
                }
                Request::PresentTicket {
                    email: r.text_or_empty("Email"),
                    ticket: ticket.to_owned(),
                }
            }
            Kind::GetFriendsList => Request::GetFriendsList,
            Kind::FriendAdd => Request::FriendAdd { user: r.user()? },
            Kind::FriendDelete => Request::FriendDelete { user: r.user()? },
            Kind::GetIgnoredList => Request::GetIgnoredList,
            Kind::IgnoredAdd => Request::IgnoredAdd { user: r.user()? },
            Kind::IgnoredDelete => Request::IgnoredDelete { user: r.user()? },
            Kind::Flags => Request::Flags {
                op: r.number("Op", "no Op of digits")?,
                word: r.number("Word", "no Word of digits")?,
                flags: r.number("Flags", "no Flags of digits")?,
            },
            Kind::SetLoc => Request::SetLoc {
                flags: r.number("Flags", "no Flags of digits")?,
                location: r.text_or_empty("Location"),
            },
            Kind::SendToUserEx => Request::SendToUserEx {
                user: r.user()?,
                text: r.tell()?,
                extra: r.extra()?,
            },
            Kind::CallCreate => Request::CallCreate,
            Kind::SendToRoomEx => Request::SendToRoomEx,
        })
    }

    fn answer(r: &Reader<'_>, kind: Kind) -> Result<Answer, ElementError> {
        Ok(match kind {
            Kind::PresentTicket => Answer::PresentTicket {
                status: r.number("Status", "no Status of digits")?,
            },
            Kind::GetFriendsList => Answer::GetFriendsList {
                friends: r.friends()?,
            },
            Kind::GetIgnoredList => Answer::GetIgnoredList {
                ignored: r
                    .node
                    .children
                    .iter()
                    .filter(|c| c.name == "Jerk")
                    .map(|c| {
                        c.get("Str")
                            .map(str::to_owned)
                            .ok_or_else(|| r.bad("a Jerk with no Str"))
                    })
                    .collect::<Result<_, _>>()?,
            },
            Kind::Flags => Answer::Flags {
                which_flag: r.number("WhichFlag", "no WhichFlag of digits")?,
                flags: r.number("Flags", "no Flags of digits")?,
            },
            other => Answer::empty(other),
        })
    }

    /// The element's name as written.
    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            Self::Hello(_) => "Hello",
            Self::Welcome(_) => "Welcome",
            Self::Ping { .. } => "Ping",
            Self::Pong { .. } => "Pong",
            Self::Bye { .. } => "Bye",
            Self::Request { request, .. } => request.kind().request_name(),
            Self::Answer { answer, .. } => answer.kind().answer_name(),
            Self::Event(Event::FStart { .. }) => "FStart",
            Self::Event(Event::FriendStat { .. }) => "FriendStat",
            Self::Event(Event::UTellEv { .. }) => "UTellEv",
            Self::Unknown { name } => name,
        }
    }

    /// The element as a node, attributes in the order this crate writes them.
    #[must_use]
    pub fn to_node(&self) -> Node {
        let n = Node::new(self.name());
        match self {
            Self::Hello(h) => {
                let n = n
                    .attr("Proto", h.proto.to_string())
                    .attr("Software", h.software.clone());
                match h.locale {
                    Some(l) => n.attr("Locale", l.to_string()),
                    None => n,
                }
            }
            Self::Welcome(w) => n
                .attr("Proto", w.proto.to_string())
                .attr("World", w.world.clone())
                .attr("Name", w.name.clone())
                .attr("Ping", w.ping.to_string()),
            Self::Ping { seq } | Self::Pong { seq } => n.attr("Seq", seq.to_string()),
            Self::Bye { reason } => n.attr("Reason", reason.as_str()),
            Self::Request { cx, request } => request_node(n.attr("Cx", cx.to_string()), request),
            Self::Answer { cx, hr, answer } => answer_node(
                n.attr("Cx", cx.to_string()).attr("HR", hr.to_string()),
                answer,
            ),
            Self::Event(Event::FStart { friends }) => friends_node(n, friends),
            Self::Event(Event::FriendStat { user, mode }) => n
                .attr("User", user.clone())
                .attr("Mode", mode.0.to_string()),
            Self::Event(Event::UTellEv { user, text, extra }) => with_extra(
                n.attr("User", user.clone()).attr("Text", text.clone()),
                *extra,
            ),
            Self::Unknown { .. } => n,
        }
    }

    /// The element as text.
    #[must_use]
    pub fn to_xml(&self) -> String {
        self.to_node().write()
    }

    /// Append the element as one frame.
    ///
    /// # Errors
    /// [`FrameError::TooLong`] for an element past the frame's bound; `out` is unchanged.
    pub fn write_frame(&self, out: &mut Vec<u8>) -> Result<(), FrameError> {
        frame::write_frame(&self.to_xml(), out)
    }

    /// What in the element is out of its bounds, if anything: what a reader would refuse.
    #[must_use]
    pub fn out_of_bounds(&self) -> Option<&'static str> {
        let tell = |t: &str| {
            let n = t.chars().count();
            (n == 0 || n > MAX_TELL).then_some("a tell not of 1 to 1,024 characters")
        };
        match self {
            Self::Hello(h) if !printable(&h.software, MAX_SOFTWARE) => {
                Some("a Software not of 1 to 64 printable ASCII characters")
            }
            Self::Welcome(w) if w.ping == 0 => Some("a Ping of no seconds"),
            Self::Request {
                request: Request::PresentTicket { ticket, .. },
                ..
            } if !printable(ticket, MAX_TICKET) => {
                Some("a ticket not of 1 to 512 printable ASCII characters")
            }
            Self::Request {
                request: Request::SendToUserEx { text, .. },
                ..
            }
            | Self::Event(Event::UTellEv { text, .. }) => tell(text),
            _ => None,
        }
    }
}

fn request_node(n: Node, request: &Request) -> Node {
    match request {
        Request::PresentTicket { email, ticket } => n
            .attr("Email", email.clone())
            .child(Node::new("Ticket").attr("Val", ticket.clone())),
        Request::GetFriendsList
        | Request::GetIgnoredList
        | Request::CallCreate
        | Request::SendToRoomEx => n,
        Request::FriendAdd { user }
        | Request::FriendDelete { user }
        | Request::IgnoredAdd { user }
        | Request::IgnoredDelete { user } => n.attr("User", user.clone()),
        Request::Flags { op, word, flags } => n
            .attr("Op", op.to_string())
            .attr("Word", word.to_string())
            .attr("Flags", flags.to_string()),
        Request::SetLoc { flags, location } => n
            .attr("Flags", flags.to_string())
            .attr("Location", location.clone()),
        Request::SendToUserEx { user, text, extra } => with_extra(
            n.attr("User", user.clone()).attr("Text", text.clone()),
            *extra,
        ),
    }
}

fn answer_node(n: Node, answer: &Answer) -> Node {
    match answer {
        Answer::PresentTicket { status } => n.attr("Status", status.to_string()),
        Answer::GetFriendsList { friends } => friends_node(n, friends),
        Answer::GetIgnoredList { ignored } => ignored
            .iter()
            .fold(n.attr("Cnt", ignored.len().to_string()), |n, who| {
                n.child(Node::new("Jerk").attr("Str", who.clone()))
            }),
        Answer::Flags { which_flag, flags } => n
            .attr("WhichFlag", which_flag.to_string())
            .attr("Flags", flags.to_string()),
        _ => n,
    }
}

fn friends_node(n: Node, friends: &[FriendStatus]) -> Node {
    friends
        .iter()
        .fold(n.attr("Cnt", friends.len().to_string()), |n, f| {
            n.child(
                Node::new("FriendStatus")
                    .attr("User", f.user.clone())
                    .attr("Mode", f.mode.0.to_string())
                    .attr("Location", f.location.clone()),
            )
        })
}

fn with_extra(n: Node, extra: Option<Extra>) -> Node {
    match extra {
        Some(e) => n.child(Node::new("Extra").attr("Val", e.to_string())),
        None => n,
    }
}
