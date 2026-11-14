//! Hazards ([`yalain_content::hazards`]) as the world keeps them: which reaches each player stands
//! within, so that a touch harms as the player comes within one and not again until it has left.
//!
//! **Used.** A use of a hazard's object harms its player once the player has come to it, after all
//! else the use does; a use a guarded or a shifting row refuses harms nothing. An object nothing
//! else makes usable is used as a person is: walked to, and its use answered.
//!
//! **Touched.** At each place a player reports, every reach of a row that harms at a touch is
//! measured from the player's feet to the row's object (one standing in the world now) or spot, by
//! straight-line distance; one the player has newly come within harms it. A player in portal space
//! is measured at its next report after it comes out. What it stands within is forgotten as it
//! leaves the world, and when a reload changes the rows.
//!
//! **Harm.** The row's effects are put on the player in order, cast by the object (by the player
//! itself for a spot) at their spellcraft; then its health falls by the row's harm, or all of it,
//! as the player's own harm: told with no one's name, and a death by it is one by misadventure. A
//! player already dead, or dying, is harmed by nothing.
//!
//! Nothing here is recorded: no 2013 or 2017 session shows a player harmed by an object. Which
//! objects and spots harm, how far they reach and what they do are the world's rows'.

use std::collections::{BTreeSet, HashMap};

use yalain_content::hazards::{Harm, Hazard};
use yalain_net::{ConnId, ServerNet};
use yalain_store::{Store, StoredPosition};

use crate::effects::{self, Cast};
use crate::{presence, Clock, Server};

/// The reaches each player stands within: a row (its place among the world's rows) and one of its
/// places (its objects in order, then its spots).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Hazards {
    pub within: HashMap<ConnId, BTreeSet<(usize, usize)>>,
}

/// A row's places it reaches from, each with its index among them and who casts its effects: its
/// objects standing in the world now, then its spots.
fn places<S: Store>(s: &Server<S>, r: &Hazard, me: u64) -> Vec<(usize, StoredPosition, u64)> {
    let objects = r.objects.iter().enumerate().filter_map(|(n, &i)| {
        let o = s.content.objects.get(i)?;
        if crate::events::hides(s, i) {
            return None;
        }
        let at = StoredPosition {
            cell: o.cell,
            origin: o.origin,
            rotation: [1.0, 0.0, 0.0, 0.0],
        };
        Some((n, at, presence::content_id(&s.content.objects, i)))
    });
    let spots = r.spots.iter().enumerate().map(|(n, (cell, origin))| {
        let at = StoredPosition {
            cell: *cell,
            origin: *origin,
            rotation: [1.0, 0.0, 0.0, 0.0],
        };
        (r.objects.len() + n, at, me)
    });
    objects.chain(spots).collect()
}

/// A player has reported its place: each reach of a row that harms at a touch it has newly come
/// within harms it.
pub fn moved<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, clock: Clock) {
    if s.content.hazards.is_empty() {
        return;
    }
    let Some(p) = s.players.get(&world) else {
        return;
    };
    if p.trip.is_some() {
        return;
    }
    let (at, me) = (p.position, p.character);
    let mut now = BTreeSet::new();
    let mut harms = Vec::new();
    for (k, r) in s.content.hazards.rows.iter().enumerate() {
        let Some(reach) = r.touch else {
            continue;
        };
        for (n, there, by) in places(s, r, me) {
            if presence::distance(&at, &there).is_some_and(|d| d <= reach) {
                now.insert((k, n));
                let before = s.hazards.within.get(&world);
                if before.is_none_or(|b| !b.contains(&(k, n))) {
                    harms.push((r.clone(), by));
                }
            }
        }
    }
    if now.is_empty() {
        s.hazards.within.remove(&world);
    } else {
        s.hazards.within.insert(world, now);
    }
    for (r, by) in harms {
        harm(s, net, world, r, by, clock);
    }
}

/// Does a row harm at a use of the placed object at this place in the world's list?
#[must_use]
pub fn harms_at_use<S: Store>(s: &Server<S>, object: usize) -> bool {
    let t = &s.content.hazards;
    !t.is_empty() && t.used(object).next().is_some()
}

/// A use of the placed object at this place in the world's list has been carried out: each row
/// that harms at a use of it harms the player.
pub fn used<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    object: usize,
    clock: Clock,
) {
    if s.content.hazards.is_empty() {
        return;
    }
    let by = presence::content_id(&s.content.objects, object);
    let rows: Vec<Hazard> = s.content.hazards.used(object).cloned().collect();
    for r in rows {
        harm(s, net, world, r, by, clock);
    }
}

/// A row harms a player: its effects, cast by `by`, then its harm.
fn harm<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    r: Hazard,
    by: u64,
    clock: Clock,
) {
    let harmable = |s: &Server<S>| {
        s.players
            .get(&world)
            .is_some_and(|p| p.dying.is_none() && !crate::combat::is_dead(p))
    };
    for (effect, spellcraft) in r.effects {
        if !harmable(s) {
            return;
        }
        let cast = Cast {
            caster: by,
            spellcraft,
            ..Cast::default()
        };
        effects::apply(s, net, world, effect, cast, clock);
    }
    if !harmable(s) {
        return;
    }
    if let Some(harm) = r.harm {
        let points = match harm {
            Harm::Points(points) => i32::try_from(points).unwrap_or(i32::MAX),
            Harm::All => i32::MAX,
        };
        effects::own_harm(s, net, world, points, clock);
    }
}

/// A player leaves the world: what it stood within is forgotten.
pub(crate) fn left<S: Store>(s: &mut Server<S>, world: ConnId) {
    s.hazards.within.remove(&world);
}
