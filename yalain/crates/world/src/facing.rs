//! People turn to face whoever uses them.
//!
//! A placed person that a player uses turns to face the player, and what the use makes it say and do
//! waits until the turn is done. Its place is told with its aim, the heading toward the player, a
//! moment after the use's answer, and told again, aimed at where the player is then, for as long as
//! it turns; the turn takes a beat and the angle at its body's rate, the still turn rate its motion
//! description gives in peace on the ground. Then it speaks, and its place is told once more with
//! the heading it stopped at, which it keeps. A person facing its user within a few degrees doesn't
//! turn: its place is told once, as it stands, and it speaks a moment later.
//!
//! What turns is what names a movement type other than stationary. An object that answers a use as
//! a person does, of the same script packages as people, stationary or naming no movement type,
//! speaks at once and stays as it stands; so does a person walking its route, and one whose body
//! gives no rate turns no more than one facing its user.

use std::collections::BTreeMap;

use auberean_protocol::physics::ObjectPosition;
use auberean_protocol::types::ObjectStamp;
use yalain_content::movement::MovementType;
use yalain_net::{ConnId, ServerNet};
use yalain_store::{Store, StoredPosition};

use crate::{ai, presence, Clock, Server, Timer};

/// How far off its user a person may face and not turn, in degrees.
pub const TURNS_PAST: f32 = 10.0;
/// How long after the use's answer a person's place is first told, in seconds.
pub const FIRST_TOLD_AFTER: f64 = 0.1;
/// How often a turning person's aim is told again, in seconds: by turns, the one and the other.
pub const TOLD_EVERY: [f64; 2] = [0.2, 0.3];
/// The beat a turn takes besides its angle at its body's rate, in seconds.
pub const TURN_BEAT: f64 = 0.152;
/// How long after the use's answer a person facing its user speaks, in seconds.
pub const FACING_SPEAKS_AFTER: f64 = 0.17;

/// A person's turn under way: whom it turns to, when the turn is done (world seconds), whether it
/// aims at all (one facing its user is told as it stands), how many times its place has been told,
/// and its number, so a later turn replaces an earlier one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Turn {
    pub to: ConnId,
    pub done: f64,
    pub aims: bool,
    pub told: u32,
    pub count: u32,
}

/// The people turned or turning: the stamp their places were last told with, the turns under way,
/// and how many turns have begun.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Facing {
    pub stamps: BTreeMap<u64, u16>,
    pub turning: BTreeMap<u64, Turn>,
    pub turns: u32,
}

/// A rotation's heading, in degrees.
#[must_use]
pub fn degrees_of(rotation: [f32; 4]) -> f32 {
    (-2.0 * dereth_primitives::num::math::atan2f(rotation[3], rotation[0]).to_degrees())
        .rem_euclid(360.0)
}

/// The angle between two headings in degrees, the short way round.
#[must_use]
pub fn angle(from: f32, to: f32) -> f32 {
    let d = (to - from).rem_euclid(360.0);
    d.min(360.0 - d)
}

/// How long a turn of `angle` degrees takes a body of `rate` degrees a second, its beat with it.
#[must_use]
pub fn turn_seconds(angle: f32, rate: f32) -> f64 {
    TURN_BEAT + f64::from(angle / rate)
}

/// May what is placed of a template turn to its user: it names a movement type, and not the one
/// that never moves nor turns. One that names none is an object: the type a creature takes in its
/// place is not its own.
#[must_use]
pub fn turns(props: &yalain_content::Props) -> bool {
    props
        .word(yalain_content::props::creature_property::MOVEMENT_TYPE)
        .and_then(MovementType::from_value)
        .is_some_and(|kind| kind != MovementType::Stationary)
}

/// A body's still turn rate in degrees a second, as its motion description gives it: the default
/// values of the set its element on the ground takes in peace. None for a description that gives
/// no such set, or a rate that isn't above 0.
#[must_use]
pub fn still_turn_rate(md: &auberean_assets::animation::MotionDesc) -> Option<f32> {
    md.set_of
        .iter()
        .find(|(k, _)| {
            k.element == auberean_physics::motion::EARTH
                && k.mode == auberean_physics::motion::PEACE
        })
        .and_then(|(_, set)| md.sets.get(*set as usize))
        .map(|v| v.default.still_turn_rate)
        .filter(|r| *r > 0.0)
}

/// The still turn rate a look gives: its motion description's ([`still_turn_rate`]), the look's
/// parents' where the look names none. `visual` reads a visual description, `motion` a motion
/// description.
pub fn still_turn_of_look(
    look: u32,
    visual: impl Fn(u32) -> Option<auberean_assets::visual::VisualDesc>,
    motion: impl Fn(u32) -> Option<auberean_assets::animation::MotionDesc>,
) -> Option<f32> {
    let mut did = look;
    for _ in 0..16 {
        let v = (did != 0).then(|| visual(did)).flatten()?;
        if let Some(md) = v.motion_desc {
            return still_turn_rate(&motion(md.0)?);
        }
        did = v.parent?.0;
    }
    None
}

/// The heading from a person's place toward a player's, in degrees; none where they stand together.
fn bearing(at: &StoredPosition, user: &StoredPosition) -> Option<f32> {
    let (dx, dy, _) = ai::offset(at, user);
    if dx.abs() < f32::EPSILON && dy.abs() < f32::EPSILON {
        return None;
    }
    Some(
        dereth_primitives::num::math::atan2f(dx, dy)
            .to_degrees()
            .rem_euclid(360.0),
    )
}

