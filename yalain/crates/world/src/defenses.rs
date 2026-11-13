//! Defended devices ([`yalain_content::defenses`]) as the world runs them: each device standing,
//! when it came, and the creatures it keeps about it.
//!
//! **Coming.** A creature of a device's template made anywhere (what an item's use sets down, a
//! camp's) starts its defense as it is made: it fights no one from then on, and at the next world
//! step its row's `population` of attackers appear about it.
//!
//! **Attacked.** Each attacker appears `radius` meters from the device, in a direction picked at
//! random, on the ground, and goes at it at once, as a creature goes at another; one at rest with
//! its device standing within its perception goes at it again before any player. An attacker
//! killed (or gone) is replaced its row's `regen` seconds after, or its device's template's
//! generator period when the row names none, as long as the device stands.
//!
//! **Held.** At the first step after it has stood its `holds` seconds the device goes (no fall, no
//! corpse) and what its row names in its place is made where it stood: a portal leading where its
//! template names, a creature, or another object no one's, for the row's `then_lasts` seconds. The
//! attackers it made stay until killed. A device killed first, or gone, ends its defense and leaves
//! nothing; its attackers stay.
//!
//! **Said.** The device says its row's lines aloud of itself, heard by every client that knows it:
//! each timed line once its time after the device came has come (0: at the step it comes), its
//! line for being badly hurt once as its health first falls below half, and its line for holding
//! just before it goes.
//!
//! Nothing of this is kept across a restart. Nothing is recorded: no device of a defense was made
//! in any recording; the rows say what attacks it and for how long it must stand.

use std::collections::BTreeMap;

use dereth_primitives::num::math;
use yalain_net::ServerNet;
use yalain_store::{Store, StoredPosition};

use crate::{presence, Clock, Server};

/// One device's defense under way.
#[derive(Debug, Clone, PartialEq)]
pub struct Running {
    /// Its row, by its place among the rows.
    pub row: usize,
    /// When it came (local seconds).
    pub started: f64,
    /// The attackers it made that are alive.
    pub attackers: Vec<u64>,
    /// When each replacement is due.
    pub due: Vec<f64>,
    /// Its first attackers are made.
    pub filled: bool,
    /// How many of its row's timed lines it has said.
    pub said: usize,
    /// It has said its line for being badly hurt.
    pub hurt: bool,
}

/// The defenses under way, by the device's id.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Defenses {
    pub running: BTreeMap<u64, Running>,
}

/// The replacement period for a row that names none and a device template that names no generator
/// period either, in seconds.
pub const DEFAULT_REGEN: f64 = 30.0;

/// Is a thing a creature standing: alive, and not fallen?
fn standing(t: &crate::things::Thing) -> bool {
    t.alive()
        && t.creature
            .as_ref()
            .is_some_and(|c| c.mode != crate::things::Mode::Dead)
}

/// A creature has been made: one of a device's template starts its defense, and fights no one.
pub fn made<S: Store>(s: &mut Server<S>, id: u64, template: u32, now: f64) {
    let Some(row) = s.content.defenses.of_device(template) else {
        return;
    };
    if let Some(c) = s.things.get_mut(id).and_then(|t| t.creature.as_mut()) {
        c.stats.fights = false;
    }
    s.defenses.running.insert(
        id,
        Running {
            row,
            started: now,
            attackers: Vec::new(),
            due: Vec::new(),
            filled: false,
            said: 0,
            hurt: false,
        },
    );
}

/// The device a creature a defense made goes at, if it still stands within `perception` of `at`.
#[must_use]
pub fn device_of<S: Store>(
    s: &Server<S>,
    id: u64,
    at: &StoredPosition,
    perception: f32,
) -> Option<u64> {
    if s.defenses.running.is_empty() {
        return None;
    }
    let (&device, _) = s
        .defenses
        .running
        .iter()
        .find(|(_, r)| r.attackers.contains(&id))?;
    let place = s.things.get(device).filter(|t| standing(t))?.at?;
    presence::distance(at, &place)
        .is_some_and(|d| d <= perception)
        .then_some(device)
}

/// At each world step: each defense under way ends with its device gone, or holds once it has
/// stood its time, or makes the attackers it lacks that are due.
pub fn step<S: Store>(s: &mut Server<S>, net: &mut ServerNet, clock: Clock) {
    if s.defenses.running.is_empty() {
        return;
    }
    let now = clock.local.0;
    let devices: Vec<u64> = s.defenses.running.keys().copied().collect();
    for device in devices {
        let Some(run) = s.defenses.running.get(&device).cloned() else {
            continue;
        };
        let Some(row) = s.content.defenses.rows.get(run.row).cloned() else {
            s.defenses.running.remove(&device);
            continue;
        };
        let Some(place) = s
            .things
            .get(device)
            .filter(|t| standing(t))
            .and_then(|t| t.at)
        else {
            s.defenses.running.remove(&device);
            continue;
        };
        if now - run.started >= row.holds {
            s.defenses.running.remove(&device);
            say(s, net, device, row.held_line);
            held(s, net, device, &row, place, clock);
            continue;
        }
        lines(s, net, device, &row, &run, now);
        fill(s, net, device, &row, place, clock);
    }
}

