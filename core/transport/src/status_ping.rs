//! The status ping: a world's live status over UDP on its game port, in two steps, with no
//! session and no account. This is not retail's: it is specified in
//! `docs/networking/06-status-ping.md`.
//!
//! 1. **Hello.** The asker sends a request ([`REQUEST_LEN`] bytes) with no token. The server
//!    answers with a token only ([`TOKEN_REPLY_LEN`] bytes, smaller than the request), so a forged
//!    source address draws nothing larger than it sent.
//! 2. **Ask.** The asker sends the request again carrying that token. A server that issued it to
//!    this address and port, recently, answers with the [`StatusReply`]; otherwise it says nothing.
//!
//! Every datagram starts with a 20-byte header shaped as a packet header's, whose size field
//! ([`IMPOSSIBLE_SIZE`]) is larger than any datagram a server reads: a server that does not know
//! the ping (ACE, GDLE, ClassicACE, or an Empyrean with the ping off) drops it at the size check,
//! before it reads anything else, and creates nothing.
//!
//! The token is the server's own business: the asker only echoes its bytes. This module builds and
//! reads the datagrams; minting and checking tokens, and the sockets, are the caller's.

/// The first dword of every status-ping datagram, in place of a packet's sequence number.
pub const MAGIC: [u8; 4] = *b"ACSP";

/// The header's size field: more than any datagram's payload can be, so every server's packet
/// reader drops the datagram as truncated.
pub const IMPOSSIBLE_SIZE: u16 = 0xFFFF;

/// The header in front of every status-ping datagram.
pub const HEADER_LEN: usize = 20;

/// The version of the request and token forms this module speaks.
pub const VERSION: u8 = 1;

/// The version of the status reply's layout. A later version only appends fields.
pub const REPLY_FORMAT_VERSION: u8 = 1;

/// A token: the window it was issued in, and the server's code for it.
pub const TOKEN_LEN: usize = 4 + TOKEN_MAC_LEN;

/// The server's code in a token.
pub const TOKEN_MAC_LEN: usize = 16;

/// Every request, hello or ask, is padded to this many bytes.
pub const REQUEST_LEN: usize = 160;

/// The token reply: the header, the kind and version, and the token.
pub const TOKEN_REPLY_LEN: usize = HEADER_LEN + 2 + TOKEN_LEN;

/// The most bytes a status reply may take: under any path's MTU.
pub const MAX_REPLY_LEN: usize = 1200;

/// The most bytes of each string in the status reply. A longer one is cut at a character boundary.
pub const MAX_STRING_LEN: usize = 64;

/// What a datagram is, by the byte after the header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Kind {
    /// Asker to server: a token, please.
    Hello = 0x01,
    /// Asker to server: the status, with the token.
    Ask = 0x02,
    /// Server to asker: the token.
    Token = 0x81,
    /// Server to asker: the status.
    Status = 0x82,
}

impl Kind {
    fn from_byte(b: u8) -> Option<Self> {
        [Self::Hello, Self::Ask, Self::Token, Self::Status]
            .into_iter()
            .find(|k| *k as u8 == b)
    }
}

/// A token as the server issued it: the window number and the code over the asker's address,
/// port and that window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Token {
    pub window: u32,
    pub mac: [u8; TOKEN_MAC_LEN],
}

impl Token {
    fn write(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.window.to_le_bytes());
        out.extend_from_slice(&self.mac);
    }

    fn read(b: &[u8]) -> Option<Self> {
        let window = u32::from_le_bytes(b.get(..4)?.try_into().ok()?);
        let mac = b.get(4..TOKEN_LEN)?.try_into().ok()?;
        Some(Self { window, mac })
    }
}

fn header(kind: Kind, version: u8) -> Vec<u8> {
    let mut out = Vec::with_capacity(REQUEST_LEN);
    out.extend_from_slice(&MAGIC);
    // Flags, checksum, recipient id and time: zero.
    out.extend_from_slice(&[0; 12]);
    out.extend_from_slice(&IMPOSSIBLE_SIZE.to_le_bytes());
    // Iteration: zero.
    out.extend_from_slice(&[0; 2]);
    out.push(kind as u8);
    out.push(version);
    out
}

/// Whether `datagram` is shaped as a status-ping datagram: the magic and the impossible size. A
/// server checks this before anything else, so the ping never reaches its packet reader.
#[must_use]
pub fn is_status_ping(datagram: &[u8]) -> bool {
    datagram.len() >= HEADER_LEN + 2
        && datagram[..4] == MAGIC
        && datagram[16..18] == IMPOSSIBLE_SIZE.to_le_bytes()
}

