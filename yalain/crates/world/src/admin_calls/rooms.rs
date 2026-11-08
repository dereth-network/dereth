//! Chat rooms of an admin's own: `/guncreateroom`, `/gunjoinroom`, `/gunleaveroom`,
//! `/gunleaveallrooms`, and the lines `/gunsendtoroom` says in them through the chat service; and
//! persistent rooms, `/guncreateperstroom <name> <mode>` and `/gundeleteperstroom <name>`.
//!
//! No recording holds one and the original chat service isn't held, so these are this server's own.
//! A room is made by name and lasts while anyone is in it. Only admin characters are in one, each
//! until it leaves the room or the world, or is no longer an admin's. The client shows a line of a
//! room only if it is one of its channels, and these are not, so a line reaches each of the room's
//! members as an answer to an admin's call, marked with the room's name.
//!
//! A persistent room is the same, but outlives its members and the server: it keeps its number and
//! name in the world's record ([`RECORD`]) until it is deleted, and is joined by name as any room is.
//! Its mode is the chat service's room mode, kept and shown by the client's own names for it
//! ([`mode_name`]: standard, user, password, private, secret); nothing else reads it, as nothing
//! holds what the service did with it.

use std::collections::{BTreeMap, BTreeSet};

use yalain_net::{ConnId, ServerNet};
use yalain_store::Store;

use super::{Args, Cx};
use crate::Server;

/// The calls, by their function ids.
pub mod function {
    pub const JOIN: u32 = 0x8201_0005;
    pub const CREATE: u32 = 0x8201_002A;
    pub const LEAVE: u32 = 0x8201_004E;
    pub const LEAVE_ALL: u32 = 0x8201_005A;
    pub const CREATE_PERSISTENT: u32 = 0x8201_0072;
    pub const DELETE_PERSISTENT: u32 = 0x8201_0018;
}

/// The world's record of its persistent rooms.
pub const RECORD: &str = "chat-rooms";

/// A room mode by the chat service's own name for it, as the client writes it; another value by its
/// number.
#[must_use]
pub fn mode_name(mode: u32) -> String {
    match mode {
        1 => "standard".to_owned(),
        2 => "user".to_owned(),
        4 => "password".to_owned(),
        8 => "private".to_owned(),
        0x10 => "secret".to_owned(),
        other => format!("mode {other}"),
    }
}

/// The first room number: far past those of the world's fellowships and allegiances.
pub const FIRST_ROOM: u32 = 1_000_000;

/// The longest name a room may have.
pub const MAX_NAME: usize = 64;

/// One room.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Room {
    pub name: String,
    pub members: BTreeSet<u64>,
    /// A persistent room's mode; `None` for a room that ends with its last member.
    pub persistent: Option<u32>,
}

/// The rooms there are, by number.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Rooms {
    pub rooms: BTreeMap<u32, Room>,
    next: u32,
    /// Whether the persistent rooms were read from the store.
    pub loaded: bool,
}

