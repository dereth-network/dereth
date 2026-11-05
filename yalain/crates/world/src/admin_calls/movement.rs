//! An admin's commands that move: teleports of every kind, its saved places, the lifestone,
//! evacuating a landblock, its run speed, pushes, freezing, and asking where things are.

use auberean_primitives::Vec3;
use auberean_protocol::physics::{Impulse, Scale, TargetedImpulse};
use auberean_protocol::quality::PrivateQuality;
use auberean_protocol::types::Placement;
use auberean_protocol::ServerMessage;
use dereth_primitives::num::math;
use yalain_net::ConnId;
use yalain_store::{Store, StoredPosition};

use super::{Args, Cx, Target};
use crate::places::{self, kind};
use crate::{presence, progress};

/// The calls, by their function ids.
pub mod function {
    pub const TELE_TO: u32 = 0x8201_0058;
    pub const TELEPORT: u32 = 0x8201_0135;
    pub const HOME: u32 = 0x8201_0065;
    pub const SAVE: u32 = 0x8201_0133;
    pub const LIFESTONE: u32 = 0x8201_002D;
    pub const EVACUATE: u32 = 0x8201_0125;
    pub const RUN: u32 = 0x8201_0044;
    pub const HALT: u32 = 0x8201_0041;
    pub const MOVEMENT: u32 = 0x8201_007C;
    pub const DISTANCE: u32 = 0x8201_0076;
    pub const HEIGHT: u32 = 0x8201_0062;
    pub const IMPULSE: u32 = 0x8201_0008;
    pub const PUSH_FROM: u32 = 0x8201_0031;
    pub const PUSH_OTHER: u32 = 0x8201_0022;
    pub const INTERSECTION: u32 = 0x8201_0052;
    pub const PROPERTIES: u32 = 0x8201_0025;
}

/// The quality the first saved place is kept under; the other nine follow it, as the client's own
/// list of them reads them.
pub const SAVED_BASE: u32 = 600;
/// How many places an admin saves.
pub const SAVED_SLOTS: u32 = 10;
/// The most an admin's speed is scaled, as the client's help says.
pub const MOST_RUN: f32 = 10.0;

/// A call of this kind: its answer; `None` for any other.
pub(super) fn call<S: Store>(cx: &mut Cx<'_, S>, f: u32, a: &Args<'_>) -> Option<Vec<String>> {
    Some(match f {
        function::TELE_TO => tele_to(cx, a),
        function::TELEPORT => teleport(cx, a),
        function::HOME => go_home(cx, a),
        function::SAVE => save(cx, a),
        function::LIFESTONE => lifestone(cx),
        function::EVACUATE => evacuate(cx),
        function::RUN => run(cx, a),
        function::HALT => halt(cx, a),
        function::MOVEMENT => movement(cx, a),
        function::DISTANCE => distance(cx, a),
        function::HEIGHT => height(cx),
        function::IMPULSE => impulse(cx, a),
        function::PUSH_FROM => pushed(cx, a, true),
        function::PUSH_OTHER => pushed(cx, a, false),
        function::INTERSECTION => intersection(cx, a),
        function::PROPERTIES => properties(cx, a),
        _ => return None,
    })
}

/// The player a teleport moves: the name, or the admin with none.
fn mover<S: Store>(cx: &Cx<'_, S>, name: Option<String>) -> Result<ConnId, String> {
    match name.filter(|n| !n.is_empty()) {
        None => Ok(cx.world),
        Some(n) => cx
            .player_named(&n)
            .ok_or_else(|| format!("{n} is not in the world.")),
    }
}

