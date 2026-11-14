//! A world's scheduled events ([`yalain_content::events`]) as the world runs them.
//!
//! **The clock.** At each world step the events on by the host's clock (UTC, seconds since 1970)
//! are worked out again; one that has come on starts, one that has gone off ends. The clock is the
//! host's: nothing here reads the time itself.
//!
//! **What an event gates.** A camp that names an event keeps no creatures while it is off: they
//! leave the world as it ends, and the camp fills as it starts. An object that names one stands
//! only while it is on: every client that sees it forgets it as it ends and is made it as it
//! starts; while it is off it is made for nobody, and a use of it or a give to it finds nothing. A
//! quest step that names events runs only while all of them are on. A line that names events is
//! said only while each is as it says, on or off ([`crate::talk`]).
//!
//! **Effects on players.** While an event is on, every player in the world holds each of its
//! effects, put on at spellcraft 1 (as an item's use with no spellcraft of its own puts one) and
//! lasting to the end of the event's window: it runs out as the event ends (within the second the
//! host's clock counts in), whether its holder is in the world then or not. A player who comes into the world while one is on is given it at the
//! next step; one who already holds a running record of the effect is given no second.
//!
//! **By the game's clock.** A window of the game's hours or of its day or night holds by the
//! world's calendar at the game time of the step: the share of the game's day passed, and the
//! night word of the part of the day it is in. A world with no calendar has no game times, and such
//! a window never holds. An effect it puts on lasts to the window's end, worked out from the day's
//! length.
//!
//! **Started by a tally.** An event a full tally starts ([`crate::tallies`]) is on for the window
//! the tally's fill gives it, as if its own windows said so; an event with no windows is on only
//! so.
//!
//! **Started by a quest's failure.** An event a quest's failure starts comes on at the step after
//! that quest fails for any player (a step fails it, its time runs out while its player is in the
//! world, or it fails with its player's death, a trip or a logout; a cancel is no failure), for
//! everyone, as if its own windows said so. It goes off at the first step after its `lasts` has
//! passed, or at the first step that finds the camp it names has made its creatures and keeps none
//! alive. A failure while it is on changes nothing. What a failure started is not kept across a
//! restart, and is forgotten when a reload changes the world's events.
//!
//! **Kept aside.** The placed objects an event hides are kept from the world while it is on: every
//! client that sees one forgets it as the event starts, and is made it again as the event ends
//! (unless something else keeps it from the world then); while it is on a use of it or a give to
//! it finds nothing.
//!
//! A world with no events does none of this.

use std::collections::{BTreeMap, BTreeSet};

use yalain_net::{ConnId, ServerNet};
use yalain_store::Store;

use crate::{presence, Clock, Server};

/// Which of the world's events are on, as the last step found them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Events {
    pub on: BTreeSet<String>,
    /// The events a quest's failure started, by their keys: when each came on and when its time
    /// runs out (seconds since 1970; `i64::MAX` for one only a camp's death ends).
    pub failed: BTreeMap<String, (i64, i64)>,
}

/// Is an event on?
#[must_use]
pub fn is_on<S: Store>(s: &Server<S>, key: &str) -> bool {
    s.events.on.contains(key)
}

/// May a camp keep its creatures now: it names no event, or its event is on.
#[must_use]
pub fn camp_stands<S: Store>(s: &Server<S>, camp: usize) -> bool {
    let t = &s.content.events;
    if t.is_empty() {
        return true;
    }
    s.spawns
        .camps
        .get(camp)
        .and_then(|c| t.of_camp(&c.id))
        .is_none_or(|e| is_on(s, e))
}

/// Is the placed object at this place in the world's list kept from the world now: it names an
/// event that is off, an event on hides it, an offering keeps it ([`crate::offerings::hides`]), a guarded row keeps it
/// ([`crate::guards::hides`]), a shifting row stands another of its objects
/// ([`crate::shifting::hides`]), or it is a chest gone once closed.
#[must_use]
pub fn hides<S: Store>(s: &Server<S>, object: usize) -> bool {
    let t = &s.content.events;
    (!t.is_empty()
        && (t.of_object(object).is_some_and(|e| !is_on(s, e))
            || t.hiding(object).any(|e| is_on(s, e))))
        || crate::offerings::hides(s, object)
        || crate::guards::hides(s, object)
        || crate::shifting::hides(s, object)
        || (!s.chests.gone.is_empty()
            && s.chests
                .gone
                .contains(&presence::content_id(&s.content.objects, object)))
}

