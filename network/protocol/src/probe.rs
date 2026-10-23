//! The probe: the datagrams the hub sends a world's game port when the world's host asks the hub
//! to try it from outside (`docs/network/05-hub.md` section 1, "Trying a world from outside"),
//! and the answers it reads.
//!
//! Two questions go to the game port together, since the hub does not know what runs there:
//!
//! - **The status ping** (`docs/networking/06-status-ping.md`), which Empyrean answers: a hello is
//!   answered with a token, and an ask carrying the token with the world's live status (its state,
//!   players, software and its version, and its name). Every other server drops both datagrams
//!   unread, since their header's size field is larger than any datagram.
//! - **The server-tracker login**: the account `acservertracker:jj9h26hcsggc` with no password,
//!   one fixed datagram, which ACE, GDLE and Empyrean all answer without making an account. Any
//!   well-formed packet back means the world is there; it says nothing more.
//!
//! This module builds the datagrams and reads the answers; the socket and the waiting are the
//! caller's.

/// The first dword of every status-ping datagram, in place of a packet's sequence number.
pub const STATUS_MAGIC: [u8; 4] = *b"ACSP";

/// The header's size field in a status-ping datagram: more than any datagram's payload can be.
const IMPOSSIBLE_SIZE: [u8; 2] = [0xFF, 0xFF];

/// The packet header in front of every datagram.
pub const HEADER_LEN: usize = 20;

/// A status-ping request, hello or ask, is padded to this many bytes.
pub const REQUEST_LEN: usize = 160;

/// The token a server gives in answer to a hello: its window and its code.
pub const TOKEN_LEN: usize = 20;

/// The most bytes of a status answer the hub reads: under any path's MTU.
pub const MAX_ANSWER_LEN: usize = 1200;

const HELLO: u8 = 0x01;
const ASK: u8 = 0x02;
const TOKEN: u8 = 0x81;
const STATUS: u8 = 0x82;

/// The version of the request and token forms this module speaks.
const VERSION: u8 = 1;

/// The server-tracker login, whole: a login request for `acservertracker:jj9h26hcsggc` with no
/// password, unsequenced and in plain text, as the first packet of any login is.
pub const TRACKER_LOGIN: [u8; 84] = [
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x55, 0x48, 0x27, 0xad, 0x00, 0x00, 0x00, 0x00,
    0x40, 0x00, 0x00, 0x00, 0x04, 0x00, 0x31, 0x38, 0x30, 0x32, 0x00, 0x00, 0x34, 0x00, 0x00, 0x00,
    0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1c, 0x00, 0x61, 0x63,
    0x73, 0x65, 0x72, 0x76, 0x65, 0x72, 0x74, 0x72, 0x61, 0x63, 0x6b, 0x65, 0x72, 0x3a, 0x6a, 0x6a,
    0x39, 0x68, 0x32, 0x36, 0x68, 0x63, 0x73, 0x67, 0x67, 0x63, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00,
];

fn status_header(kind: u8, version: u8) -> Vec<u8> {
    let mut out = Vec::with_capacity(REQUEST_LEN);
    out.extend_from_slice(&STATUS_MAGIC);
    // Flags, checksum, recipient id and time: zero.
    out.extend_from_slice(&[0; 12]);
    out.extend_from_slice(&IMPOSSIBLE_SIZE);
    // Iteration: zero.
    out.extend_from_slice(&[0; 2]);
    out.push(kind);
    out.push(version);
    out
}

fn request(kind: u8, token: &[u8; TOKEN_LEN]) -> Vec<u8> {
    let mut out = status_header(kind, VERSION);
    out.extend_from_slice(token);
    out.resize(REQUEST_LEN, 0);
    out
}

/// The status ping's first step: a hello, with no token.
#[must_use]
pub fn status_hello() -> Vec<u8> {
    request(HELLO, &[0; TOKEN_LEN])
}

