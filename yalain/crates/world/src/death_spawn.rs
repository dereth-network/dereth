//! What a creature's death does to its killer beyond its quests: an effect that lasts goes on the
//! killer (2017: the Illuminated Drudge's curse on 4 of 4 killers who recorded the kill, at the death,
//! from the creature, for its whole duration, and on none of 1 who watched), and a world's
//! [`yalain_content::DeathSpawn`] makes creatures around the killer.
//!
//! **Where they appear.** Each stands at its offset from the killer, as the killer faces, on the
//! ground (2017: the Drunken Drudge Sentry's two, 2 of the 3 tries at 70%, both exactly on their
//! offsets about the killer at one heading, and neither about the corpse; one kill). A killer who
//! stands far from the death makes nothing: the kill goes to who did the most damage, wherever that
//! one has gone since, and what a death makes belongs where it died.
//!
//! They stay until killed (the install's effect says they are permanent), or for the time their row
//! gives. A world may let many such kills pass with no one to fight what they make, so no more than
//! [`MOST`] of one effect's made by kills live at once.
//!
//! **An item's use** may name such an effect too: what it makes stands before its user as the use
//! is done (2017: 4 of 4 practice gems used by the player recording it, a dummy of the gem's own tier
//! 2 m ahead at its heading, within 0.05 m, with the use's answer and the gem gone). The limit is
//! none of its business: an item is used up for what it makes, and what others have made doesn't
//! take that from its user. What a use makes takes an item each time, so it is held to the items
//! there are.
//!
//! **A portal.** A row whose template names a destination makes a portal there, standing as
//! [`portals`] says; one that names a place of its own to lead to leads there instead.
//!
//! **A creature's own row.** A row may be of the creatures of a template rather than of a death
//! effect: the death of any of them makes what it names, wherever the killer stands, at the row's
//! place (facing as the place does), else at its offset from where the creature died, as it faced.
//! Made at a place, one of the row's stands at a time: a death while it stands makes none.

use dereth_primitives::num::math;
use yalain_net::{ConnId, ServerNet};
use yalain_store::{Store, StoredPosition};

use crate::effect_defs::Kind;
use crate::effects::{Cast, On};
use crate::things::Mode;
use crate::usable::portals;
use crate::{presence, Clock, Server};

/// The most of one death effect's that live at once.
pub const MOST: usize = 24;

/// How far from a death its killer may stand for what the death makes to appear, in meters: half as
/// far again as a missile reaches. (The one kill recorded had its killer at the creature's side.)
pub const NEAR_THE_DEATH: f32 = crate::missiles::MISSILE_RANGE * 1.5;

/// What a creature's death does to its killer, beyond its quests.
pub fn killed<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    killer: ConnId,
    creature: u64,
    template: u32,
    effects: &[(String, u32)],
    clock: Clock,
) {
    let lasting: Vec<(u32, f32)> = s
        .death_effects
        .get(&template)
        .into_iter()
        .flatten()
        .filter(|e| {
            s.effect_table
                .get(e.state)
                .is_some_and(|d| d.lasts() && !matches!(d.kind, Kind::Quest { .. }))
        })
        .map(|e| (e.state, e.spellcraft))
        .collect();
    for (state, spellcraft) in lasting {
        let cast = Cast {
            caster: creature,
            spellcraft,
            ..Cast::default()
        };
        crate::effects::apply_on(s, net, On::Player(killer), state, cast, clock);
    }
    let died = s.things.get(creature).and_then(|t| t.at);
    let key = yalain_content::DeathSpawn::of_creature(template);
    if let (Some(spawn), Some(died)) = (s.death_spawns.get(&key).cloned(), died) {
        let most = if spawn.at.is_some() { 1 } else { MOST };
        makes(s, net, (died, Some((key.as_str(), most))), spawn, clock);
    }
    let near = s
        .things
        .get(creature)
        .and_then(|t| t.at)
        .zip(s.players.get(&killer).map(|p| p.position))
        .and_then(|(died, at)| presence::distance(&died, &at))
        .is_some_and(|d| d <= NEAR_THE_DEATH);
    if !near {
        return;
    }
    for (name, _) in effects {
        if let Some(spawn) = s.death_spawns.get(name).cloned() {
            let Some(from) = s.players.get(&killer).map(|p| p.position) else {
                return;
            };
            makes(s, net, (from, Some((name.as_str(), MOST))), spawn, clock);
        }
    }
}

/// Does a template name for its use what a world's row makes?
#[must_use]
pub fn names_one<S: Store>(s: &Server<S>, props: &yalain_content::Props) -> bool {
    let (name, _) = crate::item_use::USED;
    (1..=8)
        .filter_map(|i| props.text(&format!("{name}{i}")))
        .any(|e| s.death_spawns.contains_key(e))
}

/// Can this world make everything a template's use names of a world's rows: creatures, or portals
/// to places it holds?
#[must_use]
pub fn can_make_all<S: Store>(s: &Server<S>, props: &yalain_content::Props) -> bool {
    let (name, _) = crate::item_use::USED;
    (1..=8)
        .filter_map(|i| props.text(&format!("{name}{i}")))
        .filter_map(|e| s.death_spawns.get(e))
        .all(|spawn| match s.catalog.get(spawn.template) {
            Some(t) if leads_anywhere(t, spawn) => {
                spawn.to.is_some() || portals::leads_to(s, t.id).is_some()
            }
            Some(t) => yalain_content::CreatureStats::from_props(&t.props).is_some(),
            None => false,
        })
}