/// The placed object of an id, unless it is kept from the world now.
#[must_use]
pub fn content_standing<S: Store>(s: &Server<S>, id: u64) -> Option<usize> {
    presence::content_by_id(&s.content.objects, id).filter(|i| !hides(s, *i))
}

/// May a quest step run now: every event it names is on.
#[must_use]
pub fn step_runs<S: Store>(s: &Server<S>, step: &yalain_content::quests::Step) -> bool {
    let t = &s.content.events;
    t.is_empty() || t.of_step(step).all(|e| is_on(s, e))
}

/// May a line be said now, by the events it names: each on, or off, as it says.
#[must_use]
pub fn line_for<S: Store>(s: &Server<S>, line: &yalain_content::talk::Line) -> bool {
    let t = &s.content.events;
    t.lines.is_empty() || t.of_line(line).all(|g| is_on(s, &g.event) == g.on)
}

/// The window of an event that holds `unix`: one of its own by the host's clock, else one of its
/// own by the game's, else one a full tally gave it, else one a quest's failure gave it.
fn window<S: Store>(
    s: &Server<S>,
    e: &yalain_content::events::Event,
    unix: i64,
) -> Option<(i64, i64)> {
    e.running(unix)
        .or_else(|| by_the_game(s, e, unix))
        .or_else(|| crate::tallies::started(s, &e.key, unix))
        .or_else(|| {
            s.events
                .failed
                .get(&e.key)
                .filter(|(from, until)| *from <= unix && unix < *until)
                .copied()
        })
}

/// A player's quest has failed: each event its failure starts that a failure has not started
/// already comes on at the next step, for its `lasts` (or until its camp's creatures are dead).
pub fn quest_failed<S: Store>(s: &mut Server<S>, quest: u32, unix: i64) {
    let due: Vec<(String, i64)> = s
        .content
        .events
        .failing(quest)
        .filter(|e| !s.events.failed.contains_key(&e.key))
        .filter_map(|e| {
            let f = e.failure?;
            let until = f.lasts.map_or(i64::MAX, |l| unix + i64::from(l));
            Some((e.key.clone(), until))
        })
        .collect();
    for (key, until) in due {
        s.events.failed.insert(key, (unix, until));
    }
}

/// What a quest's failure started that is over by `unix`: its time has run out, or the camp it
/// names has made its creatures and keeps none alive.
fn failures_over<S: Store>(s: &mut Server<S>, unix: i64) {
    if s.events.failed.is_empty() {
        return;
    }
    let over: Vec<String> = s
        .events
        .failed
        .iter()
        .filter(|(key, (_, until))| {
            unix >= *until
                || s.content.events.event(key).is_none_or(|e| {
                    e.failure
                        .and_then(|f| f.until_killed)
                        .is_some_and(|camp| crate::guards::camp_dead(s, camp))
                })
        })
        .map(|(key, _)| key.clone())
        .collect();
    for key in over {
        s.events.failed.remove(&key);
    }
}

/// The window of the game's clock an event is on in now, if it has one that holds: never in a
/// world with no calendar.
fn by_the_game<S: Store>(
    s: &Server<S>,
    e: &yalain_content::events::Event,
    unix: i64,
) -> Option<(i64, i64)> {
    if !e
        .windows
        .iter()
        .any(yalain_content::events::Window::by_the_game)
    {
        return None;
    }
    let mut clock = s.calendar.clone()?;
    clock.use_time(s.game_now);
    let now = yalain_content::events::GameNow {
        through: f64::from(clock.time_of_day()),
        day_length: f64::from(clock.day_length_seconds()),
        parts: &s.day_parts,
    };
    e.running_by_the_game(unix, &now)
}

/// Switch the events by the clock and the tallies, give the players in the world the effects of
/// those on, and reset the tallies due to.
pub fn step<S: Store>(s: &mut Server<S>, net: &mut ServerNet, clock: Clock) {
    if s.content.events.is_empty() {
        crate::tallies::step(s, net, &BTreeSet::new(), clock);
        return;
    }
    // The tallies are read before the events they start are worked out, and what a failure
    // started is over once its camp's creatures are dead.
    if !s.content.tallies.is_empty() {
        crate::tallies::ensure(s);
    }
    failures_over(s, clock.unix);
    let now: BTreeSet<String> = s
        .content
        .events
        .events
        .iter()
        .filter(|e| window(s, e, clock.unix).is_some())
        .map(|e| e.key.clone())
        .collect();
    let ended: Vec<String> = s.events.on.difference(&now).cloned().collect();
    let started: Vec<String> = now.difference(&s.events.on).cloned().collect();
    // What an ending event gates is gone before it is marked off, while it can still be found.
    for key in &ended {
        ends(s, net, key);
    }
    s.events.on = now;
    for key in &ended {
        unhidden(s, net, key);
    }
    for key in &started {
        starts(s, net, key);
    }
    give_effects(s, net, clock);
    let on = s.events.on.clone();
    crate::tallies::step(s, net, &on, clock);
}