/// The status ping's second step: the ask, carrying the token a server gave.
#[must_use]
pub fn status_ask(token: &[u8; TOKEN_LEN]) -> Vec<u8> {
    request(ASK, token)
}

/// The kind and version of a status-ping datagram; `None` for anything else.
fn status_kind(datagram: &[u8]) -> Option<(u8, u8)> {
    (datagram.len() >= HEADER_LEN + 2
        && datagram[..4] == STATUS_MAGIC
        && datagram[16..18] == IMPOSSIBLE_SIZE)
        .then(|| (datagram[HEADER_LEN], datagram[HEADER_LEN + 1]))
}

/// The token in a server's answer to a hello; `None` when `datagram` is not that answer.
#[must_use]
pub fn status_token(datagram: &[u8]) -> Option<[u8; TOKEN_LEN]> {
    match status_kind(datagram)? {
        (TOKEN, _) => datagram
            .get(HEADER_LEN + 2..HEADER_LEN + 2 + TOKEN_LEN)?
            .try_into()
            .ok(),
        _ => None,
    }
}

/// Whether `datagram` is one of the status ping's requests, hello or ask, as a server reads it.
/// `Some(None)` for a hello, `Some(Some(token))` for an ask, `None` for anything else.
#[must_use]
pub fn status_request(datagram: &[u8]) -> Option<Option<[u8; TOKEN_LEN]>> {
    if datagram.len() < REQUEST_LEN {
        return None;
    }
    let token: [u8; TOKEN_LEN] = datagram
        .get(HEADER_LEN + 2..HEADER_LEN + 2 + TOKEN_LEN)?
        .try_into()
        .ok()?;
    match status_kind(datagram)? {
        (HELLO, _) => Some(None),
        (ASK, _) => Some(Some(token)),
        _ => None,
    }
}

/// A server's answer to a hello: the header and the token, smaller than the hello.
#[must_use]
pub fn status_token_reply(token: &[u8; TOKEN_LEN]) -> Vec<u8> {
    let mut out = status_header(TOKEN, VERSION);
    out.extend_from_slice(token);
    out
}

/// What a world says of itself in answer to the status ping's ask.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    /// `1` open, `2` starting, `3` shutting down; another value is a later version's.
    pub state: u8,
    pub players: u16,
    /// The server software, as it names itself.
    pub software: String,
    pub version: String,
    /// The world's name.
    pub name: String,
}

struct Reader<'a> {
    b: &'a [u8],
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Option<&[u8]> {
        if self.b.len() < n {
            return None;
        }
        let (head, rest) = self.b.split_at(n);
        self.b = rest;
        Some(head)
    }

    fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|b| b[0])
    }

    fn u16(&mut self) -> Option<u16> {
        self.take(2).map(|b| u16::from_le_bytes([b[0], b[1]]))
    }

    fn bytes(&mut self) -> Option<&[u8]> {
        let n = usize::from(self.u8()?);
        self.take(n)
    }

    fn string(&mut self) -> Option<String> {
        self.bytes()
            .map(|b| String::from_utf8_lossy(b).into_owned())
    }
}

fn put_str(out: &mut Vec<u8>, s: &str) {
    let mut end = s.len().min(64);
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    out.push(u8::try_from(end).unwrap_or(u8::MAX));
    out.extend_from_slice(&s.as_bytes()[..end]);
}

impl Status {
    /// Reads the answer to an ask: the state, the players, the era and its systems (skipped), the
    /// software, its version and the world's name. Bytes after them are a later version's and are
    /// ignored. `None` for anything else, for one cut short, and for one over
    /// [`MAX_ANSWER_LEN`].
    #[must_use]
    pub fn parse(datagram: &[u8]) -> Option<Self> {
        if datagram.len() > MAX_ANSWER_LEN {
            return None;
        }
        let (STATUS, _) = status_kind(datagram)? else {
            return None;
        };
        let mut r = Reader {
            b: &datagram[HEADER_LEN + 2..],
        };
        let state = r.u8()?;
        let players = r.u16()?;
        let _era = r.bytes()?;
        let _table = r.u16()?;
        let _systems = r.bytes()?;
        Some(Self {
            state,
            players,
            software: r.string()?,
            version: r.string()?,
            name: r.string()?,
        })
    }

