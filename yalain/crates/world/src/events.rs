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
//! quest step that names events runs only while all of them are on.
//!
//! **Effects on players.** While an event is on, every player in the world holds each of its
//! effects, put on at spellcraft 1 (as an item's use with no spellcraft of its own puts one) and
//! lasting to the end of the event's window: it runs out as the event ends (within the second the
//! host's clock counts in), whether its holder is in the world then or not. A player who comes into the world while one is on is given it at the
//! next step; one who already holds a running record of the effect is given no second.
//!
//! A world with no events does none of this.

use std::collections::BTreeSet;

use yalain_net::{ConnId, ServerNet};
use yalain_store::Store;

use crate::{presence, Clock, Server};

/// Which of the world's events are on, as the last step found them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Events {
    pub on: BTreeSet<String>,
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

/// Is a placed object kept from the world now: it names an event that is off.
#[must_use]
pub fn hides<S: Store>(s: &Server<S>, o: &yalain_content::ContentObject) -> bool {
    let t = &s.content.events;
    !t.is_empty()
        && t.of_object(o.template, o.cell, o.origin)
            .is_some_and(|e| !is_on(s, e))
}

/// The placed object of an id, unless it is kept from the world now.
#[must_use]
pub fn content_standing<S: Store>(s: &Server<S>, id: u64) -> Option<usize> {
    presence::content_by_id(&s.content.objects, id).filter(|i| !hides(s, &s.content.objects[*i]))
}

/// May a quest step run now: every event it names is on.
#[must_use]
pub fn step_runs<S: Store>(s: &Server<S>, step: &yalain_content::quests::Step) -> bool {
    let t = &s.content.events;
    t.is_empty() || t.of_step(step).all(|e| is_on(s, e))
}

/// Switch the events by the clock, and give the players in the world the effects of those on.
pub fn step<S: Store>(s: &mut Server<S>, net: &mut ServerNet, clock: Clock) {
    if s.content.events.is_empty() {
        return;
    }
    let now: BTreeSet<String> = s
        .content
        .events
        .events
        .iter()
        .filter(|e| e.running(clock.unix).is_some())
        .map(|e| e.key.clone())
        .collect();
    let ended: Vec<String> = s.events.on.difference(&now).cloned().collect();
    let started: Vec<String> = now.difference(&s.events.on).cloned().collect();
    // What an ending event gates is gone before it is marked off, while it can still be found.
    for key in &ended {
        ends(s, net, key);
    }
    s.events.on = now;
    for key in &started {
        starts(s, net, key);
    }
    give_effects(s, net, clock);
}

/// Every event on ends, as the world's events are replaced.
pub fn end_all<S: Store>(s: &mut Server<S>, net: &mut ServerNet) {
    let on: Vec<String> = s.events.on.iter().cloned().collect();
    for key in &on {
        ends(s, net, key);
    }
    s.events.on.clear();
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
/// their next step).
fn starts<S: Store>(s: &mut Server<S>, net: &mut ServerNet, key: &str) {
    for i in gated_objects(s, key) {
        presence::content_placed(s, net, i, None);
    }
}

/// The placed objects an event gates, by their places in the world's list.
fn gated_objects<S: Store>(s: &Server<S>, key: &str) -> Vec<usize> {
    let t = &s.content.events;
    s.content
        .objects
        .iter()
        .enumerate()
        .filter(|(_, o)| t.of_object(o.template, o.cell, o.origin) == Some(key))
        .map(|(i, _)| i)
        .collect()
}

/// Each player in the world is given each effect of each event on that it holds no running record
/// of, lasting to the end of the event's window.
fn give_effects<S: Store>(s: &mut Server<S>, net: &mut ServerNet, clock: Clock) {
    let mut due: Vec<(u32, f32)> = Vec::new();
    for e in &s.content.events.events {
        let Some((_, until)) = e.running(clock.unix) else {
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