/// The lines a device says now: those of its row's timed lines whose time has come, and its line
/// for being badly hurt as its health first falls below half.
fn lines<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    device: u64,
    row: &yalain_content::defenses::Defense,
    run: &Running,
    now: f64,
) {
    let due: Vec<(u32, u32)> = row
        .says
        .iter()
        .skip(run.said)
        .take_while(|(at, _)| now - run.started >= *at)
        .map(|(_, line)| *line)
        .collect();
    let hurt = !run.hurt
        && s.things
            .get(device)
            .is_some_and(|t| t.health * 2 < t.max_health);
    if let Some(r) = s.defenses.running.get_mut(&device) {
        r.said += due.len();
        r.hurt |= hurt;
    }
    for line in due {
        say(s, net, device, Some(line));
    }
    if hurt {
        say(s, net, device, row.hurt_line);
    }
}

/// A device says a line of itself aloud, if it has one.
fn say<S: Store>(s: &mut Server<S>, net: &mut ServerNet, device: u64, line: Option<(u32, u32)>) {
    let (Some((string, table)), Some(speaker)) = (line, crate::talk::thing_speaker(s, device))
    else {
        return;
    };
    let words = yalain_content::talk::Words::Ref(string, table);
    crate::talk::said_aloud(s, net, &speaker, &words);
}

/// The attackers a device lacks that are due are made about it, each going at it.
fn fill<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    device: u64,
    row: &yalain_content::defenses::Defense,
    place: StoredPosition,
    clock: Clock,
) {
    let now = clock.local.0;
    let regen = row.regen.unwrap_or_else(|| {
        let template = s.things.get(device).map_or(0, |t| t.template);
        s.catalog
            .get(template)
            .and_then(|t| t.props.float("GeneratorRegenPeriod"))
            .map_or(DEFAULT_REGEN, f64::from)
    });
    let things = &s.things;
    let Some(run) = s.defenses.running.get_mut(&device) else {
        return;
    };
    let lost: Vec<u64> = run
        .attackers
        .iter()
        .copied()
        .filter(|id| !things.get(*id).is_some_and(standing))
        .collect();
    for id in lost {
        run.attackers.retain(|x| *x != id);
        run.due.push(now + regen);
    }
    let population = row.population as usize;
    let mut make = 0;
    if run.filled {
        run.due.sort_by(f64::total_cmp);
        while run.attackers.len() + make < population && run.due.first().is_some_and(|t| *t <= now)
        {
            run.due.remove(0);
            make += 1;
        }
    } else {
        run.filled = true;
        make = population;
    }
    for _ in 0..make {
        if let Some(id) = attacker(s, net, row, &place, clock) {
            if let Some(run) = s.defenses.running.get_mut(&device) {
                run.attackers.push(id);
            }
            crate::ai::take_on(s, net, id, device, clock);
        }
    }
}

/// One attacker made `radius` meters from the device, in a direction picked at random.
fn attacker<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    row: &yalain_content::defenses::Defense,
    place: &StoredPosition,
    clock: Clock,
) -> Option<u64> {
    let template = crate::makes::pick_one(s, &row.spawns)?;
    let template = s.catalog.get(template).cloned()?;
    let angle = s.rng.between(0.0, std::f32::consts::TAU);
    let offset = [
        row.radius * math::cosf(angle),
        row.radius * math::sinf(angle),
    ];
    let at = crate::death_spawn::at_offset(s, place, offset);
    crate::death_spawn::creature_at(s, net, &template, at, None, clock)
}

/// A device has held: it goes, and what its row names stands where it stood.
fn held<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    device: u64,
    row: &yalain_content::defenses::Defense,
    place: StoredPosition,
    clock: Clock,
) {
    crate::death::vanish(s, net, device);
    let Some(then) = row.then else {
        return;
    };
    let Some(template) = s.catalog.get(then).cloned() else {
        return;
    };
    #[allow(clippy::cast_precision_loss)]
    let seconds = row.then_lasts.map(|t| t as f32);
    if crate::usable::portals::is_portal(&template.props) {
        crate::usable::portals::made(s, net, (then, None), place, (None, seconds), clock);
    } else if yalain_content::CreatureStats::from_props(&template.props).is_some() {
        crate::death_spawn::creature_at(s, net, &template, place, seconds, clock);
    } else {
        crate::placing::place_at(s, net, (then, place), row.then_lasts, clock);
    }
}