    /// The answer as a world writes it, with no era named and no systems: for a stand-in world.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = status_header(STATUS, 1);
        out.push(self.state);
        out.extend_from_slice(&self.players.to_le_bytes());
        put_str(&mut out, "");
        out.extend_from_slice(&0u16.to_le_bytes());
        out.push(0);
        put_str(&mut out, &self.software);
        put_str(&mut out, &self.version);
        put_str(&mut out, &self.name);
        out
    }
}

/// Whether `datagram` is a packet a server sent: a header whose size field is exactly what follows
/// it, and not one of the status ping's own datagrams. Any such answer to the server-tracker login
/// means a server is there.
#[must_use]
pub fn is_packet(datagram: &[u8]) -> bool {
    datagram.len() >= HEADER_LEN
        && status_kind(datagram).is_none()
        && usize::from(u16::from_le_bytes([datagram[16], datagram[17]]))
            == datagram.len() - HEADER_LEN
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Spec: hub: the probe's status-ping requests are padded to 160 bytes and carry the token they
    /// were given; a server's token answer is smaller than the hello.
    #[test]
    fn the_status_pings_requests_carry_the_token_they_were_given() {
        let hello = status_hello();
        assert_eq!(hello.len(), REQUEST_LEN);
        assert_eq!(status_request(&hello), Some(None));
        let token = [7u8; TOKEN_LEN];
        let reply = status_token_reply(&token);
        assert!(reply.len() < hello.len(), "nothing larger than the hello");
        assert_eq!(status_token(&reply), Some(token));
        assert_eq!(status_token(&hello), None, "a hello is no token");
        let ask = status_ask(&token);
        assert_eq!(ask.len(), REQUEST_LEN);
        assert_eq!(status_request(&ask), Some(Some(token)));
        assert_eq!(status_request(&hello[..REQUEST_LEN - 1]), None, "short");
        for d in [&hello, &ask, &reply] {
            assert_eq!(d[16..18], [0xFF, 0xFF], "the impossible size");
            assert!(!is_packet(d), "a status-ping datagram is no packet");
        }
    }

    /// Spec: hub: a world's status answer is read for its state, players, software, version and
    /// name, a later version's bytes after them ignored, and one cut short refused.
    #[test]
    fn a_status_answer_reads_back_and_ignores_what_a_later_version_appends() {
        let status = Status {
            state: 1,
            players: 3,
            software: "Empyrean".into(),
            version: "0.3.3".into(),
            name: "Stand-in".into(),
        };
        let mut bytes = status.encode();
        assert_eq!(Status::parse(&bytes), Some(status.clone()));
        bytes.extend_from_slice(&[9, 9, 9]);
        assert_eq!(Status::parse(&bytes), Some(status));
        assert_eq!(Status::parse(&bytes[..30]), None, "cut short");
        assert_eq!(Status::parse(&status_hello()), None);
        let mut long = bytes.clone();
        long.resize(MAX_ANSWER_LEN + 1, 0);
        assert_eq!(Status::parse(&long), None, "over the limit");
    }

    /// Spec: hub: the server-tracker login is one well-formed login packet for the tracker account
    /// with no password; any well-formed packet back is an answer.
    #[test]
    fn the_tracker_login_is_one_packet_for_the_tracker_account() {
        assert!(is_packet(&TRACKER_LOGIN));
        let text = String::from_utf8_lossy(&TRACKER_LOGIN);
        assert!(text.contains("acservertracker:jj9h26hcsggc"), "{text}");
        assert!(!is_packet(b"hello"));
        assert!(!is_packet(&[0; 19]));
        let mut wrong_size = TRACKER_LOGIN.to_vec();
        wrong_size.push(0);
        assert!(!is_packet(&wrong_size), "its size must be what follows");
    }
}