/// Every event on ends, as the world's events are replaced.
pub fn end_all<S: Store>(s: &mut Server<S>, net: &mut ServerNet) {
    let on: Vec<String> = s.events.on.iter().cloned().collect();
    for key in &on {
        ends(s, net, key);
    }
    s.events.on.clear();
    s.events.failed.clear();
    for key in &on {
        unhidden(s, net, key);
    }
}

/// An event ends: its camps' creatures leave the world, and its objects are forgotten.
fn ends<S: Store>(s: &mut Server<S>, net: &mut ServerNet, key: &str) {
    for i in 0..s.spawns.camps.len() {
        if s.content.events.of_camp(&s.spawns.camps[i].id) != Some(key) {
            continue;
        }
        let Some(state) = s.camps.get_mut(i) else {
            continue;
        };
        let alive = std::mem::take(state);
        for id in alive.alive {
            crate::death::vanish(s, net, id);
        }
    }
    for i in gated_objects(s, key) {
        presence::content_gone(s, net, i);
    }
}

/// An event starts: its objects are made for every client that sees them (its camps fill at
/// their next step), and those it hides are forgotten.
fn starts<S: Store>(s: &mut Server<S>, net: &mut ServerNet, key: &str) {
    for i in gated_objects(s, key) {
        presence::content_placed(s, net, i, None);
    }
    for i in hidden_objects(s, key) {
        presence::content_gone(s, net, i);
    }
}

/// An event has ended: the objects it hid are made again for every client that sees them, unless
/// something else keeps them from the world now.
fn unhidden<S: Store>(s: &mut Server<S>, net: &mut ServerNet, key: &str) {
    for i in hidden_objects(s, key) {
        if !hides(s, i) {
            presence::content_placed(s, net, i, None);
        }
    }
}

/// The placed objects an event hides, by their places in the world's list.
fn hidden_objects<S: Store>(s: &Server<S>, key: &str) -> Vec<usize> {
    s.content
        .events
        .event(key)
        .map_or_else(Vec::new, |e| e.hides.clone())
}

/// The placed objects an event gates, by their places in the world's list.
fn gated_objects<S: Store>(s: &Server<S>, key: &str) -> Vec<usize> {
    let t = &s.content.events;
    (0..s.content.objects.len())
        .filter(|i| t.of_object(*i) == Some(key))
        .collect()
}

/// Each player in the world is given each effect of each event on that it holds no running record
/// of, lasting to the end of the event's window.
fn give_effects<S: Store>(s: &mut Server<S>, net: &mut ServerNet, clock: Clock) {
    let mut due: Vec<(u32, f32)> = Vec::new();
    for e in &s.content.events.events {
        let Some((_, until)) = window(s, e, clock.unix) else {
            continue;
        };
        #[allow(clippy::cast_precision_loss)]
        let left = (until - clock.unix) as f32;
        due.extend(e.on_players.iter().map(|state| (*state, left)));
    }
    if due.is_empty() {
        return;
    }
    let game_now = net.game_time(clock.local);
    let worlds: Vec<ConnId> = s.players.keys().copied().collect();
    for world in worlds {
        for &(state, left) in &due {
            let holds = s.players.get(&world).is_some_and(|p| {
                p.play.effects.iter().any(|r| {
                    r.state == state
                        && (r.timeout >= f32::MAX || r.cast + f64::from(r.timeout) > game_now)
                })
            });
            if holds {
                continue;
            }
            let Some(def) = s.effect_table.get(state).cloned() else {
                continue;
            };
            if !def.lasts() {
                continue;
            }
            let cast = crate::effects::Cast {
                spellcraft: 1.0,
                ..crate::effects::Cast::default()
            };
            let shape = crate::effects::Shape {
                flags: crate::effects::TIMED | crate::effects::record_flag::SPECIFIED,
                categories: None,
                timeout: Some(left),
                acting: 0,
            };
            crate::effects::add_on(
                s,
                net,
                crate::effects::On::Player(world),
                &def,
                cast,
                shape,
                clock,
            );
        }
    }
}
