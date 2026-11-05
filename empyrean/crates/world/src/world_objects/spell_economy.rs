//! Not ACE: the world-wide value on each spell, on a world with
//! [`EraFeatures::spell_economy`](empyrean_common::era::EraFeatures).
//!
//! DIVERGE: V503. Every number here but the band is ac1999's reconstruction, not a known retail
//! value: they are together so they can change together. The band is the range the 1999 client's
//! spell panels state. Values rest at the top, are spent by successful casts, regain over hours,
//! are worked out only when read and are not saved: a restart rests every spell.

use std::collections::HashMap;

/// The least a spell's value can be, from the 1999 panels' range.
pub const MIN: f32 = 0.75;
/// The most, and where a spell rests.
pub const MAX: f32 = 1.25;
/// What one successful cast spends with one player in the world.
pub const DRAIN_PER_CAST: f32 = 0.005;
/// How long the whole range takes to regain.
pub const RECOVERY_SECONDS: f64 = 8.0 * 60.0 * 60.0;

/// The line a cast gets, by the value it used: the lowest value each holds from, strongest first.
/// The first is seen in a retail screenshot. The other four, and all the thresholds, are ac1999's
/// (with the screenshot's "within" for its "through").
pub const LINES: [(f32, &str); 4] = [
    (1.20, "The magic blazes within you!"),
    (1.10, "The magic rushes mightily within you."),
    (0.90, "The magic courses strongly within you."),
    (0.80, "The magic surges within you."),
];
/// The line below all of them.
pub const LOWEST_LINE: &str = "The magic flows within you.";

/// The line for a value.
#[must_use]
pub fn line(value: f32) -> &'static str {
    LINES
        .iter()
        .find(|(from, _)| value >= *from)
        .map_or(LOWEST_LINE, |(_, text)| text)
}

/// One spell's value and when it was true.
#[derive(Debug, Clone, Copy)]
struct Entry {
    value: f32,
    at: f64,
}

/// The world's values. A spell not here is resting.
#[derive(Debug, Default)]
pub struct SpellEconomy {
    spells: HashMap<u32, Entry>,
}

impl SpellEconomy {
    /// A spell's value at `now`.
    #[must_use]
    pub fn value(&self, spell: u32, now: f64) -> f32 {
        self.spells.get(&spell).map_or(MAX, |e| {
            let per_second = f64::from(MAX - MIN) / RECOVERY_SECONDS;
            #[allow(clippy::cast_possible_truncation)]
            let regained = (per_second * (now - e.at).max(0.0)) as f32;
            (e.value + regained).clamp(MIN, MAX)
        })
    }

    /// A successful cast of `spell`: `economy_mod` is the spell's own weight, and the spend is shared
    /// among the players in the world.
    pub fn spend(&mut self, spell: u32, economy_mod: f32, active_players: usize, now: f64) {
        let current = self.value(spell, now);
        #[allow(clippy::cast_precision_loss)]
        let drain = DRAIN_PER_CAST * economy_mod / active_players.max(1) as f32;
        self.spells.insert(
            spell,
            Entry {
                value: (current - drain).clamp(MIN, MAX),
                at: now,
            },
        );
    }
}
