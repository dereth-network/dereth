//! Shifting objects ([`yalain_content::shifting`]) as the world keeps them: which of each row's
//! objects stands now and since when, and which comes next after a use.
//!
//! **One at a time.** At the world's first step after its rows are given, each row stands one of
//! its objects, picked at random, for everyone who sees its place; until then none stands.
//!
//! **Moving on.** A use of the standing object carried out (for one a quest step makes a person, a
//! use whose step ran) forgets it for everyone at once, and another of the row's objects, picked at
//! random among the rest, is made for everyone who sees its place: at once, or at the first step
//! after the row's time gone. A row that names how long one stands moves it on so at the first step
//! after that long unused. A use of an object of a row that does not stand is answered with nothing
//! done. Nothing of this is kept across a restart; a reload that changes the rows forgets what
//! stood and stands one again.
//!
//! Nothing here is recorded: the 2017 sessions saw each of the remains this was made for once, at
//! a different one of its places, never two in one view. How long none stands and how long one
//! lies unused are the world's rows'.

use std::collections::BTreeMap;

use yalain_net::{ConnId, ServerNet};
use yalain_store::Store;

use crate::places::{self, Use};
use crate::{presence, progress, Clock, Server};

/// What the world keeps of its shifting rows, by each row's key: the object standing now (its place
/// in the world's list) and when it came (local seconds); or, after a use, the object that comes
/// next and when.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Shifts {
    pub standing: BTreeMap<String, (usize, f64)>,
    pub coming: BTreeMap<String, (usize, f64)>,
}

/// Is the placed object at this place in the world's list kept from the world now by a shifting
/// row: one of its objects that is not the one standing?
#[must_use]
pub fn hides<S: Store>(s: &Server<S>, object: usize) -> bool {
    let t = &s.content.shifting;
    if t.is_empty() {
        return false;
    }
    t.of_object(object)
        .is_some_and(|r| s.shifts.standing.get(&r.key).map(|(o, _)| *o) != Some(object))
}

/// A use of an object a shifting row keeps from the world (a client that still held it): answered
/// with nothing done. Returns whether it was one.
pub fn refused<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    u: &Use,
    object: usize,
) -> bool {
    if !hides(s, object) {
        return false;
    }
    let (cell, origin) = {
        let o = &s.content.objects[object];
        (o.cell, o.origin)
    };
    let me = s.players.get(&world).map_or(0, |p| p.character);
    let distance = places::reach(&u.from, cell, origin).unwrap_or(0.0);
    let done = places::use_done(u, me, places::USE_FLAGS_PERSON, false, distance);
    progress::private(net, world, &done);
    true
}

/// A use of the placed object at this place in the world's list has been carried out: if it is
/// the one its row stands, it goes, and another comes in its place, at once or after the row's
/// time gone.
pub fn used<S: Store>(s: &mut Server<S>, net: &mut ServerNet, object: usize, clock: Clock) {
    let Some(r) = s.content.shifting.of_object(object).cloned() else {
        return;
    };
    if s.shifts.standing.get(&r.key).map(|(o, _)| *o) != Some(object) {
        return;
    }
    moves_on(s, net, &r, object, r.gone_for, clock);
}

/// The object standing for a row goes; another, never the same, comes after `gone_for` seconds (at
/// once when none).
fn moves_on<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    r: &yalain_content::shifting::Shifting,
    was: usize,
    gone_for: Option<f64>,
    clock: Clock,
) {
    s.shifts.standing.remove(&r.key);
    presence::content_gone(s, net, was);
    let others: Vec<usize> = r.objects.iter().copied().filter(|o| *o != was).collect();
    if others.is_empty() {
        return;
    }
    let next = others[s.rng.below(others.len())];
    let now = clock.local.0;
    match gone_for {
        Some(t) => {
            s.shifts.coming.insert(r.key.clone(), (next, now + t));
        }
        None => stands(s, net, &r.key, next, now),
    }
}

/// A row's object stands now: made for everyone who sees its place.
fn stands<S: Store>(s: &mut Server<S>, net: &mut ServerNet, key: &str, object: usize, now: f64) {
    s.shifts.standing.insert(key.to_string(), (object, now));
    if !crate::events::hides(s, object) {
        presence::content_placed(s, net, object, None);
    }
}

/// At each world step: a row with nothing standing or coming stands one of its objects; one whose
/// next is due stands it; one whose object has stood unused its time moves it on.
pub fn step<S: Store>(s: &mut Server<S>, net: &mut ServerNet, clock: Clock) {
    if s.content.shifting.is_empty() {
        return;
    }
    let now = clock.local.0;
    let rows = s.content.shifting.rows.clone();
    for r in &rows {
        if r.objects.is_empty() {
            continue;
        }
        if let Some(&(object, since)) = s.shifts.standing.get(&r.key) {
            if r.moves_after.is_some_and(|t| now - since >= t) {
                moves_on(s, net, r, object, None, clock);
            }
        } else if let Some(&(next, at)) = s.shifts.coming.get(&r.key) {
            if at <= now {
                s.shifts.coming.remove(&r.key);
                stands(s, net, &r.key, next, now);
            }
        } else {
            let first = r.objects[s.rng.below(r.objects.len())];
            stands(s, net, &r.key, first, now);
        }
    }
}

/// The rows given anew: what stood is forgotten by everyone, and each row stands one again at the
/// next step.
pub fn forget_all<S: Store>(s: &mut Server<S>, net: &mut ServerNet) {
    let standing: Vec<usize> = s.shifts.standing.values().map(|(o, _)| *o).collect();
    for o in standing {
        presence::content_gone(s, net, o);
    }
    s.shifts = Shifts::default();
}
