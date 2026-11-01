//! What the library hands the client, as the client reads it: one block of memory for each result
//! or event, with 32-bit pointers, built here with no pointer of its own so that any host can
//! check it.
//!
//! **Every block** starts with the same header:
//!
//! | offset | size | what |
//! |---|---|---|
//! | `+0x00` | 4 | the library's own (zero) |
//! | `+0x04` | 1 | 1: the block is valid (the client reads nothing from a block without it) |
//! | `+0x08` | 4 | the type: `0x?2??` a result, `0x?3??` an event; the bits `0xFF000` its family |
//! | `+0x0C` | 16 | the library's own (zero) |
//! | `+0x1C` | | a result's context, then its code at `+0x20` and its fields; an event's fields |
//!
//! **The fields** ([`block`]):
//!
//! | type | what | `+0x1C` | `+0x20` | `+0x24` | `+0x28` |
//! |---|---|---|---|---|---|
//! | `0x1300` `0x1301` `0x1302` | connected, disconnected, lost | | | | |
//! | `0x430C` | the friends, at the character's entry | count | entries | | |
//! | `0x430D` | a friend's state | name | state | | |
//! | `0x5317` | a tell | teller | 0 | text | extra data, never NULL |
//! | `0x4200` | the friends list | context | code | count | entries |
//! | `0x4203` | the ignore list | context | code | count | names |
//! | `0x4208` | the state flags | context | code | which | flags |
//! | `0xF201` | the ticket's answer | context | code | status | |
//! | `0x10200` | a help call's answer | context | code | queue | place |
//! | the other results | | context | code | | |
//!
//! A friends entry is 12 bytes: the name, the state, the location (an empty text). A name list is
//! an array of pointers. Extra data is 8 bytes, its size and a pointer to its bytes; with none,
//! both are zero. Every text is UTF-16, ending in a NUL, with no length before it. The block holds
//! everything it points to, so freeing it frees all of it.

use stream::{Answer, Event, Extra, FriendStatus, Kind};

use crate::engine::{Message, Reply};

/// The size of the header: where a block's fields start.
pub const HEADER: usize = 0x1C;

/// The connect events.
pub const CONNECTED: u32 = 0x1300;
pub const DISCONNECTED: u32 = 0x1301;
pub const LOST: u32 = 0x1302;

/// The friends events.
pub const FRIENDS_START: u32 = 0x430C;
pub const FRIEND_STATE: u32 = 0x430D;

/// A tell.
pub const TELL: u32 = 0x5317;

/// The answer to the client's being done with its ticket.
pub const TICKET_RELEASED: u32 = 0xF202;

/// The result type that answers a kind of request.
#[must_use]
pub const fn result_type(kind: Kind) -> u32 {
    match kind {
        Kind::GetFriendsList => 0x4200,
        Kind::FriendAdd => 0x4201,
        Kind::FriendDelete => 0x4202,
        Kind::GetIgnoredList => 0x4203,
        Kind::IgnoredAdd => 0x4204,
        Kind::IgnoredDelete => 0x4205,
        Kind::Flags => 0x4208,
        Kind::SetLoc => 0x420A,
        Kind::SendToRoomEx => 0x521C,
        Kind::SendToUserEx => 0x521D,
        Kind::PresentTicket => 0xF201,
        Kind::CallCreate => 0x1_0200,
    }
}

/// The type a message is handed over as.
#[must_use]
pub fn message_type(message: &Message) -> u32 {
    match message {
        Message::Connected => CONNECTED,
        Message::Disconnected => DISCONNECTED,
        Message::Lost => LOST,
        Message::Result { reply, .. } => match reply {
            Reply::Answer(a) => result_type(a.kind()),
            Reply::TicketReleased => TICKET_RELEASED,
        },
        Message::Event(Event::FStart { .. }) => FRIENDS_START,
        Message::Event(Event::FriendStat { .. }) => FRIEND_STATE,
        Message::Event(Event::UTellEv { .. }) => TELL,
    }
}

/// One result or event as the client reads it, before it has an address: bytes, and the places in
/// them that point elsewhere in the block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    bytes: Vec<u8>,
    /// Each pointer: where it is, and the offset in the block it points to.
    pointers: Vec<(usize, usize)>,
}

impl Block {
    fn new(kind: u32, fields: usize) -> Self {
        let mut b = Self {
            bytes: vec![0; HEADER + 4 * fields],
            pointers: Vec::new(),
        };
        b.bytes[4] = 1;
        b.put(8, kind);
        b
    }

    /// The block's size in bytes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Whether the block is empty: never, since every block has its header.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// The block's size in 32-bit words: what to allocate for it.
    #[must_use]
    pub fn words(&self) -> usize {
        self.bytes.len().div_ceil(4)
    }

    /// The block's bytes as they are when it sits at `base`, every pointer filled in.
    #[must_use]
    pub fn at(&self, base: u32) -> Vec<u8> {
        let mut bytes = self.bytes.clone();
        for &(at, target) in &self.pointers {
            let address = base.wrapping_add(u32::try_from(target).unwrap_or(u32::MAX));
            bytes[at..at + 4].copy_from_slice(&address.to_le_bytes());
        }
        bytes
    }