fn kind_of(datagram: &[u8]) -> Option<(Kind, u8)> {
    if !is_status_ping(datagram) {
        return None;
    }
    Some((
        Kind::from_byte(datagram[HEADER_LEN])?,
        datagram[HEADER_LEN + 1],
    ))
}

/// The hello: a request with no token.
#[must_use]
pub fn hello() -> Vec<u8> {
    request(Kind::Hello, Token::default())
}

/// The ask: a request carrying `token`.
#[must_use]
pub fn ask(token: Token) -> Vec<u8> {
    request(Kind::Ask, token)
}

fn request(kind: Kind, token: Token) -> Vec<u8> {
    let mut out = header(kind, VERSION);
    token.write(&mut out);
    out.resize(REQUEST_LEN, 0);
    out
}

/// A request as a server reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Request {
    Hello,
    Ask(Token),
}

/// Reads a request. `None` for anything else, and for a request shorter than [`REQUEST_LEN`]:
/// a server answers only a request at least as large as its answer to a hello.
#[must_use]
pub fn parse_request(datagram: &[u8]) -> Option<Request> {
    if datagram.len() < REQUEST_LEN {
        return None;
    }
    let (kind, _version) = kind_of(datagram)?;
    match kind {
        Kind::Hello => Some(Request::Hello),
        Kind::Ask => Token::read(&datagram[HEADER_LEN + 2..]).map(Request::Ask),
        Kind::Token | Kind::Status => None,
    }
}

/// The server's answer to a hello.
#[must_use]
pub fn token_reply(token: Token) -> Vec<u8> {
    let mut out = header(Kind::Token, VERSION);
    token.write(&mut out);
    out
}

/// The token in a server's answer to a hello.
#[must_use]
pub fn parse_token_reply(datagram: &[u8]) -> Option<Token> {
    match kind_of(datagram)? {
        (Kind::Token, _) => Token::read(&datagram[HEADER_LEN + 2..]),
        _ => None,
    }
}

/// Whether a world is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorldState {
    /// Players can log in.
    Open,
    /// Not open yet, and not going down.
    Starting,
    /// Shutting down.
    ShuttingDown,
    /// A value this build does not know.
    Unknown(u8),
}

impl WorldState {
    fn byte(self) -> u8 {
        match self {
            Self::Open => 1,
            Self::Starting => 2,
            Self::ShuttingDown => 3,
            Self::Unknown(b) => b,
        }
    }

    fn from_byte(b: u8) -> Self {
        match b {
            1 => Self::Open,
            2 => Self::Starting,
            3 => Self::ShuttingDown,
            b => Self::Unknown(b),
        }
    }
}

/// A world's live status: what the answer to an ask carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusReply {
    /// The layout's version, [`REPLY_FORMAT_VERSION`] when written by this build.
    pub format_version: u8,
    pub state: WorldState,
    pub players: u16,
    /// The era the world plays, as `dereth_primitives::EraId::name` spells it.
    pub era: String,
    /// The version of the systems' table [`Self::era_features`] was written against.
    pub era_table_version: u16,
    /// The world's full set of systems, as `dereth_primitives::EraFeatureBits` lays them out.
    pub era_features: Vec<u8>,
    /// The server software (`Empyrean`).
    pub software: String,
    /// Its version.
    pub software_version: String,
    /// The world's name.
    pub world_name: String,
}

fn put_str(out: &mut Vec<u8>, s: &str) {
    let mut end = s.len().min(MAX_STRING_LEN);
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    out.push(u8::try_from(end).unwrap_or(u8::MAX));
    out.extend_from_slice(&s.as_bytes()[..end]);
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

    fn bytes(&mut self) -> Option<Vec<u8>> {
        let n = usize::from(self.u8()?);
        self.take(n).map(<[u8]>::to_vec)
    }

    fn string(&mut self) -> Option<String> {
        self.bytes()
            .map(|b| String::from_utf8_lossy(&b).into_owned())
    }
}

