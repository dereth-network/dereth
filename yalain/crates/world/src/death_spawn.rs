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

use dereth_primitives::num::math;
use yalain_net::{ConnId, ServerNet};
use yalain_store::{Store, StoredPosition};

use crate::effect_defs::Kind;
use crate::effects::{Cast, On};
use crate::things::Mode;
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
    effects: &[String],
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
    for name in effects {
        if let Some(spawn) = s.death_spawns.get(name).copied() {
            let most = Some((name.as_str(), MOST));
            makes(s, net, killer, most, spawn, clock);
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
        if let Some(spawn) = s.death_spawns.get(effect).copied() {
            makes(s, net, user, None, spawn, clock);
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

/// What a row makes around a player, up to `most` live (by name, the limit) if a limit holds.
fn makes<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    killer: ConnId,
    most: Option<(&str, usize)>,
    spawn: yalain_content::DeathSpawn,
    clock: Clock,
) {
    let Some(killer_at) = s.players.get(&killer).map(|p| p.position) else {
        return;
    };
    let Some(template) = s.catalog.get(spawn.template).cloned() else {
        return;
    };
    if let Some((name, _)) = most {
        let things = &s.things;
        let live = s.death_spawned.entry(name.to_string()).or_default();
        live.retain(|id| {
            things
                .get(*id)
                .and_then(|t| t.creature.as_ref())
                .is_some_and(|c| c.mode != Mode::Dead)
        });
    }
    for _ in 0..spawn.count {
        let full = most.is_some_and(|(name, n)| s.death_spawned.get(name).map_or(0, Vec::len) >= n);
        if full || !s.rng.chance(spawn.chance) {
            continue;
        }
        let Some(stats) = yalain_content::CreatureStats::from_props(&template.props) else {
            s.unhandled.note(format!(
                "a death effect's template {:08X} is no creature",
                spawn.template
            ));
            return;
        };
        let at = at_offset(s, &killer_at, spawn.offset);
        let heading = crate::world::heading_of_rotation(at.rotation);
        let id = crate::spawn::make(
            s,
            &template,
            stats,
            (at, heading),
            None,
            None,
            clock.local.0,
        );
        if let Some((name, _)) = most {
            s.death_spawned
                .entry(name.to_string())
                .or_default()
                .push(id);
        }
        if let Some(seconds) = spawn.lasts {
            s.after(
                clock,
                f64::from(seconds),
                crate::Timer::MadeEnds { creature: id },
            );
        }
        presence::thing_appears(s, net, id);
    }
}

/// A place at an offset from another, as that one faces, on the ground.
fn at_offset<S: Store>(s: &Server<S>, from: &StoredPosition, offset: [f32; 2]) -> StoredPosition {
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
