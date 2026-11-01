//! Vitals coming back.
//!
//! **Players** below their greatest health or vigor regain them in steps, one timer for both: each
//! step comes a fresh 3.5 to 7.5 s after the one before (a hit or a cost leaves it be), and gives
//! health, then vigor, the share of the greatest the time since the last step earns, rounded down;
//! nothing is sent for a vital at its greatest or that gained nothing. The greatest is the vital's
//! with the player's effects, as the client shows it.
//!
//! **Rates.** At rest the peace rates apply, in combat the combat rates. A player is in combat for
//! a step when it attacked or was hurt at any moment since the last step, or is moving at the
//! step itself. Each rate is a float quality as the player's effects make it (the quality plus
//! what they add, times what they multiply): the peace rates are 1 unless something changes them
//! (health then fills in a minute, vigor in half of one), the combat rates nothing unless an effect
//! gives them (a buff, a worn item's), so a player without one regains nothing in combat.
//!
//! **Creatures** not hurt for eight seconds regain 1.8% of their greatest health a second (seen
//! only through 2017's selection pushes; nothing is sent for them).

use yalain_net::ServerNet;
use yalain_store::Store;

use crate::player::quality;
use crate::{effects, progress, quals, Server};

/// The least and greatest time between a player's regeneration steps, in seconds.
pub const PLAYER_STEP: (f32, f32) = (3.5, 7.5);
/// The seconds at a rate of 1 that fill health and vigor from nothing.
pub const HEALTH_FILLS_IN: f64 = 60.0;
pub const VIGOR_FILLS_IN: f64 = 30.0;
/// The player's regeneration rates (float qualities): health and vigor at peace, and in combat
/// or moving; and the rates a player holds when it holds none of them.
pub mod rate {
    pub const PEACE_HEALTH: u32 = 262;
    pub const PEACE_VIGOR: u32 = 263;
    pub const COMBAT_HEALTH: u32 = 267;
    pub const COMBAT_VIGOR: u32 = 268;
    pub const PEACE: f32 = 1.0;
    pub const COMBAT: f32 = 0.0;
    /// The kind an effect names a float quality by (`kind << 16 | quality`).
    pub const FLOAT_KIND: u32 = 3;
}
/// Seconds a creature must go unhurt before it regains health, and the share a second.
pub const CREATURE_REST: f64 = 8.0;
pub const CREATURE_HEALTH_RATE: f64 = 0.018;

/// What one step gives a vital: its greatest times its rate and the seconds since the last step,
/// over the seconds a rate of 1 takes to fill it, rounded down.
#[must_use]
pub fn gain(greatest: i32, rate: f32, seconds: f64, fills_in: f64) -> i32 {
    dereth_primitives::num::to_i32_f64(
        (f64::from(greatest) * f64::from(rate) * seconds / fills_in).floor(),
    )
}

/// One of a player's regeneration rates as its effects make it: the quality (or the rate a player
/// holds without it) plus what its effects add, times what they multiply.
#[must_use]
pub fn rate_of<S: Store>(s: &Server<S>, p: &crate::world::Player, which: u32) -> f32 {
    let held = if which == rate::COMBAT_HEALTH || which == rate::COMBAT_VIGOR {
        rate::COMBAT
    } else {
        rate::PEACE
    };
    let base = quals::float(&p.qualities, which).unwrap_or(held);
    let (add, mult) = effects::modifier(
        &s.effect_table,
        &p.play.effects,
        rate::FLOAT_KIND << 16 | which,
    );
    (base + add) * mult
}

/// A vital as one step treats it: the vital, its greatest with effects (none when the player holds
/// no greatest), its peace and combat rates, and the seconds a rate of 1 takes to fill it.
type Vital = (u32, Option<i32>, f32, f32, f64);

/// The player's two vitals as its effects make them, health first.
fn vitals<S: Store>(s: &Server<S>, p: &crate::world::Player) -> [Vital; 2] {
    let greatest = |q: u32| quals::int(&p.qualities, q).map(|_| effects::max_vital(s, p, q));
    [
        (
            quality::HEALTH,
            greatest(quality::HEALTH_MAX),
            rate_of(s, p, rate::PEACE_HEALTH),
            rate_of(s, p, rate::COMBAT_HEALTH),
            HEALTH_FILLS_IN,
        ),
        (
            quality::VIGOR,
            greatest(quality::VIGOR_MAX),
            rate_of(s, p, rate::PEACE_VIGOR),
            rate_of(s, p, rate::COMBAT_VIGOR),
            VIGOR_FILLS_IN,
        ),
    ]
}

/// Regenerate what is due.
pub fn step<S: Store>(s: &mut Server<S>, net: &mut ServerNet, now: f64) {
    let worlds: Vec<_> = s.players.keys().copied().collect();
    for world in worlds {
        let Some(p) = s.players.get(&world) else {
            continue;
        };
        if p.dying.is_some() {
            // The dead regain nothing; their timer starts again when they come back.
            if let Some(p) = s.players.get_mut(&world) {
                p.next_regen = 0.0;
            }
            continue;
        }
        let due = p.next_regen == 0.0 || now >= p.next_regen;
        let wait = if due {
            f64::from(s.regen_rng.between(PLAYER_STEP.0, PLAYER_STEP.1))
        } else {
            0.0
        };
        let stepping = (due && p.next_regen != 0.0).then(|| vitals(s, p));
        let Some(p) = s.players.get_mut(&world) else {
            continue;
        };
        let health = quals::int(&p.qualities, quality::HEALTH).unwrap_or(0);
        if p.next_regen == 0.0 {
            // New in the world, or back from death: its timer starts.
            p.last_regen = now;
            p.next_regen = now + wait;
            p.regen_active = false;
            p.regen_health = health;
            continue;
        }
        // Fighting or being hurt at any moment puts the whole step at the combat rates.
        if p.attacking.is_some() || health < p.regen_health {
            p.regen_active = true;
        }
        p.regen_health = health;
        let Some(stepping) = stepping else {
            continue;
        };
        let dt = now - p.last_regen;
        // Moving counts only at the step itself.
        let active = p.regen_active || crate::skill_use::moving(p);
        p.last_regen = now;
        p.next_regen = now + wait;
        p.regen_active = false;
        for (cur, max, peace, combat, fills_in) in stepping {
            let Some(m) = max else {
                continue;
            };
            let c = quals::int(&p.qualities, cur).unwrap_or(m);
            // Vigor comes back from nothing (a recall spends all of it); health at none is death.
            if c >= m || (cur == quality::HEALTH && c <= 0) {
                continue;
            }
            let g = gain(m, if active { combat } else { peace }, dt, fills_in);
            if g <= 0 {
                continue;
            }
            let next = (c + g).min(m);
            quals::set_int(&mut p.qualities, cur, next);
            if cur == quality::HEALTH {
                p.regen_health = next;
            }
            progress::send_int(net, world, cur, next);
        }
    }
    for t in s.things.by_id.values_mut() {
        if !t.alive() || t.health >= t.max_health || now - t.last_hurt < CREATURE_REST {
            continue;
        }
        let gain = dereth_primitives::num::to_i32_f64(
            (f64::from(t.max_health) * CREATURE_HEALTH_RATE * crate::spawn::STEP_SECONDS).ceil(),
        );
        t.health = (t.health + gain.max(1)).min(t.max_health);
    }
}
