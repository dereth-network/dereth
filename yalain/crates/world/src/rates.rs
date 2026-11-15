//! The operator's game rates: how much experience a player earns, how much treasure a creature or a
//! chest draws, how soon a camp makes a replacement and how fast a player moves, each a multiple of
//! what the game gives. Each is 1 unless the operator gives another, and at 1 the game is as it is
//! without them: the award is the game's own, the draw the profile's, the period the camp's, the
//! speed the effects', no die is cast for them and nothing more is sent.
//!
//! - **Experience** multiplies what a player earns: its part of a kill and a fellow's share of one,
//!   a quest's award and a quest step's, rounded to the nearest. What vassals pass up to a patron
//!   follows from what they earned, so it is not multiplied again; an admin's award is the number
//!   given.
//! - **Loot** multiplies how many items a creature's or a chest's treasure profile draws: a part of
//!   an item is a chance of one more. A creature's fixed drops, its profile's own extras and the
//!   death treasures (a quest's items among them) are drawn as they are, and a corpse or a chest
//!   holds no more than it can.
//! - **Respawn** divides a camp's period: at 2 a dead creature's replacement, or an item taken
//!   from where it lay, comes back in half the time.
//! - **Movement** multiplies a player's speed and acceleration scales, the ones its effects give
//!   it, run and walk, mounted or swimming alike: at 2 a player goes twice as far in a second, and
//!   comes to that speed as soon as to its own at 1. Its pets keep up, as they follow at their
//!   owner's speed; creatures are as fast as they were. The reach a reported place is checked
//!   against grows with it.

use crate::rng::Rng;

/// The highest rate the operator may give.
pub const MOST: f64 = 100.0;
/// The highest movement rate.
pub const MOST_MOVEMENT: f64 = 10.0;

/// The operator's game rates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rates {
    pub experience: f64,
    pub loot: f64,
    pub respawn: f64,
    pub movement: f64,
}

impl Default for Rates {
    fn default() -> Self {
        Self {
            experience: 1.0,
            loot: 1.0,
            respawn: 1.0,
            movement: 1.0,
        }
    }
}

impl Rates {
    /// Each rate by its name, in order.
    #[must_use]
    pub fn all(&self) -> [(&'static str, f64); 4] {
        [
            ("experience", self.experience),
            ("loot", self.loot),
            ("respawn", self.respawn),
            ("movement", self.movement),
        ]
    }

    /// The rates the operator set to something other than 1, by name.
    #[must_use]
    pub fn changed(&self) -> Vec<(&'static str, f64)> {
        self.all()
            .into_iter()
            .filter(|(_, r)| !is_one(*r))
            .collect()
    }
}

/// Whether a rate is 1, the game's own.
#[must_use]
pub fn is_one(rate: f64) -> bool {
    (rate - 1.0).abs() < f64::EPSILON
}

/// A rate as the operator gives it, checked: a number from 0 to [`MOST`]; above 0 for respawn,
/// whose period it divides; above 0 and at most [`MOST_MOVEMENT`] for movement.
///
/// # Errors
/// It is no number, or out of bounds.
pub fn read(name: &str, text: &str) -> Result<f64, String> {
    let rate: f64 = text
        .trim()
        .parse()
        .map_err(|e| format!("a {name} rate: {e}"))?;
    let least_above = name == "respawn" || name == "movement";
    let most = if name == "movement" {
        MOST_MOVEMENT
    } else {
        MOST
    };
    if !rate.is_finite() || rate < 0.0 || rate > most || (least_above && rate == 0.0) {
        let from = if least_above { "above 0" } else { "0" };
        return Err(format!(
            "a {name} rate is a number from {from} to {most}, not {text}"
        ));
    }
    Ok(rate)
}

/// Experience earned, at the experience rate.
#[must_use]
pub fn experience(rate: f64, amount: u64) -> u64 {
    if is_one(rate) {
        return amount;
    }
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    let earned = (amount as f64 * rate).round() as u64;
    earned
}

/// How many items a treasure profile's draw makes, at the loot rate: the count times the rate,
/// and a part of one a chance of one more. A whole number of items casts no die, so at 1 the draw
/// is the count, as without the rate.
#[must_use]
pub fn loot_count(rng: &mut Rng, rate: f64, count: usize) -> usize {
    #[allow(clippy::cast_precision_loss)]
    let wanted = count as f64 * rate;
    let whole = wanted.floor();
    #[allow(clippy::cast_possible_truncation)]
    let part = (wanted - whole) as f32;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let whole = whole as usize;
    whole + usize::from(part > 0.0 && rng.chance(part))
}

/// A camp's period, at the respawn rate.
#[must_use]
pub fn period(rate: f64, seconds: f64) -> f64 {
    if is_one(rate) {
        return seconds;
    }
    seconds / rate
}

/// A player's speed or acceleration scale, its effects' `speed`, at the movement rate.
#[must_use]
pub fn speed(rate: f64, speed: f32) -> f32 {
    if is_one(rate) {
        return speed;
    }
    #[allow(clippy::cast_possible_truncation)]
    let rate = rate as f32;
    speed * rate
}

/// How much farther than at the game's own speed a reported place may lie, at the movement rate: the
/// rate, and never less than 1, so a slower world is checked as the game's.
#[must_use]
pub fn reach(rate: f64) -> f64 {
    rate.max(1.0)
}