/// Where the words of a teleport say: `<cell> <x> <y> <z> [heading]`, `<id>` (an object or a player
/// by its number in hex), `<latitude> <longitude>`, a place the world names, or a character.
fn destination<S: Store>(cx: &Cx<'_, S>, words: &[String]) -> Result<StoredPosition, String> {
    let f = |w: &String| w.parse::<f32>().ok().filter(|v| v.is_finite());
    let cell = |w: &String| u32::from_str_radix(w.trim_start_matches("0x"), 16).ok();
    match words {
        [c, x, y, z, rest @ ..] if rest.len() <= 1 => {
            let heading = rest.first().and_then(f).unwrap_or(0.0);
            return Ok(StoredPosition {
                cell: cell(c).ok_or("That cell is not a number in hex.")?,
                origin: [
                    f(x).ok_or("That x is not a number.")?,
                    f(y).ok_or("That y is not a number.")?,
                    f(z).ok_or("That z is not a number.")?,
                ],
                rotation: rotation_of_degrees(heading),
            });
        }
        [one] if one.starts_with("0x") || one.chars().all(|c| c.is_ascii_hexdigit()) => {
            if let Ok(id) = u64::from_str_radix(one.trim_start_matches("0x"), 16) {
                if let Some(at) = position_of(cx, id) {
                    return Ok(at);
                }
            }
        }
        [lat, long] => {
            if let (Some(lat), Some(long)) = (coordinate(lat), coordinate(long)) {
                return of_map(cx, lat, long).ok_or_else(|| "That is off the land.".to_owned());
            }
        }
        _ => {}
    }
    let named = words.join(" ");
    if let Some(pl) = cx.s.places_by_name.get(&named.to_ascii_lowercase()) {
        return Ok(StoredPosition {
            cell: pl.cell,
            origin: pl.origin,
            rotation: pl.rotation,
        });
    }
    if let Some(p) = cx.player_named(&named).and_then(|w| cx.s.players.get(&w)) {
        let from =
            cx.s.players
                .get(&cx.world)
                .map_or(p.position, |me| me.position);
        return Ok(places::beside(p.position.cell, p.position.origin, &from));
    }
    Err(format!("The world knows no place or character {named}."))
}

/// The rotation of a heading in degrees clockwise from north.
fn rotation_of_degrees(heading: f32) -> [f32; 4] {
    let half = -heading.to_radians() / 2.0;
    [math::cosf(half), 0.0, 0.0, math::sinf(half)]
}

/// A map coordinate: a number, with `n`, `s`, `e` or `w` after it.
fn coordinate(w: &str) -> Option<f32> {
    let t = w.trim().to_ascii_lowercase();
    let (n, sign) = match t.chars().last()? {
        'n' | 'e' => (&t[..t.len() - 1], 1.0),
        's' | 'w' => (&t[..t.len() - 1], -1.0),
        _ => (t.as_str(), 1.0),
    };
    Some(n.parse::<f32>().ok().filter(|v| v.is_finite())? * sign)
}

/// The land's size in land cells, and a land cell's in meters.
const LAND: (i32, i32) = (255 * 8, 255 * 8);
const LAND_CELL: f32 = 24.0;

/// A map's latitude and longitude as a place on the land, standing on the ground where the server
/// holds the ground: the land's map runs from -100 to 100 each way.
fn of_map<S: Store>(cx: &Cx<'_, S>, lat: f32, long: f32) -> Option<StoredPosition> {
    #[allow(clippy::cast_precision_loss)]
    let along = |c: f32, size: i32| (c / 100.0 + 1.0) * size as f32 * 0.5;
    let (x, y) = (along(long, LAND.0), along(lat, LAND.1));
    let (cx_, cy) = (
        dereth_primitives::num::floor_to_i32(x),
        dereth_primitives::num::floor_to_i32(y),
    );
    let cell = auberean_primitives::land::cell_at(cx_, cy, LAND);
    if cell == 0 {
        return None;
    }
    // Each landblock holds eight land cells across.
    let within = |at: f32| at.rem_euclid(8.0) * LAND_CELL;
    let (ox, oy) = (within(x), within(y));
    let z =
        cx.s.ground
            .as_ref()
            .and_then(|g| g.height(cell, [ox, oy, 0.0]))
            .unwrap_or(0.0);
    Some(StoredPosition {
        cell,
        origin: [ox, oy, z],
        rotation: rotation_of_degrees(0.0),
    })
}

