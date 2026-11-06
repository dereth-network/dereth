//! An admin's commands over factions, player killing, how creatures treat it, and what it passes
//! through.

use yalain_store::Store;

use super::{other, Args, Cx, Target};
use crate::{kingdoms, progress, quals};

/// How creatures treat an admin (the client's words, in its order: normal, stealth, super).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Attackable {
    /// Detected and attacked as usual.
    #[default]
    Normal,
    /// Not detected, but fought back when it attacks.
    Stealth,
    /// Completely invisible to creatures.
    Super,
}

/// The override of what an admin passes through.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Ethereality {
    /// As the admin's own qualities say.
    #[default]
    Normal,
    /// Through players and people as well as what a mover passes through by default.
    Admin,
    /// Through everything that has a physics type.
    Super,
}

/// The values of the enumeration of `/ethereality`, which no install holds: a client names them as
/// this server does ([`Ethereality`]), and a retail client is served their record
/// ([`crate::admin_library`]).
pub mod ethereal_value {
    pub const NORMAL: u32 = 1;
    pub const ADMIN: u32 = 2;
    pub const SUPER: u32 = 3;
}

/// The movement ethereality of an override: every physics type for `Super`; for `Admin` the bits of
/// everything a mover passes through by default, of players and of people, on top of its own.
const ETHEREAL_ALL: i32 = -1;
const ETHEREAL_ADMIN: i32 = 1 << 4 | 1 << 23 | 1 << 24;

/// The numbers of `/attackable`'s words, the client's own.
const ATTACKABLE_NORMAL: u32 = 0x4000_0001;
const ATTACKABLE_STEALTH: u32 = 0x4000_0002;
const ATTACKABLE_SUPER: u32 = 0x4000_0003;

/// The calls of the admin family here, by their function ids.
pub mod function {
    pub const PK_RATING: u32 = 0x8201_0087;
    pub const SET_PK_RATING: u32 = 0x8201_000A;
}

/// A player's qualities of player killing (the property list's numbers).
const PK_TRUE: u32 = 0x226;
const PK_FALSE: u32 = 0x227;
const PK_RATING: u32 = 0x228;

/// A call of this kind: its answer; `None` for any other.
pub(super) fn call<S: Store>(cx: &mut Cx<'_, S>, f: u32, a: &Args<'_>) -> Option<Vec<String>> {
    Some(match f {
        other::AI_ATTACKABLE => attackable(cx, a),
        other::ETHEREALITY => ethereality(cx, a),
        other::PK_OVERRIDE => pk_override(cx, a),
        other::PK_STATUS => pk_status(cx, a),
        other::FACTION_SHOW => faction_show(cx),
        other::FACTION_OWNER => vec![
            "This server's landblocks are owned by the world database; change them there."
                .to_owned(),
        ],
        other::FACTION_TYPE => faction_type(cx, a),
        function::PK_RATING => pk_rating(cx, a),
        function::SET_PK_RATING => set_pk_rating(cx, a),
        _ => return None,
    })
}

fn attackable<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let (state, said) = match a.int(0) {
        Some(ATTACKABLE_NORMAL) => (Attackable::Normal, "Creatures treat you as usual."),
        Some(ATTACKABLE_STEALTH) => (
            Attackable::Stealth,
            "Creatures do not notice you, but fight back if you attack.",
        ),
        Some(ATTACKABLE_SUPER) => (Attackable::Super, "You are invisible to creatures."),
        _ => return vec!["That is not a way to be treated.".to_owned()],
    };
    cx.state().attackable = state;
    vec![said.to_owned()]
}

/// `/ethereality`: what the admin passes through, told to every client that knows it; the override
/// ends when it is lifted or the admin next enters the world.
fn ethereality<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let (over, said) = match a.int(0) {
        Some(ethereal_value::NORMAL) => (Ethereality::Normal, "Your ethereality is your own."),
        Some(ethereal_value::ADMIN) => (Ethereality::Admin, "You pass through players and people."),
        Some(ethereal_value::SUPER) => (
            Ethereality::Super,
            "You pass through everything with a physics type.",
        ),
        _ => return vec!["That is not an ethereality.".to_owned()],
    };
    cx.state().ethereality = over;
    let world = cx.world;
    match over {
        Ethereality::Normal => kingdoms::settle_ethereality(cx.s, cx.net, world),
        Ethereality::Admin => {
            let own =
                cx.s.players
                    .get(&world)
                    .map_or(0, |p| kingdoms::kept_ethereality(&p.qualities));
            kingdoms::tell_ethereality(cx.s, cx.net, world, own | ETHEREAL_ADMIN);
        }
        Ethereality::Super => kingdoms::tell_ethereality(cx.s, cx.net, world, ETHEREAL_ALL),
    }
    vec![said.to_owned()]
}

