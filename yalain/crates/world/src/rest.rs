//! Where no player is near, the world rests: the step's work follows the players, not the size of
//! the world.
//!
//! **Near** is a player's landblock or one beside it. That is farther than anything a client is
//! told of ([`crate::presence`]: three land cells outdoors, the landblock indoors), and as far as
//! anything a creature notices or does about a player reaches ([`crate::presence::distance`]
//! measures nothing further).
//!
//! **Creatures asleep.** Where the walls are known (as they are to a running server), a creature at
//! rest (in its peaceful mode, not walking off, not about to join a fight, not sent at a defended
//! device, whole and done regaining health, its vigor full), with no player near its place or its
//! home, sleeps: the step does not act it. Awake it would have done nothing anyone sees: it would notice no one,
//! walk nowhere (where the walls are known, a creature no player could see rests:
//! [`crate::ai`]), and play its idle behaviors to no client. A player coming near wakes it, and so does anything that
//! changes it (a hurt, a fight its kin call it to, an effect coming and going).
//!
//! **Its clocks.** Woken, a creature carries on as if it had been awake all along. It looks about
//! for players at its own moment of each second, as before. Its next idle behavior comes after a
//! fresh wait, drawn as every such wait is (the wait has no memory: one already partly run is as
//! long again as a new one). Its next thought of wandering comes after what is left of a rest
//! between walks taken at a random moment.
//!
//! Camps rest too: [`crate::spawn`] looks at a camp only when it has something to do.

use std::collections::BTreeSet;

use yalain_store::{Store, StoredPosition};

use crate::things::{Mode, Thing};
use crate::Server;

/// A place's landblock.
#[must_use]
pub fn block(at: &StoredPosition) -> u16 {
    #[allow(clippy::cast_possible_truncation)]
    let b = (at.cell >> 16) as u16;
    b
}

/// The landblocks near a player: each one's own and those beside it.
#[must_use]
pub fn near_blocks<S: Store>(s: &Server<S>) -> BTreeSet<u16> {
    let mut near = BTreeSet::new();
    for p in s.players.values() {
        let b = block(&p.position);
        let (x, y) = (i32::from(b >> 8), i32::from(b & 0xFF));
        for nx in (x - 1).max(0)..=(x + 1).min(0xFF) {
            for ny in (y - 1).max(0)..=(y + 1).min(0xFF) {
                #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
                near.insert(((nx as u16) << 8) | ny as u16);
            }
        }
    }
    near
}

/// The creatures asleep near a player wake, and every creature woken since the last step takes up
/// its clocks.
pub fn wake<S: Store>(s: &mut Server<S>, near: &BTreeSet<u16>, now: f64) {
    for b in near {
        s.things.wake_block(*b);
    }
    let mut woken = s.things.take_woken();
    woken.sort_unstable();
    woken.dedup();
    for id in woken {
        take_up_clocks(s, id, now);
    }
}

/// A creature woken carries on as if it had been awake: it looks about at its own moment of the
/// second, its idle behavior and its thought of wandering come after waits drawn as an awake
/// creature's fall at a random moment, and no player is within its perception since its last step.
fn take_up_clocks<S: Store>(s: &mut Server<S>, id: u64, now: f64) {
    let Some((idle_due, wander_due, template)) = s.things.get(id).and_then(|t| {
        t.creature
            .as_ref()
            .map(|c| (c.next_idle < now, c.next_wander < now, t.template))
    }) else {
        return;
    };
    let idle = idle_due.then(|| now + crate::ai::idle_gap(s, template));
    let wander = wander_due.then(|| now + rest_left(s));
    let Some(c) = s.things.get_mut(id).and_then(|t| t.creature.as_mut()) else {
        return;
    };
    if c.next_look < now {
        let behind = now - c.next_look;
        c.next_look += (behind / crate::ai::LOOK_EVERY).ceil() * crate::ai::LOOK_EVERY;
    }
    if let Some(t) = idle {
        c.next_idle = t;
    }
    if let Some(t) = wander {
        c.next_wander = t;
    }
    c.in_sight.clear();
}

/// What is left of a creature's rest between walks at a moment drawn at random among its rests.
/// The rests run evenly from [`crate::ai::WANDER_REST`]'s least to its most, so the moment falls in
/// a rest's last `x` seconds as often as rests are longer than `x`: evenly up to the least rest,
/// then less and less often up to the longest.
fn rest_left<S: Store>(s: &mut Server<S>) -> f64 {
    let (least, most) = crate::ai::WANDER_REST;
    let (least, most) = (f64::from(least), f64::from(most));
    let mean = (least + most) / 2.0;
    let u = f64::from(s.rng.unit()).clamp(0.0, 1.0);
    let even = least / mean;
    if u < even {
        u * mean
    } else {
        let v = ((u - even) / (1.0 - even)).clamp(0.0, 1.0);
        most - (most - least) * (1.0 - v).sqrt()
    }
}

/// Where the walls are known, every creature awake that rests, with no player near its place or its
/// home, goes to sleep.
pub fn sleep_resting<S: Store>(s: &mut Server<S>, near: &BTreeSet<u16>) {
    if s.walls.is_none() {
        return;
    }
    let resting: Vec<(u64, [u16; 2])> = s
        .things
        .awake()
        .filter_map(|id| {
            let t = s.things.get(id)?;
            let blocks = blocks_of(t)?;
            (rests(s, t) && !blocks.iter().any(|b| near.contains(b))).then_some((id, blocks))
        })
        .collect();
    for (id, blocks) in resting {
        s.things.sleep(id, blocks);
    }
}

/// The landblocks a player coming near wakes a creature in: its place's and its home's.
fn blocks_of(t: &Thing) -> Option<[u16; 2]> {
    let at = t.at?;
    let home = t.creature.as_ref().map_or(at, |c| c.home);
    Some([block(&at), block(&home)])
}

/// Does a creature rest: alive, no pet, whole and done regaining health, its vigor full, in its
/// peaceful mode, neither walking off, nor at a waypoint it is to leave, nor about to join a fight,
/// nor sent at a defended device?
fn rests<S: Store>(s: &Server<S>, t: &Thing) -> bool {
    if !t.alive()
        || s.pets.is_pet(t.id)
        || t.health < t.max_health
        || t.vigor < t.max_vigor
        || t.regen_since != 0.0
        || t.regained != 0
    {
        return false;
    }
    let Some(c) = t.creature.as_ref() else {
        return true;
    };
    let at_waypoint = c.patrol.as_ref().is_some_and(|p| {
        p.goal()
            .zip(t.at)
            .is_some_and(|(g, at)| crate::ai::arrived(&at, &g))
    });
    c.mode == Mode::Idle
        && c.joining.is_none()
        && c.wander_to.is_none()
        && !at_waypoint
        && !crate::defenses::attacks(s, t.id)
}