/// Where an object is, by its id.
fn position_of<S: Store>(cx: &Cx<'_, S>, id: u64) -> Option<StoredPosition> {
    if let Some(p) = cx.s.players.values().find(|p| p.character == id) {
        return Some(p.position);
    }
    if let Some(at) = cx.s.things.get(id).and_then(|t| t.at) {
        return Some(at);
    }
    presence::content_by_id(&cx.s.content.objects, id).map(|i| {
        let o = &cx.s.content.objects[i];
        StoredPosition {
            cell: o.cell,
            origin: o.origin,
            rotation: o.rotation,
        }
    })
}

/// Send a player somewhere; the answer.
fn send<S: Store>(cx: &mut Cx<'_, S>, who: ConnId, to: StoredPosition) -> Vec<String> {
    let name = cx.name_of(who);
    if cx.s.players.get(&who).is_some_and(|p| p.trip.is_some()) {
        return vec![format!("{name} is on its way somewhere already.")];
    }
    places::sent_away(cx.s, cx.net, who, to, cx.clock);
    vec![format!(
        "{name} is on its way to {:08X} {:?}.",
        to.cell, to.origin
    )]
}

fn tele_to<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let who = match mover(cx, a.text(0)) {
        Ok(w) => w,
        Err(why) => return vec![why],
    };
    let loc = a.text(1).unwrap_or_default();
    // A cell's words come as `<cell> [x y z] heading`, the others bare.
    let words: Vec<String> = loc
        .split(|c: char| c.is_whitespace() || c == '[' || c == ']')
        .filter(|w| !w.is_empty())
        .map(str::to_owned)
        .collect();
    let to = match destination(cx, &words) {
        Ok(to) => to,
        Err(why) => return vec![why],
    };
    send(cx, who, to)
}

fn teleport<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let who = match mover(cx, a.text(0)) {
        Ok(w) => w,
        Err(why) => return vec![why],
    };
    let to = match destination(cx, &a.list(1)) {
        Ok(to) => to,
        Err(why) => return vec![why],
    };
    send(cx, who, to)
}

fn slot_kind(slot: u32) -> Option<u32> {
    (1..=SAVED_SLOTS)
        .contains(&slot)
        .then(|| SAVED_BASE + slot - 1)
}

fn go_home<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let who = match cx.player(a.long(0).unwrap_or(0)) {
        Ok(w) => w,
        Err(why) => return vec![why],
    };
    let Some(kind) = a.int(1).and_then(slot_kind) else {
        return vec!["A slot is from 1 to 10.".to_owned()];
    };
    let saved =
        cx.s.players
            .get(&who)
            .and_then(|p| p.places.iter().find(|(k, _)| *k == kind).map(|(_, at)| *at));
    match saved {
        Some(at) => send(cx, who, at),
        None => vec!["That slot is empty.".to_owned()],
    }
}

fn save<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let who = match cx.player(a.long(0).unwrap_or(0)) {
        Ok(w) => w,
        Err(why) => return vec![why],
    };
    let Some(kind) = a.int(1).and_then(slot_kind) else {
        return vec!["A slot is from 1 to 10.".to_owned()];
    };
    let Some(p) = cx.s.players.get_mut(&who) else {
        return vec!["That is not a player in the world.".to_owned()];
    };
    let at = p.position;
    p.places.retain(|(k, _)| *k != kind);
    p.places.push((kind, at));
    let name = p.name.clone();
    progress::private(
        cx.net,
        who,
        &ServerMessage::UpdatePlacement(PrivateQuality {
            quality: kind,
            value: crate::player::placement(&at),
        }),
    );
    crate::world::keep_player(cx.s, who);
    vec![format!(
        "{name}'s place is saved in slot {}.",
        kind - SAVED_BASE + 1
    )]
}

fn lifestone<S: Store>(cx: &mut Cx<'_, S>) -> Vec<String> {
    let bound = cx.s.players.get(&cx.world).and_then(|p| {
        p.places
            .iter()
            .find(|(k, _)| *k == kind::LIFESTONE)
            .map(|(_, at)| *at)
    });
    match bound {
        Some(at) => send(cx, cx.world, at),
        None => vec!["You are bound to no lifestone.".to_owned()],
    }
}