fn pk_override<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let w = match cx.player(a.long(0).unwrap_or(0)) {
        Ok(w) => w,
        Err(why) => return vec![why],
    };
    let (yes, no) = (a.int(1).unwrap_or(0), a.int(2).unwrap_or(0));
    let Some(p) = cx.s.players.get_mut(&w) else {
        return Vec::new();
    };
    quals::set_int(&mut p.qualities, PK_TRUE, yes.cast_signed());
    quals::set_int(&mut p.qualities, PK_FALSE, no.cast_signed());
    let name = p.name.clone();
    progress::send_int(cx.net, w, PK_TRUE, yes.cast_signed());
    progress::send_int(cx.net, w, PK_FALSE, no.cast_signed());
    vec![format!(
        "{name}'s player-killing bits are {yes:#x} and {no:#x}."
    )]
}

fn pk_status<S: Store>(cx: &Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let w = match cx.player(a.long(0).unwrap_or(0)) {
        Ok(w) => w,
        Err(why) => return vec![why],
    };
    let Some(p) = cx.s.players.get(&w) else {
        return Vec::new();
    };
    let int = |k| quals::int(&p.qualities, k).unwrap_or(0);
    vec![format!(
        "{}: player-killing bits {:#x} and {:#x}, kingdom {:?}.",
        p.name,
        int(PK_TRUE),
        int(PK_FALSE),
        kingdoms::of_qualities(&quals::read(&p.qualities))
    )]
}

fn faction_show<S: Store>(cx: &Cx<'_, S>) -> Vec<String> {
    let Some(p) = cx.s.players.get(&cx.world) else {
        return Vec::new();
    };
    let status = kingdoms::status_at(&cx.s.content.conflict, p.position.cell);
    vec![format!(
        "The conflict status of landblock {:04X} is {status:#x}.",
        p.position.cell >> 16
    )]
}

fn faction_type<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let kingdom = a.int(1).unwrap_or(0);
    match cx.target(a.long(0).unwrap_or(0)) {
        Some(Target::Player(w)) => {
            let Some(p) = cx.s.players.get_mut(&w) else {
                return Vec::new();
            };
            kingdoms::set_kingdom(&mut p.qualities, kingdom);
            let name = p.name.clone();
            kingdoms::settle_ethereality(cx.s, cx.net, w);
            crate::world::keep_player(cx.s, w);
            vec![format!("{name} is of kingdom {kingdom:#x}.")]
        }
        Some(Target::Thing(id)) => {
            if let Some(t) = cx.s.things.get_mut(id) {
                t.kingdom = Some(kingdom);
            }
            vec![format!("{id:#x} is of kingdom {kingdom:#x}.")]
        }
        None => vec!["No target.".to_owned()],
    }
}

fn pk_rating<S: Store>(cx: &Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let w = match cx.player(a.long(0).unwrap_or(0)) {
        Ok(w) => w,
        Err(why) => return vec![why],
    };
    let Some(p) = cx.s.players.get(&w) else {
        return Vec::new();
    };
    vec![format!(
        "{}'s rating is {}.",
        p.name,
        quals::int(&p.qualities, PK_RATING).unwrap_or(0)
    )]
}

fn set_pk_rating<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let w = match cx.player(a.long(0).unwrap_or(0)) {
        Ok(w) => w,
        Err(why) => return vec![why],
    };
    let rating = a.signed(1).unwrap_or(0);
    let Some(p) = cx.s.players.get_mut(&w) else {
        return Vec::new();
    };
    quals::set_int(&mut p.qualities, PK_RATING, rating);
    let name = p.name.clone();
    progress::send_int(cx.net, w, PK_RATING, rating);
    vec![format!("{name}'s rating is {rating}.")]
}