    /// Fill `words`, which sit at `base`, with the block.
    pub fn fill(&self, base: u32, words: &mut [u32]) {
        let bytes = self.at(base);
        for (w, chunk) in words.iter_mut().zip(bytes.chunks(4)) {
            let mut four = [0u8; 4];
            four[..chunk.len()].copy_from_slice(chunk);
            *w = u32::from_le_bytes(four);
        }
    }

    fn put(&mut self, at: usize, value: u32) {
        self.bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }

    /// Field `i`: the word at `HEADER + 4 * i`.
    fn field(&mut self, i: usize, value: u32) {
        self.put(HEADER + 4 * i, value);
    }

    /// Room for `n` more bytes at the end, on a 4-byte boundary: their offset.
    fn tail(&mut self, n: usize) -> usize {
        let at = self.bytes.len().next_multiple_of(4);
        self.bytes.resize(at + n, 0);
        at
    }

    /// The word at `at` points to `target`.
    fn point(&mut self, at: usize, target: usize) {
        self.pointers.push((at, target));
    }

    /// A text at the end: UTF-16, then a NUL.
    fn text(&mut self, s: &str) -> usize {
        let units: Vec<u16> = s.encode_utf16().filter(|&u| u != 0).collect();
        let at = self.tail(2 * (units.len() + 1));
        for (i, u) in units.iter().enumerate() {
            self.bytes[at + 2 * i..at + 2 * i + 2].copy_from_slice(&u.to_le_bytes());
        }
        at
    }

    /// Field `i` points to a text.
    fn text_field(&mut self, i: usize, s: &str) {
        let at = self.text(s);
        self.point(HEADER + 4 * i, at);
    }

    /// A count in field `i` and the friends entries it counts in field `i + 1`.
    fn friends(&mut self, i: usize, friends: &[FriendStatus]) {
        self.field(i, count(friends.len()));
        if friends.is_empty() {
            return;
        }
        let list = self.tail(12 * friends.len());
        self.point(HEADER + 4 * (i + 1), list);
        for (n, f) in friends.iter().enumerate() {
            let entry = list + 12 * n;
            let name = self.text(&f.user);
            self.point(entry, name);
            self.put(entry + 4, f.mode.0);
            let location = self.text(&f.location);
            self.point(entry + 8, location);
        }
    }

    /// A count in field `i` and the names it counts in field `i + 1`.
    fn names(&mut self, i: usize, names: &[String]) {
        self.field(i, count(names.len()));
        if names.is_empty() {
            return;
        }
        let list = self.tail(4 * names.len());
        self.point(HEADER + 4 * (i + 1), list);
        for (n, name) in names.iter().enumerate() {
            let at = self.text(name);
            self.point(list + 4 * n, at);
        }
    }

    /// Field `i` points to extra data: its size and a pointer to its bytes, or both zero.
    fn extra(&mut self, i: usize, extra: Option<&Extra>) {
        let blob = self.tail(8);
        self.point(HEADER + 4 * i, blob);
        if let Some(e) = extra {
            self.put(blob, count(e.0.len()));
            let data = self.tail(e.0.len());
            self.bytes[data..data + e.0.len()].copy_from_slice(&e.0);
            self.point(blob + 4, data);
        }
    }
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// A message as the client reads it.
#[must_use]
pub fn block(message: &Message) -> Block {
    let kind = message_type(message);
    match message {
        Message::Connected | Message::Disconnected | Message::Lost => Block::new(kind, 0),
        Message::Event(Event::FStart { friends }) => {
            let mut b = Block::new(kind, 2);
            b.friends(0, friends);
            b
        }
        Message::Event(Event::FriendStat { user, mode }) => {
            let mut b = Block::new(kind, 2);
            b.text_field(0, user);
            b.field(1, mode.0);
            b
        }
        Message::Event(Event::UTellEv { user, text, extra }) => {
            let mut b = Block::new(kind, 4);
            b.text_field(0, user);
            b.text_field(2, text);
            b.extra(3, extra.as_ref());
            b
        }
        Message::Result { context, hr, reply } => {
            let fields = match reply {
                Reply::Answer(
                    Answer::GetFriendsList { .. }
                    | Answer::GetIgnoredList { .. }
                    | Answer::Flags { .. }
                    | Answer::CallCreate,
                ) => 4,
                Reply::Answer(Answer::PresentTicket { .. }) => 3,
                _ => 2,
            };
            let mut b = Block::new(kind, fields);
            b.field(0, *context);
            b.field(1, hr.0);
            match reply {
                Reply::Answer(Answer::GetFriendsList { friends }) => b.friends(2, friends),
                Reply::Answer(Answer::GetIgnoredList { ignored }) => b.names(2, ignored),
                Reply::Answer(Answer::Flags { which_flag, flags }) => {
                    b.field(2, *which_flag);
                    b.field(3, *flags);
                }
                Reply::Answer(Answer::PresentTicket { status }) => b.field(2, *status),
                _ => {}
            }
            b
        }
    }
}