fn evacuate<S: Store>(cx: &mut Cx<'_, S>) -> Vec<String> {
    let Some(block) = cx.s.players.get(&cx.world).map(|p| p.position.cell >> 16) else {
        return Vec::new();
    };
    let mut here: Vec<ConnId> =
        cx.s.players
            .iter()
            .filter(|(w, p)| **w != cx.world && p.position.cell >> 16 == block && p.trip.is_none())
            .map(|(w, _)| *w)
            .collect();
    here.sort_unstable();
    for w in &here {
        let to = places::resurrection_place(cx.s, *w);
        places::sent_away(cx.s, cx.net, *w, to, cx.clock);
    }
    vec![format!("{} players sent to their lifestones.", here.len())]
}

fn run<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let Some(by) = a.float(0).filter(|v| *v > 0.0 && *v < MOST_RUN) else {
        return vec!["The run speed is scaled by more than 0 and less than 10.".to_owned()];
    };
    cx.state().run = by;
    let Some(p) = cx.s.players.get_mut(&cx.world) else {
        return Vec::new();
    };
    let m = ServerMessage::VelocityScale(places::player_event(p, Scale(by)));
    let me = p.character;
    progress::private(cx.net, cx.world, &m);
    places::to_knowers(cx.s, cx.net, me, &m);
    vec![format!("Your run speed is scaled by {by}.")]
}

fn halt<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let on = a.flag(1).unwrap_or(true);
    match cx.target(a.long(0).unwrap_or(0)) {
        Some(Target::Player(w)) => {
            progress::private(cx.net, w, &crate::session::movement_frozen(on));
            progress::private(cx.net, w, &places::animation_frozen(on));
            let name = cx.name_of(w);
            vec![format!(
                "{name} is {}.",
                if on { "frozen" } else { "free to move" }
            )]
        }
        Some(Target::Thing(id)) => {
            let Some(c) = cx.s.things.get_mut(id).and_then(|t| t.creature.as_mut()) else {
                return vec!["That is not something that moves.".to_owned()];
            };
            // A creature frozen waits a day for its next look around.
            c.next_look = if on { f64::MAX } else { 0.0 };
            c.next_wander = if on { f64::MAX } else { 0.0 };
            vec![format!(
                "It is {}.",
                if on { "frozen" } else { "free to move" }
            )]
        }
        None => vec!["No target.".to_owned()],
    }
}

fn movement<S: Store>(cx: &Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    match cx.target(a.long(0).unwrap_or(0)) {
        Some(Target::Player(w)) => {
            let Some(p) = cx.s.players.get(&w) else {
                return Vec::new();
            };
            vec![format!(
                "{}: cell {:08X} at {:?}, heading and motion {:#010x}, following {:?}, walking to a \
                 use {}, on a trip {}.",
                p.name,
                p.position.cell,
                p.position.origin,
                p.heading_motion,
                p.following,
                p.walking_to.is_some(),
                p.trip.is_some()
            )]
        }
        Some(Target::Thing(id)) => {
            let Some(t) = cx.s.things.get(id) else {
                return Vec::new();
            };
            vec![format!(
                "{id:#x}: at {:?}, heading and motion {:#010x}, mode {:?}.",
                t.at,
                t.heading_motion,
                t.creature.as_ref().map(|c| c.mode)
            )]
        }
        None => vec!["No target.".to_owned()],
    }
}

fn distance<S: Store>(cx: &Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let id = a.long(0).unwrap_or(0);
    let id = if id == 0 {
        cx.s.players.get(&cx.world).map_or(0, |p| p.selection)
    } else {
        id
    };
    let (Some(me), Some(at)) = (cx.s.players.get(&cx.world), position_of(cx, id)) else {
        return vec!["No target.".to_owned()];
    };
    match presence::distance(&me.position, &at) {
        Some(d) => vec![format!("{id:#x} is {d} meters away.")],
        None => vec![format!("{id:#x} is too far to measure.")],
    }
}