impl Rooms {
    /// The persistent rooms and the next room's number, as the world's record keeps them.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let kept: Vec<_> = self
            .rooms
            .iter()
            .filter_map(|(n, r)| r.persistent.map(|m| (*n, m, r.name.as_bytes())))
            .collect();
        let mut b = Vec::new();
        b.extend(self.next.to_le_bytes());
        b.extend(u32::try_from(kept.len()).unwrap_or(u32::MAX).to_le_bytes());
        for (number, mode, name) in kept {
            b.extend(number.to_le_bytes());
            b.extend(mode.to_le_bytes());
            b.extend(u32::try_from(name.len()).unwrap_or(u32::MAX).to_le_bytes());
            b.extend(name);
        }
        b
    }

    /// The persistent rooms from the world's record, with no one in them; `None` for bytes that
    /// don't read as one.
    #[must_use]
    pub fn from_bytes(b: &[u8]) -> Option<Self> {
        fn take<'b>(b: &'b [u8], at: &mut usize, n: usize) -> Option<&'b [u8]> {
            let part = b.get(*at..at.checked_add(n)?)?;
            *at += n;
            Some(part)
        }
        fn word(b: &[u8], at: &mut usize) -> Option<u32> {
            Some(u32::from_le_bytes(take(b, at, 4)?.try_into().ok()?))
        }
        let mut at = 0usize;
        let next = word(b, &mut at)?;
        let count = word(b, &mut at)?;
        let mut rooms = BTreeMap::new();
        for _ in 0..count {
            let number = word(b, &mut at)?;
            let mode = word(b, &mut at)?;
            let len = usize::try_from(word(b, &mut at)?).ok()?;
            let name = String::from_utf8(take(b, &mut at, len)?.to_vec()).ok()?;
            rooms.insert(
                number,
                Room {
                    name,
                    members: BTreeSet::new(),
                    persistent: Some(mode),
                },
            );
        }
        (at == b.len()).then_some(Self {
            rooms,
            next,
            loaded: false,
        })
    }

    /// A room's number by its name, in any case.
    fn named(&self, name: &str) -> Option<u32> {
        self.rooms
            .iter()
            .find(|(_, r)| r.name.eq_ignore_ascii_case(name))
            .map(|(n, _)| *n)
    }

    /// A character is out of a room, which goes with its last member. Whether it was in it.
    fn take_out(&mut self, number: u32, character: u64) -> bool {
        let Some(r) = self.rooms.get_mut(&number) else {
            return false;
        };
        let was = r.members.remove(&character);
        if r.members.is_empty() && r.persistent.is_none() {
            self.rooms.remove(&number);
        }
        was
    }

    /// The rooms a character is in.
    fn of(&self, character: u64) -> Vec<u32> {
        self.rooms
            .iter()
            .filter(|(_, r)| r.members.contains(&character))
            .map(|(n, _)| *n)
            .collect()
    }
}

/// Read the persistent rooms from the store, once. Whether they are read: while the store cannot
/// answer, no room is made or deleted.
pub fn ensure<S: Store>(s: &mut Server<S>) -> bool {
    if s.admin_rooms.loaded {
        return true;
    }
    let Some(read) = crate::saves::read_record(s, RECORD, Rooms::from_bytes) else {
        return false;
    };
    let mut rooms = read.unwrap_or_default();
    // Rooms made before the record was read (none, unless the store failed at first) keep theirs.
    for (n, r) in std::mem::take(&mut s.admin_rooms.rooms) {
        rooms.rooms.entry(n).or_insert(r);
    }
    let past = rooms
        .rooms
        .keys()
        .next_back()
        .map_or(0, |n| n + 1 - FIRST_ROOM);
    rooms.next = rooms.next.max(s.admin_rooms.next).max(past);
    rooms.loaded = true;
    s.admin_rooms = rooms;
    true
}

/// Keep the persistent rooms.
fn keep<S: Store>(s: &mut Server<S>) {
    if s.admin_rooms.loaded {
        let b = s.admin_rooms.to_bytes();
        s.saves.hold_record(RECORD, b);
    }
}

/// A character is in no room any more: it left the world, or is no longer an admin's.
pub(crate) fn leave_all<S: Store>(s: &mut Server<S>, character: u64) {
    for n in s.admin_rooms.of(character) {
        s.admin_rooms.take_out(n, character);
    }
}

/// A call of this kind: its answer; `None` for any other.
pub(super) fn call<S: Store>(cx: &mut Cx<'_, S>, f: u32, a: &Args<'_>) -> Option<Vec<String>> {
    if ![
        function::CREATE,
        function::JOIN,
        function::LEAVE,
        function::LEAVE_ALL,
        function::CREATE_PERSISTENT,
        function::DELETE_PERSISTENT,
    ]
    .contains(&f)
    {
        return None;
    }
    if !ensure(cx.s) {
        return Some(vec![
            "The rooms can't be read from the world's store right now.".to_owned(),
        ]);
    }
    Some(match f {
        function::CREATE_PERSISTENT => create_persistent(cx, a),
        function::DELETE_PERSISTENT => delete_persistent(cx, a),
        function::CREATE => create(cx, a),
        function::JOIN => join(cx, a),
        function::LEAVE => leave(cx, a),
        function::LEAVE_ALL => {
            let me = cx.me();
            leave_all(cx.s, me);
            vec!["You left every room.".to_owned()]
        }
        _ => return None,
    })
}

