//! Swimming: a player outdoors whose feet are 1.25 m or more under its block's water surface swims
//! (2017's swimmers floated 1.25 to 2 m under it: 3,710 of 4,554 places in deep water).
//!
//! **What it costs.** While it swims a player regains nothing. At each of its regeneration steps
//! ([`crate::regen`]) it loses vigor instead: its greatest's share that the time since the step
//! before earns at a thirtieth a second (all of it in 30 s), rounded down; with no vigor left,
//! health at a hundred and twentieth a second (2017: vigor a median 3.32% of the greatest a second
//! over 90 swims, health 0.82% over 10 swims with no vigor left). Swimming Mastery takes its perk's
//! share off both (Turbine, May 2005: "Less health and vigor is lost when swimming", 25% and 50%;
//! its three records hold 0.25, 0.5 and 0.6). With no health left it drowns.

use yalain_net::{ConnId, ServerNet};
use yalain_store::Store;

use crate::player::quality;
use crate::{progress, quals, Server};

/// How far under its block's water surface a player's feet are when it swims, in meters.
pub const UNDER: f32 = 1.25;
/// The seconds swimming takes to empty vigor, and health once vigor is gone.
pub const VIGOR_EMPTIES_IN: f64 = 30.0;
pub const HEALTH_EMPTIES_IN: f64 = 120.0;

/// Whether a player swims, and if so the share of what swimming costs that its Swimming Mastery
/// takes off (0 without the perk). One in portal space, indoors or on a block without water does
/// not swim.
#[must_use]
pub fn swims<S: Store>(s: &Server<S>, p: &crate::world::Player) -> Option<f32> {
    if p.trip.is_some() || p.position.cell & 0xFFFF >= 0x100 {
        return None;
    }
    let surface = s.ground.as_ref()?.water(p.position.cell)?;
    (p.position.origin[2] <= surface - UNDER).then(|| mastery(s, p))
}

/// The share of what swimming costs that a player's Swimming Mastery takes off.
fn mastery<S: Store>(s: &Server<S>, p: &crate::world::Player) -> f32 {
    let starting = s.start.skills.get(&p.race).cloned().unwrap_or_default();
    let skills = crate::skills::held(&starting, &p.play.skills);
    #[allow(clippy::cast_possible_truncation)]
    crate::skills::perk_value(&s.skill_table, &skills, crate::skills::perk_kind::SWIMMING)
        .map_or(0.0, |v| (v as f32).clamp(0.0, 1.0))
}

/// What one step of swimming takes from a vital: its greatest times the seconds since the step
/// before, over the seconds swimming takes to empty it, less the share `cut` takes off, rounded
/// down.
#[must_use]
pub fn loss(greatest: i32, seconds: f64, empties_in: f64, cut: f32) -> i32 {
    dereth_primitives::num::to_i32_f64(
        (f64::from(greatest) * seconds / empties_in * f64::from(1.0 - cut)).floor(),
    )
}

/// A swimming player's regeneration step, `seconds` after the one before: vigor lost while it has
/// any, else health, each sent to it. `greatest` is its greatest health and vigor with effects.
/// Returns whether it has no health left.
pub(crate) fn step(
    p: &mut crate::world::Player,
    net: &mut ServerNet,
    world: ConnId,
    greatest: (Option<i32>, Option<i32>),
    seconds: f64,
    cut: f32,
) -> bool {
    let vigor = quals::int(&p.qualities, quality::VIGOR).unwrap_or(0);
    let (vital, empties_in, greatest, now) = if vigor > 0 {
        (quality::VIGOR, VIGOR_EMPTIES_IN, greatest.1, vigor)
    } else {
        let health = quals::int(&p.qualities, quality::HEALTH).unwrap_or(0);
        (quality::HEALTH, HEALTH_EMPTIES_IN, greatest.0, health)
    };
    let Some(m) = greatest else {
        return false;
    };
    let lost = loss(m, seconds, empties_in, cut);
    if lost <= 0 || now <= 0 {
        return false;
    }
    // An operator who may not die keeps the health it may not lose.
    let least = if vital == quality::HEALTH {
        i32::from(p.admin.never_say_die)
    } else {
        0
    };
    let next = (now - lost).max(least.min(now));
    quals::set_int(&mut p.qualities, vital, next);
    if vital == quality::HEALTH {
        p.regen_health = next;
    }
    progress::send_int(net, world, vital, next);
    vital == quality::HEALTH && next <= 0
}