/// A person (by its index among the placed objects) is used by the player of `world`: it turns to
/// face the player. Returns how long after the use's answer what the use makes it say and do comes:
/// none for an object or a person walking its route, a moment for one facing the player, the turn
/// for one that turns.
pub fn to_face<S: Store>(s: &mut Server<S>, index: usize, world: ConnId, clock: Clock) -> f64 {
    let Some(o) = s.content.objects.get(index) else {
        return 0.0;
    };
    let id = presence::content_id(&s.content.objects, index);
    let may = s.catalog.get(o.template).is_some_and(|t| turns(&t.props));
    let Some(user) = s.players.get(&world).map(|p| p.position) else {
        return 0.0;
    };
    if !may || s.walkers.by_id.contains_key(&id) {
        return 0.0;
    }
    let here = StoredPosition {
        cell: o.cell,
        origin: o.origin,
        rotation: o.rotation,
    };
    let off = bearing(&here, &user).map_or(0.0, |b| angle(degrees_of(o.rotation), b));
    let rate = s.catalog.still_turns.get(&o.template).copied();
    let (aims, wait) = match rate {
        Some(rate) if off > TURNS_PAST => (true, turn_seconds(off, rate)),
        _ => (false, FACING_SPEAKS_AFTER),
    };
    s.facing.turns = s.facing.turns.wrapping_add(1);
    let count = s.facing.turns;
    let turn = Turn {
        to: world,
        done: clock.local.0 + wait,
        aims,
        told: 0,
        count,
    };
    s.facing.turning.insert(id, turn);
    s.after(clock, FIRST_TOLD_AFTER, Timer::PersonTurns { id, count });
    wait
}

/// A person's place is told, as its turn goes: aimed at where its user is now while it turns, as it
/// stands if it doesn't; again after the next of [`TOLD_EVERY`] until its turn is done, and told once
/// more then, with the heading it stopped at.
pub fn step<S: Store>(s: &mut Server<S>, net: &mut ServerNet, id: u64, count: u32, clock: Clock) {
    let Some(&turn) = s.facing.turning.get(&id) else {
        return;
    };
    if turn.count != count {
        return;
    }
    // Found by its id: a reload may have moved its place in the list since the turn began.
    let Some(index) = presence::content_by_id(&s.content.objects, id) else {
        s.facing.turning.remove(&id);
        return;
    };
    let o = &s.content.objects[index];
    let here = StoredPosition {
        cell: o.cell,
        origin: o.origin,
        rotation: o.rotation,
    };
    let mut heading = crate::world::heading_of_rotation(o.rotation);
    if turn.aims {
        if let Some(b) = s
            .players
            .get(&turn.to)
            .and_then(|p| bearing(&here, &p.position))
        {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let byte = (b * 255.0 / 360.0).round() as u32 % 256;
            heading = u8::try_from(byte).unwrap_or(heading);
        }
    }
    place(s, net, id, index, heading, clock);
    if !turn.aims || clock.local.0 >= turn.done {
        s.facing.turning.remove(&id);
        return;
    }
    let every = TOLD_EVERY[(turn.told % 2) as usize];
    if let Some(t) = s.facing.turning.get_mut(&id) {
        t.told += 1;
    }
    s.after(clock, every, Timer::PersonTurns { id, count });
}

/// A person takes a new heading where it stands: kept as its content object's, and told to everyone
/// who sees it.
fn place<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    id: u64,
    index: usize,
    heading: u8,
    clock: Clock,
) {
    let time = net.game_time(clock.local);
    let instance_stamp = s.placed.instance(id);
    let stamp = s.facing.stamps.entry(id).or_insert(1);
    *stamp = stamp.wrapping_add(1);
    let position_stamp = *stamp;
    let Some(o) = s.content.objects.get_mut(index) else {
        return;
    };
    o.rotation = crate::world::rotation_of_heading(heading);
    let m = ObjectPosition {
        sender: ObjectStamp {
            id,
            instance_stamp,
            other_stamp: 0,
        },
        time,
        cell: o.cell,
        origin: auberean_primitives::Vec3::new(o.origin[0], o.origin[1], o.origin[2]),
        heading_motion: u32::from(heading) << 24 | crate::world::IDLE_MOTION,
        flags: ai::ON_GROUND,
        position_stamp,
        ..ObjectPosition::default()
    };
    presence::content_moves(s, net, index, &m, false);
}

/// The stamp a turned person's place was last told with, for a client it is made for.
#[must_use]
pub fn position_stamp<S: Store>(s: &Server<S>, id: u64) -> Option<u16> {
    s.facing.stamps.get(&id).copied()
}

/// What a player's use of a person says and does, once the person faces the player: the person
/// used (by its id, its place in the list perhaps moved by a reload since) answers through its
/// store or quest step, and a person a shifting row stands moves on once its step has run.
pub fn talks<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    id: u64,
    clock: Clock,
) {
    if !s.players.contains_key(&world) {
        return;
    }
    let Some(index) = presence::content_by_id(&s.content.objects, id) else {
        return;
    };
    let Some(o) = s.content.objects.get(index).cloned() else {
        return;
    };
    if crate::store::talk(s, net, world, id, &o, clock) {
        crate::shifting::used(s, net, index, clock);
    }
}
