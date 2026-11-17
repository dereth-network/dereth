//! Rounds ([`yalain_content::rounds`]) as the world keeps them: which stop each row's creature
//! stands at, and the creature; or, after a death, which comes next and when.
//!
//! **One at a time.** At the world's first step after its rows are given, each row's first stop's
//! creature stands at its place, made as a camp makes one but kept by no camp; outdoors, on the
//! ground under the place, as a camp's creature stands. When it dies, the
//! next stop's creature stands at that stop's place (the first's after the last): the one the stop
//! names for its killer's kingdom, where it names one and the killer is a player of a kingdom, else
//! the stop's own; at once, or at the first step after the row's gap. A killer the creature could
//! not reach changes no kingdom: the next stands for the kingdom the dead one stood for. A creature taken from the
//! world otherwise than by a death stands again where it stood. Nothing of this is kept across a
//! restart; a reload that changes the rows takes their creatures away and starts each row again.

use std::collections::BTreeMap;

use yalain_net::ServerNet;
use yalain_store::{Store, StoredPosition};

use crate::{Clock, Server};

/// What the world keeps of its rounds, by each row's key: the stop its creature stands at and the
/// creature; or, after a death, the stop that comes next, its creature's template and when (local
/// seconds).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Rounds {
    pub standing: BTreeMap<String, (usize, u64)>,
    pub coming: BTreeMap<String, (usize, u32, f64)>,
}

/// At each world step: a row with nothing standing or coming stands its first stop's creature; one
/// whose next is due stands it; one whose creature is gone from the world without a death stands it
/// again where it stood.
pub fn step<S: Store>(s: &mut Server<S>, net: &mut ServerNet, clock: Clock) {
    if s.content.rounds.is_empty() {
        return;
    }
    let now = clock.local.0;
    let rows = s.content.rounds.rows.clone();
    for r in &rows {
        let Some(first) = r.stops.first() else {
            continue;
        };
        if let Some(&(stop, id)) = s.rounds.standing.get(&r.key) {
            if s.things.get(id).is_none() {
                s.rounds.standing.remove(&r.key);
                stands(s, net, (&r.key, stop), r.stops[stop].template, clock);
            }
        } else if let Some(&(stop, template, at)) = s.rounds.coming.get(&r.key) {
            if at <= now {
                s.rounds.coming.remove(&r.key);
                stands(s, net, (&r.key, stop), template, clock);
            }
        } else {
            stands(s, net, (&r.key, 0), first.template, clock);
        }
    }
}

/// The kingdom a round's standing creature stands for: the one its stop names it for; none for the
/// stop's own creature, or a creature no round keeps.
#[must_use]
pub fn kingdom_of<S: Store>(s: &Server<S>, creature: u64) -> Option<u32> {
    let (key, stop) = s
        .rounds
        .standing
        .iter()
        .find(|(_, (_, id))| *id == creature)
        .map(|(k, (stop, _))| (k, *stop))?;
    let template = s.things.get(creature)?.template;
    let r = s.content.rounds.rows.iter().find(|r| r.key == *key)?;
    r.stops
        .get(stop)?
        .by_kingdom
        .iter()
        .find(|(_, t)| *t == template)
        .map(|(k, _)| *k)
}

/// A creature died: if it is a round's, the next stop's creature for `kingdom` (its killer's, or
/// none) comes, at once or after the row's gap.
pub fn died<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    creature: u64,
    kingdom: Option<u32>,
    clock: Clock,
) {
    if s.rounds.standing.is_empty() {
        return;
    }
    let Some(key) = s
        .rounds
        .standing
        .iter()
        .find(|(_, (_, id))| *id == creature)
        .map(|(k, _)| k.clone())
    else {
        return;
    };
    let Some((stop, _)) = s.rounds.standing.remove(&key) else {
        return;
    };
    let Some(r) = s.content.rounds.rows.iter().find(|r| r.key == key).cloned() else {
        return;
    };
    let next = (stop + 1) % r.stops.len();
    let template = r.stops[next].template_for(kingdom);
    match r.gap {
        Some(gap) => {
            s.rounds
                .coming
                .insert(key, (next, template, clock.local.0 + gap));
        }
        None => stands(s, net, (&key, next), template, clock),
    }
}

/// A row's creature of `template` stands at a stop's place, facing as the place does.
fn stands<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    (key, stop): (&str, usize),
    template: u32,
    clock: Clock,
) {
    let Some(place) = s
        .content
        .rounds
        .rows
        .iter()
        .find(|r| r.key == key)
        .and_then(|r| r.stops.get(stop))
        .map(|st| st.at)
    else {
        return;
    };
    let Some(t) = s.catalog.get(template).cloned() else {
        s.unhandled.note(format!(
            "a round's template {template:08X} is not in the install"
        ));
        return;
    };
    let mut at = StoredPosition {
        cell: place.cell,
        origin: place.origin,
        rotation: place.rotation,
    };
    if !crate::presence::indoors(at.cell) {
        if let Some(z) = s.ground.as_ref().and_then(|g| g.height(at.cell, at.origin)) {
            at.origin[2] = z;
        }
    }
    match crate::death_spawn::creature_at(s, net, &t, at, None, None, clock) {
        Some(id) => {
            s.rounds.standing.insert(key.to_string(), (stop, id));
        }
        None => s
            .unhandled
            .note(format!("a round's template {template:08X} is no creature")),
    }
}

/// The rows given anew: their creatures go, and each row starts again at the next step.
pub fn forget_all<S: Store>(s: &mut Server<S>, net: &mut ServerNet) {
    let standing: Vec<u64> = s.rounds.standing.values().map(|(_, id)| *id).collect();
    for id in standing {
        crate::death_spawn::ends(s, net, id);
    }
    s.rounds = Rounds::default();
}