/// Is what a row makes of a template a portal: its template names a destination, or the row names
/// a place it leads to and the template is no creature (an object a world's row sends its user on
/// from, as a placed object's `to` does)?
fn leads_anywhere(template: &yalain_content::Template, spawn: &yalain_content::DeathSpawn) -> bool {
    portals::is_portal(&template.props)
        || (spawn.to.is_some()
            && yalain_content::CreatureStats::from_props(&template.props).is_none())
}

/// What an item's use makes around its user, by the effects its template names.
pub fn used<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    user: ConnId,
    props: &yalain_content::Props,
    clock: Clock,
) {
    let (name, _) = crate::item_use::USED;
    for i in 1..=8 {
        let Some(effect) = props.text(&format!("{name}{i}")) else {
            continue;
        };
        if let Some(spawn) = s.death_spawns.get(effect).cloned() {
            let Some(from) = s.players.get(&user).map(|p| p.position) else {
                return;
            };
            makes(s, net, (from, None), spawn, clock);
        }
    }
}

/// What a row makes has stood its time: it goes, as a pet whose lifetime is over goes, with no fall
/// and no corpse (2017 shows none go so: all 6 dummies seen were killed within 71 s of their 90). One
/// killed already is left to its death's own course.
pub fn ends<S: Store>(s: &mut Server<S>, net: &mut ServerNet, id: u64) {
    if s.things.get(id).is_some_and(crate::things::Thing::alive) {
        crate::death::vanish(s, net, id);
    }
}

/// What a row makes about a place (a player's, or where a creature died), at its offset as the
/// place faces, or at the row's own place, up to `most` live (by name, the limit) if a limit holds.
fn makes<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    (from, most): (StoredPosition, Option<(&str, usize)>),
    spawn: yalain_content::DeathSpawn,
    clock: Clock,
) {
    let stored = |p: yalain_content::Place| StoredPosition {
        cell: p.cell,
        origin: p.origin,
        rotation: p.rotation,
    };
    let Some(template) = s.catalog.get(spawn.template).cloned() else {
        return;
    };
    if let Some((name, _)) = most {
        let things = &s.things;
        let live = s.death_spawned.entry(name.to_string()).or_default();
        // A creature lives until it dies; a portal until it goes.
        live.retain(|id| {
            things
                .get(*id)
                .is_some_and(|t| t.creature.as_ref().is_none_or(|c| c.mode != Mode::Dead))
        });
    }
    let portal = leads_anywhere(&template, &spawn);
    for _ in 0..spawn.count {
        let full = most.is_some_and(|(name, n)| s.death_spawned.get(name).map_or(0, Vec::len) >= n);
        if full || !s.rng.chance(spawn.chance) {
            continue;
        }
        let at = spawn
            .at
            .map_or_else(|| at_offset(s, &from, spawn.offset), stored);
        if portal {
            let made = portals::made(
                s,
                net,
                (spawn.template, spawn.to.map(stored)),
                at,
                (spawn.kingdom, spawn.lasts),
                spawn.name.as_ref(),
                clock,
            );
            if let (Some(id), Some((name, _))) = (made, most) {
                s.death_spawned
                    .entry(name.to_string())
                    .or_default()
                    .push(id);
            }
            continue;
        }
        let name = spawn.name.as_ref();
        let Some(id) = creature_at(s, net, &template, at, spawn.lasts, name, clock) else {
            s.unhandled.note(format!(
                "a death effect's template {:08X} is no creature",
                spawn.template
            ));
            return;
        };
        if let Some((name, _)) = most {
            s.death_spawned
                .entry(name.to_string())
                .or_default()
                .push(id);
        }
    }
}

/// Make a creature of a template at a place, facing as the place does, standing until killed or for
/// `lasts` seconds, named as a row names it. Returns its id; none for a template that is no
/// creature.
pub(crate) fn creature_at<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    template: &yalain_content::Template,
    at: StoredPosition,
    lasts: Option<f32>,
    name: Option<&yalain_content::Words>,
    clock: Clock,
) -> Option<u64> {
    let stats = yalain_content::CreatureStats::from_props(&template.props)?;
    let heading = crate::world::heading_of_rotation(at.rotation);
    let id = crate::spawn::make(s, template, stats, (at, heading), None, None, clock.local.0);
    if let (Some(name), Some(t)) = (name, s.things.get_mut(id)) {
        t.name_as(name);
    }
    if let Some(seconds) = lasts {
        s.after(
            clock,
            f64::from(seconds),
            crate::Timer::MadeEnds { creature: id },
        );
    }
    presence::thing_appears(s, net, id);
    crate::spawn::holds_its_effects(s, net, id, clock);
    Some(id)
}

/// A place at an offset from another, as that one faces, on the ground.
pub(crate) fn at_offset<S: Store>(
    s: &Server<S>,
    from: &StoredPosition,
    offset: [f32; 2],
) -> StoredPosition {
    let (x, y) = turned(from.rotation, offset);
    let (x, y) = (from.origin[0] + x, from.origin[1] + y);
    let (cell, x, y) = if presence::indoors(from.cell) {
        (from.cell, x, y)
    } else {
        crate::spawn::outdoor(from.cell >> 16, x, y)
    };
    let z = if presence::indoors(from.cell) {
        from.origin[2]
    } else {
        s.ground
            .as_ref()
            .and_then(|g| g.height(cell, [x, y, from.origin[2]]))
            .unwrap_or(from.origin[2])
    };
    StoredPosition {
        cell,
        origin: [x, y, z],
        rotation: from.rotation,
    }
}

/// An offset as a rotation about the vertical turns it.
fn turned(r: [f32; 4], [x, y]: [f32; 2]) -> (f32, f32) {
    let angle = 2.0 * math::atan2f(r[3], r[0]);
    let (sin, cos) = (math::sinf(angle), math::cosf(angle));
    (x * cos - y * sin, x * sin + y * cos)
}