fn height<S: Store>(cx: &Cx<'_, S>) -> Vec<String> {
    let Some(p) = cx.s.players.get(&cx.world) else {
        return Vec::new();
    };
    let ground =
        cx.s.ground
            .as_ref()
            .and_then(|g| g.height(p.position.cell, p.position.origin));
    match ground {
        Some(z) => vec![format!(
            "The landscape here is at {z}; you are at {}.",
            p.position.origin[2]
        )],
        None => vec![format!(
            "This server holds no landscape here; you are at {}.",
            p.position.origin[2]
        )],
    }
}

fn impulse<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let (Some(x), Some(y), Some(z)) = (a.float(0), a.float(1), a.float(2)) else {
        return vec!["An impulse is x, y and z.".to_owned()];
    };
    let contact = a.int(3).unwrap_or(0);
    let Some(p) = cx.s.players.get_mut(&cx.world) else {
        return Vec::new();
    };
    let m = ServerMessage::Push(places::player_event(
        p,
        Impulse {
            direction: Vec3::new(x, y, z),
            contact,
        },
    ));
    progress::private(cx.net, cx.world, &m);
    vec![format!("You are pushed by ({x}, {y}, {z}).")]
}

/// A push measured against a target: `from` pushes the admin, otherwise the target.
fn pushed<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>, from: bool) -> Vec<String> {
    let target = a.long(0).unwrap_or(0);
    let (magnitude, falloff, vertical) = (
        a.float(1).unwrap_or(0.0),
        a.int(2).unwrap_or(0),
        a.float(3).unwrap_or(0.0),
    );
    let (moved, away_from) = if from {
        (Ok(cx.world), target)
    } else {
        (cx.player(target), cx.me())
    };
    let moved = match moved {
        Ok(w) => w,
        Err(why) => return vec![why],
    };
    let Some(p) = cx.s.players.get_mut(&moved) else {
        return Vec::new();
    };
    let m = ServerMessage::PushToward(places::player_event(
        p,
        TargetedImpulse {
            target: away_from,
            magnitude,
            falloff,
            vertical,
        },
    ));
    let name = p.name.clone();
    progress::private(cx.net, moved, &m);
    vec![format!("{name} is pushed.")]
}

fn intersection<S: Store>(cx: &Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let (Some(x), Some(y), Some(z), Some(r)) = (a.float(0), a.float(1), a.float(2), a.float(3))
    else {
        return vec!["A sphere is x, y, z and a radius.".to_owned()];
    };
    let Some(me) = cx.s.players.get(&cx.world) else {
        return Vec::new();
    };
    let centre = StoredPosition {
        origin: [x, y, z],
        ..me.position
    };
    let near = |at: &StoredPosition| presence::distance(&centre, at).is_some_and(|d| d <= r);
    let mut ids: Vec<u64> =
        cx.s.players
            .values()
            .filter(|p| near(&p.position))
            .map(|p| p.character)
            .chain(
                cx.s.things
                    .placed()
                    .filter(|t| t.at.as_ref().is_some_and(near))
                    .map(|t| t.id),
            )
            .collect();
    ids.sort_unstable();
    let mut lines = vec![format!("{} objects inside.", ids.len())];
    lines.extend(ids.iter().map(|id| format!("{id:#x}")));
    lines
}

fn properties<S: Store>(cx: &Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let id = a.long(0).unwrap_or(0);
    match cx.target(id) {
        Some(Target::Player(w)) => {
            let Some(p) = cx.s.players.get(&w) else {
                return Vec::new();
            };
            let m = crate::player::Marks::of(&crate::quals::read(&p.qualities));
            let at: Placement = crate::player::placement(&p.position);
            vec![format!(
                "{}: physics words {:#x?}, placed {at:?}.",
                p.name, m.numbers
            )]
        }
        Some(Target::Thing(id)) => {
            let Some(t) = cx.s.things.get(id) else {
                return Vec::new();
            };
            vec![format!(
                "{id:#x}: template {:08X}, at {:?}, kind {:?}.",
                t.template, t.at, t.kind
            )]
        }
        None => vec!["No target.".to_owned()],
    }
}