impl StatusReply {
    /// The datagram. Strings are cut to [`MAX_STRING_LEN`] bytes and the bitfield to 255, so it
    /// is always under [`MAX_REPLY_LEN`].
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = header(Kind::Status, self.format_version);
        out.push(self.state.byte());
        out.extend_from_slice(&self.players.to_le_bytes());
        put_str(&mut out, &self.era);
        out.extend_from_slice(&self.era_table_version.to_le_bytes());
        let bits = &self.era_features[..self.era_features.len().min(255)];
        out.push(u8::try_from(bits.len()).unwrap_or(u8::MAX));
        out.extend_from_slice(bits);
        put_str(&mut out, &self.software);
        put_str(&mut out, &self.software_version);
        put_str(&mut out, &self.world_name);
        out
    }

    /// Reads a status reply. Bytes after the fields this build knows are a later version's and are
    /// ignored. `None` for anything else, or one cut short.
    #[must_use]
    pub fn parse(datagram: &[u8]) -> Option<Self> {
        let (Kind::Status, format_version) = kind_of(datagram)? else {
            return None;
        };
        let mut r = Reader {
            b: &datagram[HEADER_LEN + 2..],
        };
        Some(Self {
            format_version,
            state: WorldState::from_byte(r.u8()?),
            players: r.u16()?,
            era: r.string()?,
            era_table_version: r.u16()?,
            era_features: r.bytes()?,
            software: r.string()?,
            software_version: r.string()?,
            world_name: r.string()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply(s: &str) -> StatusReply {
        StatusReply {
            format_version: REPLY_FORMAT_VERSION,
            state: WorldState::Open,
            players: 12,
            era: s.to_owned(),
            era_table_version: 1,
            era_features: vec![0xff, 0xff, 0x5f],
            software: s.to_owned(),
            software_version: s.to_owned(),
            world_name: s.to_owned(),
        }
    }

    #[test]
    fn a_request_is_padded_and_reads_back() {
        assert_eq!(hello().len(), REQUEST_LEN);
        assert_eq!(parse_request(&hello()), Some(Request::Hello));
        let token = Token {
            window: 0x0102_0304,
            mac: [7; TOKEN_MAC_LEN],
        };
        assert_eq!(ask(token).len(), REQUEST_LEN);
        assert_eq!(parse_request(&ask(token)), Some(Request::Ask(token)));
        assert_eq!(parse_request(&hello()[..REQUEST_LEN - 1]), None, "short");
        assert_eq!(parse_request(&token_reply(token)), None, "a reply");
    }

    #[test]
    fn the_answer_to_a_hello_is_smaller_than_the_hello() {
        let reply = token_reply(Token::default());
        assert_eq!(reply.len(), TOKEN_REPLY_LEN);
        assert!(reply.len() <= hello().len());
        assert_eq!(parse_token_reply(&reply), Some(Token::default()));
        assert_eq!(parse_token_reply(&hello()), None);
    }

    /// The header is a packet header whose size field exceeds the datagram, which is what makes
    /// every server's packet reader drop it.
    #[test]
    fn every_datagram_has_the_magic_and_a_size_larger_than_itself() {
        let token = Token::default();
        for d in [hello(), ask(token), token_reply(token), reply("x").encode()] {
            assert!(is_status_ping(&d));
            assert_eq!(d[..4], MAGIC);
            let size = u16::from_le_bytes([d[16], d[17]]);
            assert!(usize::from(size) > d.len() - HEADER_LEN);
            assert_eq!(d[4..8], [0; 4], "no flags");
        }
        assert!(!is_status_ping(b"ACSP"));
        assert!(!is_status_ping(&[0; 40]));
    }

    #[test]
    fn the_status_reply_round_trips_and_ignores_what_a_later_version_appends() {
        let r = reply("Eulmore");
        let mut bytes = r.encode();
        assert_eq!(StatusReply::parse(&bytes), Some(r.clone()));
        bytes.extend_from_slice(&[9, 9, 9]);
        assert_eq!(StatusReply::parse(&bytes), Some(r));
        let cut = &reply("Eulmore").encode()[..40];
        assert_eq!(StatusReply::parse(cut), None);
        assert_eq!(StatusReply::parse(&hello()), None);
    }

    #[test]
    fn the_status_reply_stays_under_its_limit_at_every_cap() {
        let long = "\u{00e9}".repeat(200);
        let mut r = reply(&long);
        r.era_features = vec![0xff; 1000];
        r.players = u16::MAX;
        let bytes = r.encode();
        assert!(bytes.len() <= MAX_REPLY_LEN, "{}", bytes.len());
        let back = StatusReply::parse(&bytes).expect("reads");
        assert!(back.world_name.len() <= MAX_STRING_LEN);
        assert!(long.starts_with(&back.world_name), "cut at a character");
        assert_eq!(back.era_features.len(), 255);
    }

    #[test]
    fn a_state_this_build_does_not_know_is_kept_as_unknown() {
        let mut r = reply("x");
        r.state = WorldState::Unknown(9);
        assert_eq!(
            StatusReply::parse(&r.encode()).map(|r| r.state),
            Some(WorldState::Unknown(9))
        );
        for s in [
            WorldState::Open,
            WorldState::Starting,
            WorldState::ShuttingDown,
        ] {
            assert_eq!(WorldState::from_byte(s.byte()), s);
        }
    }
}