/// A room's name as typed: trimmed, short, with nothing that would break a line.
fn name_of(a: &Args<'_>) -> Result<String, String> {
    let typed = a.text(0).unwrap_or_default();
    let name = typed.trim();
    if name.is_empty() || name.chars().count() > MAX_NAME || name.chars().any(char::is_control) {
        return Err(format!(
            "A room's name is 1 to {MAX_NAME} characters, on one line."
        ));
    }
    Ok(name.to_owned())
}

fn create<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let name = match name_of(a) {
        Ok(n) => n,
        Err(why) => return vec![why],
    };
    if let Some(n) = cx.s.admin_rooms.named(&name) {
        return vec![format!("There is a room {name} already: room {n}.")];
    }
    let rooms = &mut cx.s.admin_rooms;
    let number = FIRST_ROOM + rooms.next;
    rooms.next += 1;
    let me = cx.me();
    cx.s.admin_rooms.rooms.insert(
        number,
        Room {
            name: name.clone(),
            members: BTreeSet::from([me]),
            persistent: None,
        },
    );
    keep(cx.s);
    vec![format!("You made room {number}, {name}, and are in it.")]
}

fn create_persistent<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let name = match name_of(a) {
        Ok(n) => n,
        Err(why) => return vec![why],
    };
    let mode = a.int(1).unwrap_or(0);
    if let Some(n) = cx.s.admin_rooms.named(&name) {
        return vec![format!("There is a room {name} already: room {n}.")];
    }
    let rooms = &mut cx.s.admin_rooms;
    let number = FIRST_ROOM + rooms.next;
    rooms.next += 1;
    rooms.rooms.insert(
        number,
        Room {
            name: name.clone(),
            members: BTreeSet::new(),
            persistent: Some(mode),
        },
    );
    keep(cx.s);
    vec![format!(
        "You made persistent room {number}, {name}, {}. It stays until it is deleted; join it by name.",
        mode_name(mode)
    )]
}

fn delete_persistent<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let name = match name_of(a) {
        Ok(n) => n,
        Err(why) => return vec![why],
    };
    let Some(number) = cx.s.admin_rooms.named(&name) else {
        return vec![format!("There is no room {name}.")];
    };
    if cx.s.admin_rooms.rooms[&number].persistent.is_none() {
        return vec![format!(
            "Room {number}, {name}, isn't persistent: it ends when its last member leaves."
        )];
    }
    let gone = cx.s.admin_rooms.rooms.remove(&number);
    keep(cx.s);
    vec![format!(
        "You deleted persistent room {number}, {}.",
        gone.map_or(name, |r| r.name)
    )]
}

fn join<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let name = match name_of(a) {
        Ok(n) => n,
        Err(why) => return vec![why],
    };
    let me = cx.me();
    let Some(number) = cx.s.admin_rooms.named(&name) else {
        return vec![format!("There is no room {name}.")];
    };
    let Some(r) = cx.s.admin_rooms.rooms.get_mut(&number) else {
        return Vec::new();
    };
    r.members.insert(me);
    vec![format!("You are in room {number}, {}.", r.name)]
}

fn leave<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let number = a.int(0).unwrap_or(0);
    let me = cx.me();
    if cx.s.admin_rooms.take_out(number, me) {
        vec![format!("You left room {number}.")]
    } else {
        vec![format!("You are not in room {number}.")]
    }
}

/// A line to a room of this kind, from a character on a world connection: `None` when the room is
/// not one of these. Otherwise whether it went: only from a member with an admin's rights now, or it
/// goes nowhere and is answered as one to a room that isn't there. Each member with those rights
/// hears it.
pub(crate) fn said<S: Store>(
    s: &Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    room: u32,
    name: &str,
    text: &str,
) -> Option<bool> {
    let r = s.admin_rooms.rooms.get(&room)?;
    let member = s
        .players
        .get(&world)
        .is_some_and(|p| r.members.contains(&p.character));
    if !member || super::allowed(s, world).is_none() {
        return Some(false);
    }
    let line = format!("[{}] {name}: {text}", r.name);
    for (w, p) in &s.players {
        if r.members.contains(&p.character) && super::allowed(s, *w).is_some() {
            super::answer(net, *w, &line);
        }
    }
    Some(true)
}
