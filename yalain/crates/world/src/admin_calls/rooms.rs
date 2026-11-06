//! Chat rooms of an admin's own: `/guncreateroom`, `/gunjoinroom`, `/gunleaveroom`,
//! `/gunleaveallrooms`, and the lines `/gunsendtoroom` says in them through the chat service.
//!
//! No recording holds one and the original chat service isn't held, so these are this server's own.
//! A room is made by name and lasts while anyone is in it. Only admin characters are in one, each
//! until it leaves the room or the world, or is no longer an admin's. The client shows a line of a
//! room only if it is one of its channels, and these are not, so a line reaches each of the room's
//! members as an answer to an admin's call, marked with the room's name.

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
}

/// The rooms there are, by number.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Rooms {
    pub rooms: BTreeMap<u32, Room>,
    next: u32,
}

impl Rooms {
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
        if r.members.is_empty() {
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

/// A character is in no room any more: it left the world, or is no longer an admin's.
pub(crate) fn leave_all<S: Store>(s: &mut Server<S>, character: u64) {
    for n in s.admin_rooms.of(character) {
        s.admin_rooms.take_out(n, character);
    }
}

/// A call of this kind: its answer; `None` for any other.
pub(super) fn call<S: Store>(cx: &mut Cx<'_, S>, f: u32, a: &Args<'_>) -> Option<Vec<String>> {
    Some(match f {
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
        },
    );
    vec![format!("You made room {number}, {name}, and are in it.")]
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
